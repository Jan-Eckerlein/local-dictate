fn link_clang_rt_on_macos() {
  if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
    return;
  }
  println!("cargo:rustc-link-arg=-mmacosx-version-min=11.0");

  // ggml-metal uses @available → ___isPlatformVersionAtLeast in compiler-rt.
  // Rust's final link uses -nodefaultlibs, so pull clang_rt explicitly when possible.
  let Ok(out) = std::process::Command::new("xcrun")
    .args(["--sdk", "macosx", "clang", "--print-file-name", "libclang_rt.osx.a"])
    .output()
  else {
    return;
  };
  let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
  if path.is_empty() || path == "libclang_rt.osx.a" {
    return;
  }
  if let Some(dir) = std::path::Path::new(&path).parent() {
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
  }
}

fn main() {
  link_clang_rt_on_macos();
  tauri_build::build();
}
