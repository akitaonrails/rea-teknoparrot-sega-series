//! A zip archive whose entries are stored, not compressed.
//!
//! Flycast loads a NAOMI cartridge only from an archive named after its
//! MAME set (`f355twn2.zip`), and the TeknoParrot dump keeps the chips as
//! loose files. The launcher packs them into the runtime once: stored
//! entries make that a plain copy, and Flycast reads them without
//! inflating anything.

use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// 1980-01-01 00:00 in DOS time: the archive doesn't depend on when it
/// was made.
const DOS_DATE: u16 = (1 << 5) | 1;
const DOS_TIME: u16 = 0;

/// Writes `files` (by their file names) into a new archive at `to`.
pub fn write(files: &[PathBuf], to: &Path) -> io::Result<()> {
    let mut out = BufWriter::new(File::create(to)?);
    let mut central = Vec::new();
    for f in files {
        let name = f
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| io::Error::other(format!("{}: not a plain name", f.display())))?;
        let offset = u32_of(out.stream_position()?)?;
        let header_at = out.stream_position()?;
        // the header, then the data; its CRC and sizes are filled in after
        local_header(&mut out, name, 0, 0)?;
        let mut crc = crc32fast::Hasher::new();
        let mut size: u64 = 0;
        let mut src = File::open(f)?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            crc.update(&buf[..n]);
            out.write_all(&buf[..n])?;
            size += n as u64;
        }
        let (crc, size) = (crc.finalize(), u32_of(size)?);
        let end = out.stream_position()?;
        out.seek(SeekFrom::Start(header_at))?;
        local_header(&mut out, name, crc, size)?;
        out.seek(SeekFrom::Start(end))?;
        central.push((name.to_string(), crc, size, offset));
    }
    let dir_at = u32_of(out.stream_position()?)?;
    for (name, crc, size, offset) in &central {
        out.write_all(&0x0201_4b50u32.to_le_bytes())?;
        out.write_all(&20u16.to_le_bytes())?; // made by
        out.write_all(&10u16.to_le_bytes())?; // needed to extract
        out.write_all(&0u16.to_le_bytes())?; // flags
        out.write_all(&0u16.to_le_bytes())?; // stored
        out.write_all(&DOS_TIME.to_le_bytes())?;
        out.write_all(&DOS_DATE.to_le_bytes())?;
        out.write_all(&crc.to_le_bytes())?;
        out.write_all(&size.to_le_bytes())?;
        out.write_all(&size.to_le_bytes())?;
        out.write_all(&(name.len() as u16).to_le_bytes())?;
        out.write_all(&[0u8; 12])?; // extra, comment, disk, attributes
        out.write_all(&offset.to_le_bytes())?;
        out.write_all(name.as_bytes())?;
    }
    let dir_size = u32_of(out.stream_position()?)? - dir_at;
    let count = u16::try_from(central.len()).map_err(|_| io::Error::other("too many files"))?;
    out.write_all(&0x0605_4b50u32.to_le_bytes())?;
    out.write_all(&[0u8; 4])?; // disk numbers
    out.write_all(&count.to_le_bytes())?;
    out.write_all(&count.to_le_bytes())?;
    out.write_all(&dir_size.to_le_bytes())?;
    out.write_all(&dir_at.to_le_bytes())?;
    out.write_all(&0u16.to_le_bytes())?; // comment
    out.into_inner().map_err(|e| e.into_error())?.sync_all()
}

fn local_header(out: &mut impl Write, name: &str, crc: u32, size: u32) -> io::Result<()> {
    out.write_all(&0x0403_4b50u32.to_le_bytes())?;
    out.write_all(&10u16.to_le_bytes())?;
    out.write_all(&0u16.to_le_bytes())?;
    out.write_all(&0u16.to_le_bytes())?;
    out.write_all(&DOS_TIME.to_le_bytes())?;
    out.write_all(&DOS_DATE.to_le_bytes())?;
    out.write_all(&crc.to_le_bytes())?;
    out.write_all(&size.to_le_bytes())?;
    out.write_all(&size.to_le_bytes())?;
    out.write_all(&(name.len() as u16).to_le_bytes())?;
    out.write_all(&0u16.to_le_bytes())?;
    out.write_all(name.as_bytes())
}

fn u32_of(n: u64) -> io::Result<u32> {
    u32::try_from(n).map_err(|_| io::Error::other("a file over 4 GB doesn't fit a plain zip"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn the_archive_lists_and_checks_with_unzip() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("epr-1.ic22");
        let b = dir.path().join("key.bin");
        std::fs::write(&a, vec![7u8; 300_000]).unwrap();
        std::fs::write(&b, b"\x01\x02\x03\x04").unwrap();
        let zip = dir.path().join("set.zip");
        write(&[a, b], &zip).unwrap();
        let bytes = std::fs::read(&zip).unwrap();
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        assert_eq!(&bytes[bytes.len() - 22..bytes.len() - 18], b"PK\x05\x06");
        // the real tool, when it is installed (STANDARDS "Test behaviour")
        match Command::new("unzip").arg("-tq").arg(&zip).output() {
            Ok(o) => assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stdout)),
            Err(_) => eprintln!("unzip not installed: skipped the round trip"),
        }
        if let Ok(o) = Command::new("unzip").arg("-Z1").arg(&zip).output() {
            assert_eq!(String::from_utf8_lossy(&o.stdout), "epr-1.ic22\nkey.bin\n");
        }
    }

    #[test]
    fn an_empty_list_is_an_empty_archive() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("none.zip");
        write(&[], &zip).unwrap();
        assert_eq!(std::fs::metadata(&zip).unwrap().len(), 22);
    }
}
