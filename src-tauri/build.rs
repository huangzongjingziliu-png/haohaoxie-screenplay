fn main() {
    // Compile the current renderer into the resource Tauri actually bundles.
    // A manually copied binary can otherwise silently ship an older font protocol.
    let source = "../crates/screenplay-export/swift/pdf_sidecar.swift";
    println!("cargo:rerun-if-changed={source}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        std::fs::create_dir_all("resources").expect("create sidecar resource directory");
        let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
            Ok("aarch64") => "arm64", Ok("x86_64") => "x86_64", other => panic!("unsupported macOS target: {other:?}"),
        };
        let result = std::process::Command::new("swiftc")
            .args(["-O", "-target", &format!("{arch}-apple-macos11.0"), source, "-o", "resources/pdf_sidecar"])
            .status().expect("Swift compiler is required to build the macOS PDF renderer");
        assert!(result.success(), "PDF renderer compilation failed; refuse to bundle stale renderer");
    }
    tauri_build::build()
}
