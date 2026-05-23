use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let build_input_dir = out_dir.join("tauri-build-input");
    let capabilities_dir = build_input_dir.join("capabilities");

    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("tauri.conf.json").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("capabilities").display()
    );

    fs::create_dir_all(&capabilities_dir).expect("create temporary Tauri capabilities directory");
    let web_dir = manifest_dir
        .parent()
        .expect("workspace root")
        .join("web")
        .to_string_lossy()
        .replace('\\', "\\\\");
    let config = fs::read_to_string(manifest_dir.join("tauri.conf.json"))
        .expect("read tauri.conf.json")
        .replace(
            "\"frontendDist\": \"../web\"",
            &format!("\"frontendDist\": \"{web_dir}\""),
        );
    fs::write(build_input_dir.join("tauri.conf.json"), config)
        .expect("write temporary tauri.conf.json");
    fs::copy(
        manifest_dir.join("Cargo.toml"),
        build_input_dir.join("Cargo.toml"),
    )
    .expect("copy Cargo.toml for Tauri metadata");
    fs::create_dir_all(build_input_dir.join("src")).expect("create temporary src directory");
    fs::write(build_input_dir.join("src/main.rs"), "fn main() {}\n")
        .expect("write temporary Cargo target");
    fs::copy(
        manifest_dir.join("capabilities/default.json"),
        capabilities_dir.join("default.json"),
    )
    .expect("copy Tauri default capability");

    env::set_current_dir(&build_input_dir).expect("switch to temporary Tauri build input");
    tauri_build::build()
}
