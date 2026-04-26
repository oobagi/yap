fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // Build the native overlay sidecar on macOS (release builds only).
    // In dev mode (`cargo run`), the sidecar is found at its debug build
    // path — run `swift build` once in sidecar-overlay/ to set it up.
    if target_os == "macos" {
        let profile = std::env::var("PROFILE").unwrap_or_default();
        if profile == "release" {
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
            let script = std::path::PathBuf::from(&manifest_dir)
                .join("sidecar-overlay/build-sidecar.sh");

            if script.exists() {
                let status = std::process::Command::new("bash")
                    .arg(&script)
                    .status()
                    .expect("failed to run build-sidecar.sh");
                assert!(status.success(), "build-sidecar.sh failed");
            }
        }
    }

    tauri_build::build();

    // Link the Speech framework on macOS for SFSpeechRecognizer
    if target_os == "macos" {
        println!("cargo:rustc-link-lib=framework=Speech");
    }
}
