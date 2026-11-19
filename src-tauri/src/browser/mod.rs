// Chrome / Edge launch this executable with the extension origin as argv[1].
// The short-lived host talks to the sole vault UI; it never opens vault files.
pub(crate) mod install;

use aes_gcm::aead::{Aead, KeyInit, OsRng, rand_core::RngCore};
use aes_gcm::{Aes256Gcm, Nonce};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const ORIGIN: &str = "chrome-extension://glegogjdjanaadhibeoinabaejjgfkpm/";
const HOST: &str = "com.havelight.argovault";
const MAX_FRAME: usize = 128 * 1024;
const TTL: Duration = Duration::from_secs(90);
const IO_TIMEOUT: Duration = Duration::from_secs(3);

fn invalid() -> io::Error {
    io::Error::other("invalid browser message")
}

fn directory() -> io::Result<PathBuf> {
    let dir = dirs::data_local_dir()
        .ok_or_else(invalid)?
        .join("com.havelight.pypasswords/browser");
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

// Atomic replacement also works when upgrading an already registered host on Windows.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().ok_or_else(invalid)?)?;
    temp.write_all(bytes)?;
    temp.persist(path)?;
    Ok(())
}

fn frame_read(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut size = [0; 4];
    reader.read_exact(&mut size)?;
    let size = u32::from_ne_bytes(size) as usize;
    if size == 0 || size > MAX_FRAME {
        return Err(invalid());
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn frame_write(writer: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(invalid());
    }
    writer.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Login {
    url: String,
    user: String,
    password: String,
}

impl Login {
    fn valid(&self) -> bool {
        tauri::Url::parse(&self.url)
            .is_ok_and(|u| u.scheme() == "https" && u.origin().ascii_serialization() == self.url)
            && self.user.len() <= 16384
            && !self.password.is_empty()
            && self.password.len() <= 32768
    }
}

struct Ticket {
    id: String,
    login: Option<Login>,
    until: Instant,
}
#[derive(Default)]
struct Inbox(Mutex<Option<Ticket>>);

#[derive(Serialize)]
pub struct Request {
    id: String,
    #[serde(flatten)]
    login: Login,
}

#[tauri::command]
pub fn browser_claim(app: AppHandle) -> Option<Request> {
    let state = app.try_state::<Inbox>()?;
    let mut slot = state.0.lock().ok()?;
    let ticket = slot.as_mut()?;
    if ticket.until <= Instant::now() {
        *slot = None;
        return None;
    }
    Some(Request {
        id: ticket.id.clone(),
        login: ticket.login.take()?,
    })
}

#[tauri::command]
pub fn browser_finish(app: AppHandle, id: String) {
    let Some(state) = app.try_state::<Inbox>() else {
        return;
    };
    let mut slot = state.0.lock().unwrap();
    if slot.as_ref().is_some_and(|t| t.id == id) {
        *slot = None;
    }
}

fn focus(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn present(app: &AppHandle, login: Login) -> io::Result<()> {
    if !login.valid() {
        return Err(invalid());
    }
    let id = format!("{:016x}{:016x}", OsRng.next_u64(), OsRng.next_u64());
    {
        let inbox = app.state::<Inbox>();
        let mut slot = inbox.0.lock().unwrap();
        if slot.as_ref().is_some_and(|t| t.until > Instant::now()) {
            return Err(invalid());
        }
        *slot = Some(Ticket {
            id: id.clone(),
            login: Some(login),
            until: Instant::now() + TTL,
        });
    }
    focus(app);
    // Only a wakeup event; the ready main window claims the secret exactly once.
    let _ = app.emit_to("main", "browser-save-pending", ());
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(TTL);
        browser_finish(app, id);
    });
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Session {
    port: u16,
    key: [u8; 32],
}

fn session_path(dir: &Path) -> PathBuf {
    dir.join("session.json")
}
fn instance_lock(dir: &Path) -> io::Result<File> {
    File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("instance.lock"))
}

// Authenticate both directions using a new key per app launch and a server challenge
// per connection. Stale/reassigned ports cannot read credentials or replay a request.
fn timeout(stream: &TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))
}
fn reply_nonce(mut nonce: [u8; 12]) -> [u8; 12] {
    nonce[0] |= 0x80;
    nonce
}

fn serve(app: &AppHandle, key: &[u8; 32], mut stream: TcpStream) -> io::Result<()> {
    timeout(&stream)?;
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0; 12];
    OsRng.fill_bytes(&mut nonce);
    nonce[0] &= 0x7f;
    stream.write_all(&nonce)?;
    let encrypted = frame_read(&mut stream)?;
    let mut plain = cipher
        .decrypt(Nonce::from_slice(&nonce), encrypted.as_slice())
        .map_err(|_| invalid())?;
    let accepted = if plain == b"focus" {
        focus(app);
        true
    } else {
        serde_json::from_slice(&plain)
            .ok()
            .is_some_and(|login| present(app, login).is_ok())
    };
    plain.fill(0);
    let reply = cipher
        .encrypt(
            Nonce::from_slice(&reply_nonce(nonce)),
            &[accepted as u8][..],
        )
        .map_err(|_| invalid())?;
    frame_write(&mut stream, &reply)
}

fn exchange(session: &Session, bytes: &[u8]) -> io::Result<bool> {
    let addr = (Ipv4Addr::LOCALHOST, session.port).into();
    let mut stream = TcpStream::connect_timeout(&addr, IO_TIMEOUT)?;
    timeout(&stream)?;
    let mut nonce = [0; 12];
    stream.read_exact(&mut nonce)?;
    if nonce[0] & 0x80 != 0 {
        return Err(invalid());
    }
    let cipher = Aes256Gcm::new((&session.key).into());
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&nonce), bytes)
        .map_err(|_| invalid())?;
    frame_write(&mut stream, &encrypted)?;
    let reply = frame_read(&mut stream)?;
    let plain = cipher
        .decrypt(Nonce::from_slice(&reply_nonce(nonce)), reply.as_slice())
        .map_err(|_| invalid())?;
    Ok(plain == [1])
}

fn read_session(dir: &Path) -> io::Result<Session> {
    Ok(serde_json::from_slice(&fs::read(session_path(dir))?)?)
}

pub struct Instance {
    dir: PathBuf,
    _lock: File,
}
impl Drop for Instance {
    fn drop(&mut self) {
        let _ = fs::remove_file(session_path(&self.dir));
    }
}

pub fn primary() -> io::Result<Option<Instance>> {
    let dir = directory()?;
    let lock = instance_lock(&dir)?;
    match lock.try_lock() {
        Ok(()) => Ok(Some(Instance { dir, _lock: lock })),
        Err(std::fs::TryLockError::WouldBlock) => {
            if let Ok(s) = read_session(&dir) {
                let _ = exchange(&s, b"focus");
            }
            Ok(None)
        }
        Err(std::fs::TryLockError::Error(e)) => Err(e),
    }
}

pub fn listen(app: AppHandle, instance: Instance) -> io::Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let mut key = [0; 32];
    OsRng.fill_bytes(&mut key);
    let session = Session {
        port: listener.local_addr()?.port(),
        key,
    };
    write_private(&session_path(&instance.dir), &serde_json::to_vec(&session)?)?;
    install::refresh_if_connected(&instance.dir);
    app.manage(Inbox::default());
    app.manage(instance);
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = serve(&app, &key, stream);
        }
    });
    Ok(())
}

fn launch() -> io::Result<()> {
    let exe = std::env::current_exe()?;
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let bundle = exe
            .ancestors()
            .find(|p| p.extension().is_some_and(|v| v == "app"));
        if let Some(bundle) = bundle {
            let mut open = Command::new("/usr/bin/open");
            open.args(["-n", "-a"]).arg(bundle);
            open
        } else {
            Command::new(exe)
        }
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        // Ask to leave the native host's browser job so closing its pipe cannot kill the vault.
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        if Command::new(&exe)
            .creation_flags(CREATE_BREAKAWAY_FROM_JOB)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
        // Some browser jobs disallow breakaway; hand activation to the desktop shell.
        let explorer =
            PathBuf::from(std::env::var_os("SystemRoot").ok_or_else(invalid)?).join("explorer.exe");
        let mut cmd = Command::new(explorer);
        cmd.arg(exe);
        cmd
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

fn forward(bytes: &[u8]) -> io::Result<bool> {
    let login: Login = serde_json::from_slice(bytes)?;
    if !login.valid() {
        return Err(invalid());
    }
    drop(login);
    let dir = directory()?;
    let lock = instance_lock(&dir)?;
    let old = read_session(&dir).ok();
    let cold = match lock.try_lock() {
        Ok(()) => {
            drop(lock);
            launch()?;
            true
        }
        Err(std::fs::TryLockError::WouldBlock) => false,
        Err(std::fs::TryLockError::Error(e)) => return Err(e),
    };
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(session) = read_session(&dir) {
            if !cold || !old.as_ref().is_some_and(|s| s.key == session.key) {
                match exchange(&session, bytes) {
                    Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {}
                    result => return result,
                }
            }
        }
        if Instant::now() >= until {
            return Err(invalid());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub fn handle_native_launch() -> bool {
    let Some(origin) = std::env::args_os().nth(1) else {
        return false;
    };
    if !origin.to_string_lossy().starts_with("chrome-extension://") {
        return false;
    }
    let ok = origin == ORIGIN
        && frame_read(&mut io::stdin().lock()).is_ok_and(|mut bytes| {
            let result = forward(&bytes).unwrap_or(false);
            bytes.fill(0);
            result
        });
    let reply: &[u8] = if ok {
        br#"{"ok":true}"#
    } else {
        br#"{"ok":false}"#
    };
    let _ = frame_write(&mut io::stdout().lock(), reply);
    true
}
