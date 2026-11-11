//! macOS Accessibility (TCC) for synthetic ⌘V.

#[cfg(target_os = "macos")]
mod ffi {
    use core_foundation::dictionary::CFDictionaryRef;
    use core_foundation::string::CFStringRef;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        pub fn AXIsProcessTrusted() -> bool;
        pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
        pub static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    #[link(name = "System")]
    unsafe extern "C" {
        fn pthread_main_np() -> i32;
        /// `dispatch_get_main_queue()` is a header inline of `&_dispatch_main_q`.
        /// Newer SDKs (Xcode 26) do not export the function symbol.
        static _dispatch_main_q: u8;
        fn dispatch_sync_f(
            queue: *mut std::ffi::c_void,
            context: *mut std::ffi::c_void,
            work: extern "C" fn(*mut std::ffi::c_void),
        );
    }

    /// System dialog that opens Privacy → Accessibility. Not an Allow/Deny grant.
    pub fn prompt_if_untrusted() {
        if unsafe { AXIsProcessTrusted() } {
            return;
        }
        if unsafe { pthread_main_np() } != 0 {
            show_prompt();
            return;
        }
        unsafe {
            dispatch_sync_f(
                &raw const _dispatch_main_q as *mut std::ffi::c_void,
                std::ptr::null_mut(),
                show_prompt_on_main,
            );
        }
    }

    extern "C" fn show_prompt_on_main(_: *mut std::ffi::c_void) {
        show_prompt();
    }

    fn show_prompt() {
        use core_foundation::base::TCFType;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;

        let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
        let value = CFBoolean::true_value();
        let options = CFDictionary::from_CFType_pairs(&[(key, value)]);
        unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) };
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

/// Show the system Accessibility prompt when this process is not trusted yet.
#[cfg(target_os = "macos")]
pub fn request_access_if_needed() -> Result<(), String> {
    ffi::prompt_if_untrusted();
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn request_access_if_needed() -> Result<(), String> {
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
