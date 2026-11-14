// 桌面生物辨識解鎖：macOS 為 Touch ID 加 Secure Enclave（macos/bio.swift），Windows 為 Windows Hello。
// 兩者都是「包裝主碼」，註冊資料存 <AppData>/bio/<設定檔>.bio；回傳碼與 Android 的 PwBio 相同。
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[allow(non_snake_case)]
mod bio {
    use serde_json::{json, Value};
    use std::path::PathBuf;
    use tauri::{AppHandle, Manager};

    fn dir(app: &AppHandle) -> Option<PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("bio"))
    }

    fn file(app: &AppHandle, profile: &str) -> Option<PathBuf> {
        let ok = !profile.is_empty() && profile.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');
        if ok { dir(app).map(|d| d.join(format!("{profile}.bio"))) } else { None }
    }

    #[tauri::command(async)]
    pub fn bio_init(app: AppHandle) -> Value {
        let profiles: Vec<String> = dir(&app)
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|it| {
                it.flatten()
                    .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".bio")).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        json!({ "status": imp::status(), "profiles": profiles })
    }

    #[tauri::command(async)]
    pub fn bio_remove(app: AppHandle, profile: String) {
        if let Some(p) = file(&app, &profile) {
            let _ = std::fs::remove_file(p);
            imp::remove(&profile);
        }
    }

    #[tauri::command(async)]
    pub fn bio_enroll(app: AppHandle, profile: String, mut secret: Vec<u8>, reason: String, cancel: String) -> Value {
        let Some(p) = file(&app, &profile) else { return json!({ "code": "error", "msg": "badinput" }) };
        let (code, msg) = imp::enroll(&p, &profile, &secret, &reason, &cancel);
        secret.fill(0);
        json!({ "code": code, "msg": msg })
    }

    #[tauri::command(async)]
    pub fn bio_unlock(app: AppHandle, profile: String, reason: String, cancel: String) -> Value {
        let Some(p) = file(&app, &profile) else { return json!({ "code": "missing", "msg": "" }) };
        let (code, msg, data) = imp::unlock(&p, &profile, &reason, &cancel);
        json!({ "code": code, "msg": msg, "data": data })
    }

    #[cfg(target_os = "macos")]
    mod imp {
        use std::ffi::{c_char, CStr, CString};
        use std::path::Path;

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

        fn cstr(s: &str) -> CString {
            CString::new(s.replace('\0', "")).unwrap()
        }

        fn text(msg: &[c_char]) -> String {
            unsafe { CStr::from_ptr(msg.as_ptr()) }.to_string_lossy().into_owned()
        }

        fn code(c: i32) -> &'static str {
            CODES.get(c as usize).copied().unwrap_or("error")
        }

        pub fn status() -> &'static str {
            ["ok", "unenrolled", "nohw", "unavail"][unsafe { av_bio_status() }.clamp(0, 3) as usize]
        }

        pub fn remove(_profile: &str) {}

        pub fn enroll(p: &Path, _profile: &str, secret: &[u8], reason: &str, cancel: &str) -> (&'static str, String) {
            let (path, reason, cancel) = (cstr(&p.to_string_lossy()), cstr(reason), cstr(cancel));
            let mut msg = [0 as c_char; 256];
            let c = unsafe {
                av_bio_enroll(path.as_ptr(), secret.as_ptr(), secret.len() as isize, reason.as_ptr(), cancel.as_ptr(), msg.as_mut_ptr(), 256)
            };
            (code(c), text(&msg))
        }

        pub fn unlock(p: &Path, _profile: &str, reason: &str, cancel: &str) -> (&'static str, String, Vec<u8>) {
            let (path, reason, cancel) = (cstr(&p.to_string_lossy()), cstr(reason), cstr(cancel));
            let mut msg = [0 as c_char; 256];
            let mut out = vec![0u8; 4096];
            let mut len: isize = 0;
            let c = unsafe {
                av_bio_unlock(path.as_ptr(), reason.as_ptr(), cancel.as_ptr(), out.as_mut_ptr(), 4096, &mut len, msg.as_mut_ptr(), 256)
            };
            let data = out[..(len.clamp(0, 4096) as usize)].to_vec();
            out.fill(0);
            (code(c), text(&msg), data)
        }
    }

    // Windows Hello：每設定檔一把 KeyCredential（有 TPM 時由硬體保護），對隨機挑戰值簽章，
    // 以 SHA-256(簽章) 為 AES-256-GCM 金鑰加密主碼。簽章為確定性的 RSA PKCS#1 v1.5，故每次可重得同一把金鑰。
    #[cfg(target_os = "windows")]
    mod imp {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Key, Nonce};
        use serde::{Deserialize, Serialize};
        use sha2::{Digest, Sha256};
        use std::path::Path;
        use windows::core::{w, Array, Result as WinResult, HSTRING, PCWSTR};
        use windows::Security::Credentials::{
            KeyCredential, KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
        };
        use windows::Security::Cryptography::CryptographicBuffer;
        use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow};

        #[derive(Serialize, Deserialize)]
        struct Entry {
            challenge: Vec<u8>,
            nonce: Vec<u8>,
            sealed: Vec<u8>,
        }

        fn keyName(profile: &str) -> HSTRING {
            HSTRING::from(format!("ArgoVault-{profile}"))
        }

        fn random(n: u32) -> WinResult<Vec<u8>> {
            let mut a = Array::<u8>::new();
            CryptographicBuffer::CopyToByteArray(&CryptographicBuffer::GenerateRandom(n)?, &mut a)?;
            Ok(a.to_vec())
        }

        fn code(s: KeyCredentialStatus) -> &'static str {
            match s {
                KeyCredentialStatus::UserCanceled | KeyCredentialStatus::UserPrefersPassword => "cancel",
                KeyCredentialStatus::SecurityDeviceLocked => "lockout",
                KeyCredentialStatus::NotFound => "invalidated",
                _ => "error",
            }
        }

        // Windows Hello 對話框有時會開在視窗後方，短暫輪詢並把它帶到前景。
        fn focusPrompt() {
            std::thread::spawn(|| {
                for _ in 0..60 {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    if let Ok(h) = unsafe { FindWindowW(w!("Credential Dialog Xaml Host"), PCWSTR::null()) } {
                        if !h.is_invalid() {
                            let _ = unsafe { SetForegroundWindow(h) };
                            break;
                        }
                    }
                }
            });
        }

        fn sign(cred: &KeyCredential, challenge: &[u8]) -> WinResult<Result<[u8; 32], &'static str>> {
            let buf = CryptographicBuffer::CreateFromByteArray(challenge)?;
            focusPrompt();
            let r = cred.RequestSignAsync(&buf)?.join()?;
            let st = r.Status()?;
            if st != KeyCredentialStatus::Success {
                return Ok(Err(code(st)));
            }
            let mut sig = Array::<u8>::new();
            CryptographicBuffer::CopyToByteArray(&r.Result()?, &mut sig)?;
            let mut h = Sha256::new();
            h.update(b"ArgoVault-bio-v1");
            h.update(&sig[..]);
            Ok(Ok(h.finalize().into()))
        }

        fn cipher(key: &[u8; 32]) -> Aes256Gcm {
            Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key))
        }

        pub fn status() -> &'static str {
            match KeyCredentialManager::IsSupportedAsync().and_then(|o| o.join()) {
                Ok(true) => "ok",
                Ok(false) => "unenrolled",
                Err(_) => "unavail",
            }
        }

        pub fn remove(profile: &str) {
            let _ = KeyCredentialManager::DeleteAsync(&keyName(profile)).and_then(|o| o.join());
        }

        fn enrollInner(p: &Path, profile: &str, secret: &[u8]) -> WinResult<Result<(), &'static str>> {
            focusPrompt();
            let r = KeyCredentialManager::RequestCreateAsync(&keyName(profile), KeyCredentialCreationOption::ReplaceExisting)?
                .join()?;
            let st = r.Status()?;
            if st != KeyCredentialStatus::Success {
                return Ok(Err(code(st)));
            }
            let (challenge, nonce) = (random(32)?, random(12)?);
            let mut key = match sign(&r.Credential()?, &challenge)? {
                Ok(k) => k,
                Err(c) => return Ok(Err(c)),
            };
            let sealed = cipher(&key).encrypt(Nonce::from_slice(&nonce), secret);
            key.fill(0);
            let Ok(sealed) = sealed else { return Ok(Err("error")) };
            let json = serde_json::to_vec(&Entry { challenge, nonce, sealed }).unwrap_or_default();
            let wrote = p.parent().map(std::fs::create_dir_all).is_some_and(|r| r.is_ok()) && std::fs::write(p, json).is_ok();
            Ok(if wrote { Ok(()) } else { Err("error") })
        }

        pub fn enroll(p: &Path, profile: &str, secret: &[u8], _reason: &str, _cancel: &str) -> (&'static str, String) {
            let (c, msg) = match enrollInner(p, profile, secret) {
                Ok(Ok(())) => ("ok", String::new()),
                Ok(Err(c)) => (c, String::new()),
                Err(e) => ("error", e.message().to_string()),
            };
            if c != "ok" {
                let _ = std::fs::remove_file(p);
                remove(profile);
            }
            (c, msg)
        }

        fn unlockInner(p: &Path, profile: &str) -> WinResult<Result<Vec<u8>, &'static str>> {
            let Some(entry) = std::fs::read(p).ok().and_then(|b| serde_json::from_slice::<Entry>(&b).ok()) else {
                return Ok(Err("missing"));
            };
            if entry.nonce.len() != 12 {
                return Ok(Err("missing"));
            }
            let r = KeyCredentialManager::OpenAsync(&keyName(profile))?.join()?;
            let st = r.Status()?;
            if st != KeyCredentialStatus::Success {
                return Ok(Err(code(st)));
            }
            let mut key = match sign(&r.Credential()?, &entry.challenge)? {
                Ok(k) => k,
                Err(c) => return Ok(Err(c)),
            };
            let plain = cipher(&key).decrypt(Nonce::from_slice(&entry.nonce), entry.sealed.as_slice());
            key.fill(0);
            Ok(plain.map_err(|_| "invalidated"))
        }

        pub fn unlock(p: &Path, profile: &str, _reason: &str, _cancel: &str) -> (&'static str, String, Vec<u8>) {
            match unlockInner(p, profile) {
                Ok(Ok(d)) => ("ok", String::new(), d),
                Ok(Err(c)) => (c, String::new(), Vec::new()),
                Err(e) => ("error", e.message().to_string(), Vec::new()),
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init());
    #[cfg(any(target_os = "macos", target_os = "windows"))]
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
