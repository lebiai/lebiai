//! Ensure the web UI is available for the desktop shell.
//!
//! Default load path is `ui/dist` (not Vite on :5173). If `dist` is missing
//! at compile time and Node is available, run `npm run build` once so
//! `cargo run -p hermes-gui` does not open a white screen.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ui_dir = manifest_dir.join("ui");
    let index = ui_dir.join("dist").join("index.html");

    println!("cargo:rerun-if-changed={}", index.display());
    println!(
        "cargo:rerun-if-changed={}",
        ui_dir.join("package.json").display()
    );

    if !index.exists() {
        eprintln!(
            "cargo:warning=lebi-AI: ui/dist missing — building frontend (npm run build in ui/)"
        );
        let node_modules = ui_dir.join("node_modules");
        if !node_modules.exists() {
            let st = Command::new("npm")
                .arg("install")
                .current_dir(&ui_dir)
                .status()
                .expect("failed to spawn npm install; install Node.js or run: cd crates/hermes-gui/ui && npm install && npm run build");
            assert!(
                st.success(),
                "npm install failed; fix Node/npm then: cd crates/hermes-gui/ui && npm install && npm run build"
            );
        }
        let st = Command::new("npm")
            .args(["run", "build"])
            .current_dir(&ui_dir)
            .status()
            .expect("failed to spawn npm run build; install Node.js or run: cd crates/hermes-gui/ui && npm run build");
        assert!(
            st.success(),
            "npm run build failed; fix the UI build then: cd crates/hermes-gui/ui && npm run build"
        );
        assert!(
            index.exists(),
            "ui/dist/index.html still missing after npm run build"
        );
    }

    tauri_build::build()
}
