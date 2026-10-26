use std::path::Path;

use winres::{VersionInfo, WindowsResource};

fn main() {
    let version = env!("CARGO_PKG_VERSION");

    // Embed the icon and richer PE version metadata into the executable.
    let mut res = WindowsResource::new();
    let numeric_version = pack_version(version);

    res.set_icon("src/icons/icon.ico")
        .set("FileVersion", version)
        .set("ProductVersion", version)
        .set_version_info(VersionInfo::FILEVERSION, numeric_version)
        .set_version_info(VersionInfo::PRODUCTVERSION, numeric_version);

    if let Some(toolkit_path) = local_windows_sdk_toolkit_path() {
        res.set_toolkit_path(&toolkit_path);
    }

    res.compile().expect("Failed to compile Windows resources");
}

fn local_windows_sdk_toolkit_path() -> Option<String> {
    if let Ok(path) = std::env::var("WINRES_TOOLKIT_PATH") {
        if Path::new(&path).join("rc.exe").exists() {
            return Some(path);
        }
    }

    [
        r"C:\Program Files\Microsoft Visual Studio\2022\Professional\SDK\ScopeCppSDK\vc15\SDK\bin",
        r"C:\Program Files\Microsoft Visual Studio\18\Professional\SDK\ScopeCppSDK\vc15\SDK\bin",
    ]
    .into_iter()
    .find(|path| Path::new(path).join("rc.exe").exists())
    .map(str::to_string)
}

fn pack_version(version: &str) -> u64 {
    let core = version.split('-').next().unwrap_or(version);
    let mut parts = core.split('.').map(|part| part.parse::<u64>().unwrap_or(0));

    let major = parts.next().unwrap_or(0).min(u16::MAX as u64);
    let minor = parts.next().unwrap_or(0).min(u16::MAX as u64);
    let patch = parts.next().unwrap_or(0).min(u16::MAX as u64);

    (major << 48) | (minor << 32) | (patch << 16)
}
