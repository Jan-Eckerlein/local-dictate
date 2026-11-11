//! macOS Accessibility (TCC) for synthetic ⌘V.

#[cfg(target_os = "macos")]
mod ffi {
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        pub fn AXIsProcessTrusted() -> bool;
    }
}

#[cfg(target_os = "macos")]
pub fn authorization_status() -> &'static str {
    if unsafe { ffi::AXIsProcessTrusted() } {
        "authorized"
    } else {
        "denied"
    }
}

#[cfg(not(target_os = "macos"))]
pub fn authorization_status() -> &'static str {
    "authorized"
}

#[cfg(target_os = "macos")]
pub fn ensure_trusted() -> Result<(), String> {
    if unsafe { ffi::AXIsProcessTrusted() } {
        Ok(())
    } else {
        Err(
            "Accessibility access required to paste into other apps. Enable local-dictate in System Settings → Privacy & Security → Accessibility.".into(),
        )
    }
}

#[cfg(not(target_os = "macos"))]
pub fn ensure_trusted() -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn open_privacy_settings() -> Result<(), String> {
    std::process::Command::new("open")
        .arg(
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility",
        )
        .spawn()
        .map_err(|e| format!("open settings: {e}"))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn open_privacy_settings() -> Result<(), String> {
    Err("macOS only".into())
}
