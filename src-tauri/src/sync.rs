// 裝置對裝置同步（整檔）：分享端暫時監聽並顯示一次性配對碼，拉取端輸入位址與配對碼取得 vault 密文。
// 以 SPAKE2 由配對碼協商工作階段金鑰，再用 AES-256-GCM 加密傳輸；配對碼錯三次即停止分享。
// 訊框：4 位元組大端長度加內容。流程：A 訊息、B 訊息、加密的 "pull"（nonce 1）、nonce 加密文、加密的 "ok"（nonce 2）。
#![allow(non_snake_case)]

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use serde_json::{json, Value};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PORT: u16 = 47826;
const MAX: usize = 64 << 20;
const LIFE: Duration = Duration::from_secs(300);
const IO: Duration = Duration::from_secs(10);
const IDA: &[u8] = b"argovault-sync-pull";
const IDB: &[u8] = b"argovault-sync-share";

struct Share {
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<&'static str>>,
}

static SHARE: Mutex<Option<Share>> = Mutex::new(None);

fn writeFrame(s: &mut TcpStream, b: &[u8]) -> io::Result<()> {
    s.write_all(&(b.len() as u32).to_be_bytes())?;
    s.write_all(b)?;
    s.flush()
}

fn readFrame(s: &mut TcpStream, max: usize) -> io::Result<Vec<u8>> {
    let mut l = [0u8; 4];
    s.read_exact(&mut l)?;
    let n = u32::from_be_bytes(l) as usize;
    if n > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame"));
    }
    let mut b = vec![0u8; n];
    s.read_exact(&mut b)?;
    Ok(b)
}

fn fixedNonce(n: u8) -> [u8; 12] {
    let mut v = [0u8; 12];
    v[11] = n;
    v
}

fn bad() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "proto")
}

// Ok(true) 已送達；Ok(false) 配對碼錯誤；Err 為連線或格式問題（不計入錯誤次數）。
fn serve(mut s: TcpStream, code: &str, name: &str, data: &[u8]) -> io::Result<bool> {
    s.set_nonblocking(false)?;
    s.set_read_timeout(Some(IO))?;
    s.set_write_timeout(Some(IO))?;
    let msgA = readFrame(&mut s, 256)?;
    let (st, msgB) = Spake2::<Ed25519Group>::start_b(&Password::new(code.as_bytes()), &Identity::new(IDA), &Identity::new(IDB));
    writeFrame(&mut s, &msgB)?;
    let key = st.finish(&msgA).map_err(|_| bad())?;
    let c = Aes256Gcm::new_from_slice(&key).map_err(|_| bad())?;
    let req = readFrame(&mut s, 256)?;
    match c.decrypt(Nonce::from_slice(&fixedNonce(1)), req.as_slice()) {
        Ok(p) if p == b"pull" => {}
        _ => return Ok(false),
    }
    let nb = name.as_bytes();
    let mut plain = Vec::with_capacity(2 + nb.len() + data.len());
    plain.extend_from_slice(&(nb.len() as u16).to_be_bytes());
    plain.extend_from_slice(nb);
    plain.extend_from_slice(data);
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let mut out = nonce.to_vec();
    out.extend(c.encrypt(Nonce::from_slice(&nonce), plain.as_slice()).map_err(|_| bad())?);
    writeFrame(&mut s, &out)?;
    let ack = readFrame(&mut s, 256)?;
    match c.decrypt(Nonce::from_slice(&fixedNonce(2)), ack.as_slice()) {
        Ok(p) if p == b"ok" => Ok(true),
        _ => Err(bad()),
    }
}

fn stopShare() {
    if let Some(s) = SHARE.lock().unwrap().take() {
        s.stop.store(true, Ordering::SeqCst);
    }
}

fn localIp() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("192.0.2.1:9").and_then(|_| s.local_addr()))
        .map(|a| a.ip().to_string())
        .unwrap_or_default()
}

#[tauri::command(async)]
pub fn sync_share_start(name: String, data: Vec<u8>) -> Result<Value, String> {
    stopShare();
    if name.len() > 200 || data.len() + 1024 > MAX {
        return Err("size".into());
    }
    let listener = TcpListener::bind(("0.0.0.0", PORT)).or_else(|_| TcpListener::bind(("0.0.0.0", 0))).map_err(|_| "bind")?;
    listener.set_nonblocking(true).map_err(|_| "bind")?;
    let port = listener.local_addr().map_err(|_| "bind")?.port();
    let code = format!("{:06}", OsRng.next_u32() % 1_000_000);
    let stop = Arc::new(AtomicBool::new(false));
    let state = Arc::new(Mutex::new("waiting"));
    *SHARE.lock().unwrap() = Some(Share { stop: stop.clone(), state: state.clone() });
    let pin = code.clone();
    std::thread::spawn(move || {
        let (start, mut fails) = (Instant::now(), 0);
        let end = loop {
            if stop.load(Ordering::SeqCst) {
                break "stopped";
            }
            if start.elapsed() > LIFE {
                break "expired";
            }
            match listener.accept() {
                Ok((s, _)) => match serve(s, &pin, &name, &data) {
                    Ok(true) => break "done",
                    Ok(false) => {
                        fails += 1;
                        if fails >= 3 {
                            break "blocked";
                        }
                    }
                    Err(_) => {}
                },
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        };
        *state.lock().unwrap() = end;
    });
    // 實測（Android 16，SM-A1760）：剛開始監聽時，來自區網的連線會逾時，直到本機先連過一次才通；原因未明，先自連一次。
    let _ = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(1));
    Ok(json!({ "ip": localIp(), "port": port, "defaultPort": PORT, "code": code }))
}

#[tauri::command(async)]
pub fn sync_share_state() -> String {
    SHARE.lock().unwrap().as_ref().map(|s| *s.state.lock().unwrap()).unwrap_or("stopped").to_string()
}

#[tauri::command(async)]
pub fn sync_share_stop() {
    stopShare();
}

// 錯誤碼：addr 位址無效、connect 無法連線（冒號後附系統錯誤訊息）、auth 配對碼錯誤或連線中斷、proto 資料格式不符。
#[tauri::command(async)]
pub fn sync_pull(addr: String, code: String) -> Result<Value, String> {
    let addr = addr.trim();
    let target = if addr.contains(':') { addr.to_string() } else { format!("{addr}:{PORT}") };
    let sock = target.to_socket_addrs().ok().and_then(|mut i| i.next()).ok_or("addr")?;
    let net = |e: io::Error| format!("connect:{e}");
    let mut s = TcpStream::connect_timeout(&sock, Duration::from_secs(5)).map_err(net)?;
    s.set_read_timeout(Some(IO)).map_err(net)?;
    s.set_write_timeout(Some(IO)).map_err(net)?;
    let (st, msgA) = Spake2::<Ed25519Group>::start_a(&Password::new(code.trim().as_bytes()), &Identity::new(IDA), &Identity::new(IDB));
    writeFrame(&mut s, &msgA).map_err(net)?;
    let msgB = readFrame(&mut s, 256).map_err(net)?;
    let key = st.finish(&msgB).map_err(|_| "proto")?;
    let c = Aes256Gcm::new_from_slice(&key).map_err(|_| "proto")?;
    let req = c.encrypt(Nonce::from_slice(&fixedNonce(1)), b"pull".as_slice()).map_err(|_| "proto")?;
    writeFrame(&mut s, &req).map_err(|_| "connect")?;
    let got = readFrame(&mut s, MAX).map_err(|_| "auth")?;
    if got.len() < 12 {
        return Err("proto".into());
    }
    let plain = c.decrypt(Nonce::from_slice(&got[..12]), &got[12..]).map_err(|_| "auth")?;
    if plain.len() < 2 {
        return Err("proto".into());
    }
    let n = u16::from_be_bytes([plain[0], plain[1]]) as usize;
    if plain.len() < 2 + n {
        return Err("proto".into());
    }
    let name = String::from_utf8_lossy(&plain[2..2 + n]).into_owned();
    if let Ok(ack) = c.encrypt(Nonce::from_slice(&fixedNonce(2)), b"ok".as_slice()) {
        let _ = writeFrame(&mut s, &ack);
    }
    Ok(json!({ "name": name, "data": plain[2 + n..].to_vec() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundTrip() {
        let data: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let r = sync_share_start("測試-庫".into(), data.clone()).unwrap();
        let (port, code) = (r["port"].as_u64().unwrap(), r["code"].as_str().unwrap().to_string());
        let addr = format!("127.0.0.1:{port}");
        let wrong = if code == "000000" { "000001" } else { "000000" };
        assert_eq!(sync_pull(addr.clone(), wrong.into()).unwrap_err(), "auth");
        assert_eq!(sync_share_state(), "waiting");
        let got = sync_pull(addr.clone(), code.clone()).unwrap();
        assert_eq!(got["name"], "測試-庫");
        let bytes: Vec<u8> = got["data"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
        assert_eq!(bytes, data);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(sync_share_state(), "done");
        assert!(sync_pull(addr.clone(), code).unwrap_err().starts_with("connect:"));

        let r = sync_share_start("x".into(), vec![1, 2, 3]).unwrap();
        let addr = format!("127.0.0.1:{}", r["port"]);
        let code = r["code"].as_str().unwrap();
        let wrong = if code == "000000" { "000001" } else { "000000" };
        for _ in 0..3 {
            assert_eq!(sync_pull(addr.clone(), wrong.into()).unwrap_err(), "auth");
        }
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(sync_share_state(), "blocked");
        sync_share_stop();
        assert_eq!(sync_share_state(), "stopped");
    }
}
