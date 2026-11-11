fn main() {
  #[cfg(target_os = "macos")]
  {
    println!("cargo:rustc-link-arg=-mmacosx-version-min=11.0");
  }
  tauri_build::build()
}
