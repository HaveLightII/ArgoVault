fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        buildBio();
    }
    tauri_build::build()
}

// 把 macos/bio.swift 編成靜態庫並連結（只需 Command Line Tools）。
#[allow(non_snake_case)]
fn buildBio() {
    use std::process::Command;
    let run = |c: &mut Command| -> String {
        let o = c.output().expect("無法執行 Swift 工具");
        if !o.status.success() {
            panic!("{}", String::from_utf8_lossy(&o.stderr));
        }
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    let out = std::env::var("OUT_DIR").unwrap();
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64",
        a => a,
    }
    .to_string();
    let sdk = run(Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]));
    let swiftc = run(Command::new("xcrun").args(["--find", "swiftc"]));
    run(Command::new(&swiftc).args([
        "-emit-library",
        "-static",
        "-parse-as-library",
        "-O",
        "-module-name",
        "ArgoBio",
        "-sdk",
        &sdk,
        "-target",
        &format!("{arch}-apple-macosx11.0"),
        "macos/bio.swift",
        "-o",
        &format!("{out}/libargobio.a"),
    ]));
    let toolchain = std::path::Path::new(&swiftc).parent().unwrap().join("../lib/swift/macosx");
    println!("cargo:rerun-if-changed=macos/bio.swift");
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-search=native={}", toolchain.display());
    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    println!("cargo:rustc-link-search=native=/usr/lib/swift");
    println!("cargo:rustc-link-lib=static=argobio");
    for f in ["Foundation", "Security", "LocalAuthentication", "CryptoKit"] {
        println!("cargo:rustc-link-lib=framework={f}");
    }
}
