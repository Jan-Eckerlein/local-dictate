//! macOS microphone permission (TCC) before cpal.

#[cfg(target_os = "macos")]
pub fn authorization_status() -> &'static str {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

    let Some(media_type) = (unsafe { AVMediaTypeAudio.as_ref() }) else {
        return "unknown";
    };

    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) } {
        AVAuthorizationStatus::Authorized => "authorized",
        AVAuthorizationStatus::Denied => "denied",
        AVAuthorizationStatus::Restricted => "restricted",
        AVAuthorizationStatus::NotDetermined => "not_determined",
        _ => "unknown",
    }
}

#[cfg(not(target_os = "macos"))]
pub fn authorization_status() -> &'static str {
    "authorized"
}

#[cfg(target_os = "macos")]
pub fn ensure_access() -> Result<(), String> {
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    use std::sync::mpsc;
    use std::time::Duration;

    let media_type = unsafe { AVMediaTypeAudio.as_ref() }
        .ok_or_else(|| "AVMediaTypeAudio unavailable".to_string())?;

    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) } {
        AVAuthorizationStatus::Authorized => Ok(()),
        AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => {
            Err("Microphone access denied.".into())
        }
        AVAuthorizationStatus::NotDetermined => {
            let (tx, rx) = mpsc::channel();
            let handler = block2::RcBlock::new(move |granted: Bool| {
                let _ = tx.send(granted.as_bool());
            });
            unsafe {
                AVCaptureDevice::requestAccessForMediaType_completionHandler(
                    media_type,
                    &handler,
                );
            }
            match rx.recv_timeout(Duration::from_secs(120)) {
                Ok(true) => Ok(()),
                Ok(false) => Err("Microphone access denied.".into()),
                Err(_) => Err("Microphone permission dialog timed out.".into()),
            }
        }
        _ => Ok(()),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn ensure_access() -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn open_privacy_settings() -> Result<(), String> {
    std::process::Command::new("open")
        .arg(
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Microphone",
        )
        .spawn()
        .map_err(|e| format!("open settings: {e}"))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn open_privacy_settings() -> Result<(), String> {
    Err("macOS only".into())
}
