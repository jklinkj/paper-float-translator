fn main() {
    compile_macos_native_bridge();
    tauri_build::build();
}

fn compile_macos_native_bridge() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.ends_with("apple-darwin") {
        return;
    }

    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let source = manifest_dir.join("native/PaperFloatNativeBridge.m");
    let object = out_dir.join("PaperFloatNativeBridge.o");
    let library = out_dir.join("libpaper_float_native_bridge.a");
    let clang_target = clang_target_for_cargo_target(&target);

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_ACCEPTANCE_TESTING");

    let mut clang = std::process::Command::new("/usr/bin/xcrun");
    clang.args([
        "clang",
        "-x",
        "objective-c",
        "-fobjc-arc",
        "-fblocks",
        "-O2",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-target",
        clang_target,
        "-mmacosx-version-min=11.0",
    ]);
    if std::env::var_os("CARGO_FEATURE_ACCEPTANCE_TESTING").is_some() {
        clang.arg("-DPAPER_FLOAT_NATIVE_TESTING=1");
    }
    let clang_status = clang
        .args([
            "-c",
            source
                .to_str()
                .expect("native bridge source path is not valid UTF-8"),
            "-o",
            object
                .to_str()
                .expect("native bridge object path is not valid UTF-8"),
        ])
        .status()
        .expect("failed to run xcrun clang; install Xcode Command Line Tools before building");

    if !clang_status.success() {
        panic!("failed to compile Paper Float native macOS bridge");
    }

    let ar_status = std::process::Command::new("/usr/bin/xcrun")
        .args([
            "ar",
            "crs",
            library
                .to_str()
                .expect("native bridge library path is not valid UTF-8"),
            object
                .to_str()
                .expect("native bridge object path is not valid UTF-8"),
        ])
        .status()
        .expect("failed to run xcrun ar while building native bridge");

    if !ar_status.success() {
        panic!("failed to archive Paper Float native macOS bridge");
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=paper_float_native_bridge");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=ApplicationServices");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=LocalAuthentication");
    println!("cargo:rustc-link-lib=framework=Security");
    println!("cargo:rustc-link-lib=objc");
}

fn clang_target_for_cargo_target(target: &str) -> &'static str {
    match target {
        "aarch64-apple-darwin" => "arm64-apple-macosx11.0",
        "x86_64-apple-darwin" => "x86_64-apple-macosx11.0",
        _ => "x86_64-apple-macosx11.0",
    }
}
