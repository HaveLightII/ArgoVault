use super::{HOST, ORIGIN, directory, write_private};
use serde_json::json;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const EXTENSION: &[(&str, &[u8])] = &[
    (
        "manifest.json",
        include_bytes!("../../../browser-extension/manifest.json"),
    ),
    (
        "background.js",
        include_bytes!("../../../browser-extension/background.js"),
    ),
    (
        "content.js",
        include_bytes!("../../../browser-extension/content.js"),
    ),
    (
        "popup.html",
        include_bytes!("../../../browser-extension/popup.html"),
    ),
    (
        "popup.js",
        include_bytes!("../../../browser-extension/popup.js"),
    ),
    (
        "confirm.html",
        include_bytes!("../../../browser-extension/confirm.html"),
    ),
    (
        "confirm.js",
        include_bytes!("../../../browser-extension/confirm.js"),
    ),
    ("ui.js", include_bytes!("../../../browser-extension/ui.js")),
    (
        "ui.css",
        include_bytes!("../../../browser-extension/ui.css"),
    ),
];

#[cfg(target_os = "macos")]
fn register(_dir: &Path, manifest: &[u8]) -> io::Result<()> {
    let support = dirs::data_dir().ok_or_else(super::invalid)?;
    for browser in ["Google/Chrome", "Microsoft Edge"] {
        let hosts = support.join(browser).join("NativeMessagingHosts");
        fs::create_dir_all(&hosts)?;
        write_private(&hosts.join(format!("{HOST}.json")), manifest)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn register(dir: &Path, manifest: &[u8]) -> io::Result<()> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};
    let path = dir.join(format!("{HOST}.json"));
    write_private(&path, manifest)?;
    for browser in [r"Google\Chrome", r"Microsoft\Edge"] {
        let name = format!(r"Software\{browser}\NativeMessagingHosts\{HOST}");
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(name)?;
        key.set_value("", &path.as_os_str())?;
    }
    Ok(())
}

fn install() -> io::Result<PathBuf> {
    let dir = directory()?;
    let extension = dir.join("extension");
    fs::create_dir_all(&extension)?;
    for (name, bytes) in EXTENSION {
        write_private(&extension.join(name), bytes)?;
    }
    let manifest = json!({"name":HOST, "description":"ArgoVault", "path":std::env::current_exe()?,
        "type":"stdio", "allowed_origins":[ORIGIN]});
    register(&dir, &serde_json::to_vec_pretty(&manifest)?)?;
    Ok(extension)
}

pub(super) fn refresh_if_connected(dir: &Path) {
    if dir.join("extension/manifest.json").is_file() {
        let _ = install();
    }
}

#[tauri::command(async)]
pub fn browser_connect(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    if app.try_state::<super::Inbox>().is_none() {
        return Err("unavailable".into());
    }
    install()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}
