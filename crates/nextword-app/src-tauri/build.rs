//! Tauri + llama.cpp build script.
//!
//! Runs tauri-build for the Tauri side, then checks whether
//! `binaries/llama-server-<triple>` exists at the workspace root.
//! If not, it shells out to cmake + make on `vendor/llama.cpp`.
//!
//! The built binary is required by tauri.conf.json's `externalBin`
//! entry, so missing it would fail the bundle step anyway.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Build llama-server FIRST. tauri_build::build() validates that the
    // externalBin path exists and errors out if not, so we need a binary
    // in place before we hand off to it.
    build_llama_server();
    tauri_build::build();
}

fn build_llama_server() {
    let target_triple = std::env::var("TARGET").unwrap_or_default();
    let workspace_root = workspace_root();
    let binaries_dir = workspace_root.join("binaries");
    std::fs::create_dir_all(&binaries_dir).expect("create binaries dir");

    let dest = binaries_dir.join(format!("llama-server-{target_triple}"));
    let dest_plain = binaries_dir.join("llama-server");

    if dest.exists() {
        println!("cargo:warning=llama-server already built at {}", dest.display());
        return;
    }

    if std::env::var("NEXTWORD_SKIP_LLAMA_BUILD").is_ok() {
        // tauri-build still requires the file to exist; drop a stub.
        write_stub(&dest);
        write_stub(&dest_plain);
        println!("cargo:warning=NEXTWORD_SKIP_LLAMA_BUILD set; wrote stub at {}", dest.display());
        return;
    }

    let llama_src = workspace_root.join("vendor").join("llama.cpp");
    if !llama_src.join("CMakeLists.txt").exists() {
        println!(
            "cargo:warning=vendor/llama.cpp missing. Run: git submodule update --init --recursive"
        );
        println!("cargo:warning=Skipping sidecar build. The Tauri bundle step will fail until this is fixed.");
        return;
    }

    let build_dir = llama_src.join("build");
    std::fs::create_dir_all(&build_dir).expect("create build dir");

    let mut cmake_args: Vec<String> = vec![
        "-S".into(), llama_src.to_string_lossy().into(),
        "-B".into(), build_dir.to_string_lossy().into(),
        "-DCMAKE_BUILD_TYPE=Release".into(),
        "-DLLAMA_BUILD_TESTS=OFF".into(),
        "-DLLAMA_BUILD_EXAMPLES=OFF".into(),
        "-DLLAMA_BUILD_SERVER=ON".into(),
    ];

    if cfg!(target_os = "macos") {
        cmake_args.push("-DGGML_METAL=ON".into());
        cmake_args.push("-DGGML_METAL_EMBED_LIBRARY=ON".into());
    }

    let status = Command::new("cmake").args(&cmake_args).status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            println!("cargo:warning=cmake configure failed ({s}); see logs above");
            return;
        }
        Err(e) => {
            println!("cargo:warning=cmake not found: {e}");
            return;
        }
    }

    let build_status = Command::new("cmake")
        .args([
            "--build",
            &build_dir.to_string_lossy(),
            "--config",
            "Release",
            "--target",
            "llama-server",
            "--parallel",
        ])
        .status();
    match build_status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            println!("cargo:warning=cmake build failed ({s})");
            return;
        }
        Err(e) => {
            println!("cargo:warning=cmake build error: {e}");
            return;
        }
    }

    let candidates = [
        build_dir.join("bin").join("llama-server"),
        build_dir.join("bin").join("Release").join("llama-server"),
    ];
    let built = candidates.into_iter().find(|p| p.exists());

    match built {
        Some(p) => {
            if let Err(e) = std::fs::copy(&p, &dest) {
                println!("cargo:warning=failed to copy llama-server to {}: {}", dest.display(), e);
            }
            let _ = std::fs::copy(&p, &dest_plain);
            println!("cargo:warning=copied llama-server to {}", dest.display());
        }
        None => {
            println!("cargo:warning=llama-server binary not found in build/bin after build");
        }
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", llama_src.join("CMakeLists.txt").display());
}

fn write_stub(path: &Path) {
    use std::io::Write;
    let mut f = std::fs::File::create(path).expect("create stub file");
    writeln!(f, "#!/bin/sh\necho 'NEXTWORD_SKIP_LLAMA_BUILD stub'\nexit 1").ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = f.metadata().unwrap().permissions();
        perm.set_mode(0o755);
        let _ = std::fs::set_permissions(path, perm);
    }
}

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // src-tauri -> nextword-app -> crates -> workspace root
    manifest
        .parent().and_then(Path::parent).and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}
