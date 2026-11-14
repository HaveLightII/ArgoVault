// macOS 生物辨識解鎖（Touch ID 加 Secure Enclave），實作在 macos/bio.swift。
#[cfg(target_os = "macos")]
mod bio {
    use serde_json::{json, Value};
    use std::ffi::{c_char, CStr, CString};
    use std::path::PathBuf;
    use tauri::{AppHandle, Manager};

    unsafe extern "C" {
        fn av_bio_status() -> i32;
        fn av_bio_enroll(
            path: *const c_char,
            secret: *const u8,
            len: isize,
            reason: *const c_char,
            cancel: *const c_char,
            msg: *mut c_char,
            msgCap: isize,
        ) -> i32;
        fn av_bio_unlock(
            path: *const c_char,
            reason: *const c_char,
            cancel: *const c_char,
            out: *mut u8,
            outCap: isize,
            outLen: *mut isize,
            msg: *mut c_char,
            msgCap: isize,
        ) -> i32;
    }

    const CODES: [&str; 6] = ["ok", "cancel", "lockout", "missing", "invalidated", "error"];

    fn dir(app: &AppHandle) -> Option<PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("bio"))
    }

    fn file(app: &AppHandle, profile: &str) -> Option<PathBuf> {
        let ok = !profile.is_empty() && profile.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');
        if ok { dir(app).map(|d| d.join(format!("{profile}.bio"))) } else { None }
    }

    fn cstr(s: &str) -> CString {
        CString::new(s.replace('\0', "")).unwrap()
    }

    fn reply(code: i32, msg: &[c_char], data: Vec<u8>) -> Value {
        let m = unsafe { CStr::from_ptr(msg.as_ptr()) }.to_string_lossy().into_owned();
        json!({ "code": CODES.get(code as usize).unwrap_or(&"error"), "msg": m, "data": data })
    }

    #[tauri::command(async)]
    pub fn bio_init(app: AppHandle) -> Value {
        let status = ["ok", "unenrolled", "nohw", "unavail"][unsafe { av_bio_status() }.clamp(0, 3) as usize];
        let profiles: Vec<String> = dir(&app)
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|it| {
                it.flatten()
                    .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".bio")).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        json!({ "status": status, "profiles": profiles })
    }

    #[tauri::command(async)]
    pub fn bio_remove(app: AppHandle, profile: String) {
        if let Some(p) = file(&app, &profile) {
            let _ = std::fs::remove_file(p);
        }
    }

    #[tauri::command(async)]
    #[allow(non_snake_case)]
    pub fn bio_enroll(app: AppHandle, profile: String, mut secret: Vec<u8>, reason: String, cancel: String) -> Value {
        let Some(p) = file(&app, &profile) else { return json!({ "code": "error", "msg": "badinput" }) };
        let (path, reason, cancel) = (cstr(&p.to_string_lossy()), cstr(&reason), cstr(&cancel));
        let mut msg = [0 as c_char; 256];
        let code = unsafe {
            av_bio_enroll(path.as_ptr(), secret.as_ptr(), secret.len() as isize, reason.as_ptr(), cancel.as_ptr(), msg.as_mut_ptr(), 256)
        };
        secret.fill(0);
        reply(code, &msg, Vec::new())
    }

    #[tauri::command(async)]
    pub fn bio_unlock(app: AppHandle, profile: String, reason: String, cancel: String) -> Value {
        let Some(p) = file(&app, &profile) else { return json!({ "code": "missing", "msg": "" }) };
        let (path, reason, cancel) = (cstr(&p.to_string_lossy()), cstr(&reason), cstr(&cancel));
        let mut msg = [0 as c_char; 256];
        let mut out = vec![0u8; 4096];
        let mut len: isize = 0;
        let code = unsafe {
            av_bio_unlock(path.as_ptr(), reason.as_ptr(), cancel.as_ptr(), out.as_mut_ptr(), 4096, &mut len, msg.as_mut_ptr(), 256)
        };
        let data = out[..(len.clamp(0, 4096) as usize)].to_vec();
        out.fill(0);
        reply(code, &msg, data)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init());
    #[cfg(target_os = "macos")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        bio::bio_init,
        bio::bio_remove,
        bio::bio_enroll,
        bio::bio_unlock
    ]);
    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
