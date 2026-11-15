//! The Windows executable's version information, icon and manifest
//! (rea-launcher-build): a named, versioned program with the launcher's
//! icon, instead of an anonymous one that antivirus heuristics distrust.
fn main() {
    rea_launcher_build::Resources::new(
        "Sega Racing Launcher",
        "Launcher for the Sega arcade racers on modern systems",
    )
    .company("AkitaOnRails")
    .file_name("sega-launcher.exe")
    .icon("../../packaging/appimage/sega-launcher.png")
    .embed()
    .unwrap();
}
