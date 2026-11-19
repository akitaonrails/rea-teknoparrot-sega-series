//! Packs an extracted Sega Chihiro game folder into a FATX netboot `.bin`
//! image for xemu's Chihiro machine (`Tovarichtch/xemu`).
//!
//! TeknoParrot Chihiro dumps are loose directories (`boot.id`, `*.xbe`,
//! `Media/`); xemu's Chihiro media board mounts a FATX filesystem image.
//! Packing streams each file in 16 KiB clusters directly to disk without
//! buffering the whole image in memory, and skips rebuilding while the
//! source tree's file list and sizes are unchanged.

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use rea_launcher::linkfarm;

const SECTOR: usize = 512;
const SPC: u32 = 32;
const CLUSTER: usize = (SPC as usize) * SECTOR; // 16 KiB
const PAGE: usize = 4096;

/// 512 MB DIMM offers 496 MiB of game filesystem (`mbcom_base = 0xF8000`);
/// 1 GB DIMM offers 1008 MiB (`mbcom_base = 0x1F8000`).
const FS_512M: u64 = 496 * 1024 * 1024;
const FS_1G: u64 = 1008 * 1024 * 1024;

const ATTR_DIR: u8 = 0x10;
const ATTR_FILE: u8 = 0x00;
const MAX_NAME: usize = 42;

enum Chunk {
    Dir(Vec<u8>),
    File { path: PathBuf, size: u64 },
}

struct Builder {
    next_cluster: u32,
    /// `(first_cluster, count, chunk)` in ascending cluster order.
    chunks: Vec<(u32, u32, Chunk)>,
    /// Manifest lines (`rel_path\tsize`) for up-to-date checks.
    manifest: Vec<String>,
}

impl Builder {
    fn new() -> Self {
        Self {
            next_cluster: 1,
            chunks: Vec::new(),
            manifest: Vec::new(),
        }
    }

    fn alloc_file(&mut self, path: PathBuf, size: u64) -> io::Result<u32> {
        if size == 0 {
            return Ok(0);
        }
        let count = size.div_ceil(CLUSTER as u64);
        let count = u32::try_from(count)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "file too large for FATX"))?;
        let first = self.next_cluster;
        self.next_cluster = self
            .next_cluster
            .checked_add(count)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "FATX cluster overflow"))?;
        self.chunks.push((first, count, Chunk::File { path, size }));
        Ok(first)
    }

    fn alloc_dir(&mut self, data: Vec<u8>) -> io::Result<u32> {
        let count = (data.len() / CLUSTER) as u32;
        let first = self.next_cluster;
        self.next_cluster = self
            .next_cluster
            .checked_add(count)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "FATX cluster overflow"))?;
        self.chunks.push((first, count, Chunk::Dir(data)));
        Ok(first)
    }

    fn walk(&mut self, dir: &Path, rel_prefix: &str, top: bool) -> io::Result<u32> {
        let mut items: Vec<(String, PathBuf, bool, u64)> = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if linkfarm::ntfs_leftover(&name) {
                continue;
            }
            if top && name.eq_ignore_ascii_case("teknoparrot.ini") {
                continue;
            }
            let path = entry.path();
            let meta = fs::metadata(&path)?;
            items.push((name, path, meta.is_dir(), meta.len()));
        }
        items.sort_by(|a, b| a.0.cmp(&b.0));

        let mut dir_bytes = Vec::new();
        for (name, path, is_dir, size) in items {
            let rel = if rel_prefix.is_empty() {
                name.clone()
            } else {
                format!("{rel_prefix}/{name}")
            };
            if !name.is_ascii() || name.len() > MAX_NAME {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("FATX name must be <= {MAX_NAME} ASCII bytes: {rel}"),
                ));
            }
            let (first, attr, entry_size) = if is_dir {
                let c = self.walk(&path, &rel, false)?;
                (c, ATTR_DIR, 0u32)
            } else {
                let entry_size = u32::try_from(size).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("file exceeds 4 GiB FATX limit: {rel}"),
                    )
                })?;
                self.manifest.push(format!("{rel}\t{size}"));
                let c = self.alloc_file(path, size)?;
                (c, ATTR_FILE, entry_size)
            };
            let mut ent = [0u8; 64];
            ent[0] = name.len() as u8;
            ent[1] = attr;
            ent[2..2 + name.len()].copy_from_slice(name.as_bytes());
            ent[2 + name.len()..44].fill(0xFF);
            ent[0x2C..0x30].copy_from_slice(&first.to_le_bytes());
            ent[0x30..0x34].copy_from_slice(&entry_size.to_le_bytes());
            dir_bytes.extend_from_slice(&ent);
        }

        // Pad directory stream to a whole cluster with 0xFF (end-of-directory sentinel).
        let rem = dir_bytes.len() % CLUSTER;
        if rem != 0 || dir_bytes.is_empty() {
            dir_bytes.resize(dir_bytes.len() + (CLUSTER - rem), 0xFF);
        }
        self.alloc_dir(dir_bytes)
    }
}

fn manifest_path(dst: &Path) -> PathBuf {
    let mut name = dst.as_os_str().to_os_string();
    name.push(".manifest");
    PathBuf::from(name)
}

/// Packs `src` into a FATX `.bin` image at `dst` if `dst` is missing or any
/// source file changed in name or size. `src` is only ever read.
pub fn pack_dir(src: &Path, dst: &Path) -> io::Result<()> {
    let mut b = Builder::new();
    let root = b.walk(src, "", true)?;
    let manifest = b.manifest.join("\n");
    let mpath = manifest_path(dst);
    if dst.is_file() && fs::read_to_string(&mpath).ok().as_deref() == Some(manifest.as_str()) {
        return Ok(());
    }

    let used_clusters = (b.next_cluster - 1) as u64;
    let fs_size = if used_clusters * (CLUSTER as u64) + (PAGE as u64) * 17 <= FS_512M {
        FS_512M
    } else if used_clusters * (CLUSTER as u64) + (PAGE as u64) * 65 <= FS_1G {
        FS_1G
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Chihiro game folder exceeds 1008 MiB DIMM limit",
        ));
    };

    let entries = (fs_size / (CLUSTER as u64) + 1) as usize;
    let fat16 = entries < 0xFFF0;
    let entry_bytes = if fat16 { 2 } else { 4 };
    let fat_size = (entries * entry_bytes).div_ceil(PAGE) * PAGE;

    let mut fat = vec![0u8; fat_size];
    if fat16 {
        fat[0..2].copy_from_slice(&0xFFF8u16.to_le_bytes());
    } else {
        fat[0..4].copy_from_slice(&0xFFFF_FFF8u32.to_le_bytes());
    }
    for &(first, count, _) in &b.chunks {
        for i in 0..count {
            let c = (first + i) as usize;
            if fat16 {
                let val: u16 = if i + 1 < count {
                    (first + i + 1) as u16
                } else {
                    0xFFFF
                };
                fat[c * 2..c * 2 + 2].copy_from_slice(&val.to_le_bytes());
            } else {
                let val: u32 = if i + 1 < count {
                    first + i + 1
                } else {
                    0xFFFF_FFFF
                };
                fat[c * 4..c * 4 + 4].copy_from_slice(&val.to_le_bytes());
            }
        }
    }

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut part_name = dst.as_os_str().to_os_string();
    part_name.push(".part");
    let part = PathBuf::from(part_name);

    {
        let mut out = BufWriter::with_capacity(256 * 1024, File::create(&part)?);
        let mut hdr = [0u8; PAGE];
        hdr[0..4].copy_from_slice(b"FATX");
        hdr[4..8].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        hdr[8..12].copy_from_slice(&SPC.to_le_bytes());
        hdr[12..16].copy_from_slice(&root.to_le_bytes());
        out.write_all(&hdr)?;
        out.write_all(&fat)?;

        let mut buf = vec![0u8; 64 * 1024];
        let zero = vec![0u8; CLUSTER];
        for (_, count, chunk) in &b.chunks {
            match chunk {
                Chunk::Dir(bytes) => out.write_all(bytes)?,
                Chunk::File { path, size } => {
                    let mut f = File::open(path)?;
                    let mut written = 0u64;
                    loop {
                        let n = f.read(&mut buf)?;
                        if n == 0 {
                            break;
                        }
                        out.write_all(&buf[..n])?;
                        written += n as u64;
                    }
                    if written != *size {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            format!("{} changed size while packing", path.display()),
                        ));
                    }
                    let total = (*count as u64) * (CLUSTER as u64);
                    let pad = (total - written) as usize;
                    if pad > 0 {
                        out.write_all(&zero[..pad])?;
                    }
                }
            }
        }
        out.flush()?;
    }

    fs::rename(&part, dst)?;
    fs::write(&mpath, manifest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_le(b: &[u8], off: usize) -> u32 {
        u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
    }

    fn u16_le(b: &[u8], off: usize) -> u16 {
        u16::from_le_bytes(b[off..off + 2].try_into().unwrap())
    }

    #[test]
    fn packs_nested_tree_into_valid_fatx_image_and_skips_teknoparrot_ini() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("dump");
        fs::create_dir_all(src.join("Media")).unwrap();
        let big = vec![0xABu8; CLUSTER + 123]; // spans 2 clusters
        fs::write(src.join("boot.id"), b"BTID_TEST").unwrap();
        fs::write(src.join("empty.dat"), b"").unwrap();
        fs::write(src.join("teknoparrot.ini"), b"skip me").unwrap();
        fs::write(src.join("Media/data.bin"), &big).unwrap();

        let dst = tmp.path().join("rt/game.bin");
        pack_dir(&src, &dst).unwrap();

        let img = fs::read(&dst).unwrap();
        assert_eq!(&img[0..4], b"FATX");
        assert_eq!(u32_le(&img, 8), SPC);
        let root = u32_le(&img, 12);
        assert!(root >= 1);

        let entries = (FS_512M / (CLUSTER as u64) + 1) as usize;
        let fat_size = (entries * 2).div_ceil(PAGE) * PAGE;
        let data_off = PAGE + fat_size;

        // Read root directory entries
        let root_off = data_off + ((root - 1) as usize) * CLUSTER;
        let mut found = Vec::new();
        for i in (0..CLUSTER).step_by(64) {
            let e = &img[root_off + i..root_off + i + 64];
            if e[0] == 0 || e[0] == 0xFF {
                break;
            }
            let name = std::str::from_utf8(&e[2..2 + e[0] as usize]).unwrap();
            let attr = e[1];
            let first = u32_le(e, 0x2C);
            let size = u32_le(e, 0x30);
            found.push((name.to_string(), attr, first, size));
        }
        assert_eq!(found.len(), 3, "teknoparrot.ini skipped: {found:?}");
        assert!(
            found
                .iter()
                .any(|(n, a, _, _)| n == "Media" && *a == ATTR_DIR)
        );
        let (_, _, boot_c, boot_sz) = found.iter().find(|(n, _, _, _)| n == "boot.id").unwrap();
        assert_eq!(*boot_sz, 9);
        let boot_off = data_off + ((*boot_c - 1) as usize) * CLUSTER;
        assert_eq!(&img[boot_off..boot_off + 9], b"BTID_TEST");

        // Follow Media/data.bin's 2-cluster FAT16 chain
        let (_, _, media_c, _) = found.iter().find(|(n, _, _, _)| n == "Media").unwrap();
        let media_off = data_off + ((*media_c - 1) as usize) * CLUSTER;
        let de = &img[media_off..media_off + 64];
        assert_eq!(&de[2..2 + de[0] as usize], b"data.bin");
        let c1 = u32_le(de, 0x2C) as usize;
        let sz = u32_le(de, 0x30) as usize;
        assert_eq!(sz, big.len());
        let c2 = u16_le(&img, PAGE + c1 * 2) as usize;
        assert_eq!(c2, c1 + 1);
        assert_eq!(u16_le(&img, PAGE + c2 * 2), 0xFFFF);
        let mut read_back = Vec::new();
        read_back.extend_from_slice(&img[data_off + (c1 - 1) * CLUSTER..data_off + c1 * CLUSTER]);
        read_back.extend_from_slice(
            &img[data_off + (c2 - 1) * CLUSTER..data_off + (c2 - 1) * CLUSTER + 123],
        );
        assert_eq!(read_back, big);

        // Idempotent when unchanged, rebuilds when a file changes
        let m1 = fs::metadata(&dst).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        pack_dir(&src, &dst).unwrap();
        assert_eq!(fs::metadata(&dst).unwrap().modified().unwrap(), m1);
        fs::write(src.join("boot.id"), b"BTID_TEST_V2").unwrap();
        pack_dir(&src, &dst).unwrap();
        let img2 = fs::read(&dst).unwrap();
        assert!(img2.windows(12).any(|w| w == b"BTID_TEST_V2"));
    }
}
