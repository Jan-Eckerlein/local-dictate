//! Clipboard + synthetic paste shortcut (⌘V on macOS).

use crate::notice::AppNotice;
use std::thread;
use std::time::Duration;

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("clipboard: {e}"))?;
    clipboard
        .set_text(trimmed)
        .map_err(|e| format!("clipboard write: {e}"))
}

pub fn paste_text(text: &str) -> Result<(), AppNotice> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(AppNotice::warn("nothing to paste"));
    }

    copy_to_clipboard(trimmed).map_err(AppNotice::error)?;

    // Clipboard must be visible before the synthetic paste (Electron targets are picky).
    thread::sleep(Duration::from_millis(120));
    simulate_paste_shortcut().map_err(AppNotice::error)?;
    thread::sleep(Duration::from_millis(50));
    Ok(())
}

fn simulate_paste_shortcut() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return macos::post_cmd_v();
    }

    #[cfg(all(
        not(target_os = "macos"),
        not(any(target_os = "android", target_os = "ios"))
    ))]
    {
        use enigo::{Direction, Enigo, Key, Keyboard, Settings};

        let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("enigo: {e}"))?;
        enigo
            .key(Key::Control, Direction::Press)
            .map_err(|e| format!("paste key: {e}"))?;
        enigo
            .key(Key::Unicode('v'), Direction::Click)
            .map_err(|e| format!("paste key: {e}"))?;
        enigo
            .key(Key::Control, Direction::Release)
            .map_err(|e| format!("paste key: {e}"))?;
        return Ok(());
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Err("paste shortcut not supported on this platform".into())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;

    type CGEventTapLocation = u32;
    type CGKeyCode = u16;
    type CGEventFlags = u64;
    type CGEventSourceStateID = i32;

    const K_CG_HID_EVENT_TAP: CGEventTapLocation = 0;
    const K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: CGEventSourceStateID = 1;
    const K_CG_EVENT_FLAG_MASK_COMMAND: CGEventFlags = 0x00100000;

    const KEYCODE_LEFT_CMD: CGKeyCode = 0x37;
    const KEYCODE_V: CGKeyCode = 0x09;

    #[repr(C)]
    struct CGEvent {
        _opaque: [u8; 0],
    }
    type CGEventRef = *mut CGEvent;

    #[repr(C)]
    struct CGEventSource {
        _opaque: [u8; 0],
    }
    type CGEventSourceRef = *mut CGEventSource;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceCreate(state_id: CGEventSourceStateID) -> CGEventSourceRef;
        fn CGEventCreateKeyboardEvent(
            source: CGEventSourceRef,
            virtual_key: CGKeyCode,
            key_down: bool,
        ) -> CGEventRef;
        fn CGEventSetFlags(event: CGEventRef, flags: CGEventFlags);
        fn CGEventPost(tap: CGEventTapLocation, event: CGEventRef);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *const c_void);
    }

  /// HID-level Cmd+V with Command flag on modifier down (required for Electron/Chromium).
    pub fn post_cmd_v() -> Result<(), String> {
        unsafe {
            let source = CGEventSourceCreate(K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE);
            if source.is_null() {
                return Err("CGEventSourceCreate failed".into());
            }

            let sequence: [(CGKeyCode, bool, CGEventFlags); 4] = [
                (KEYCODE_LEFT_CMD, true, K_CG_EVENT_FLAG_MASK_COMMAND),
                (KEYCODE_V, true, K_CG_EVENT_FLAG_MASK_COMMAND),
                (KEYCODE_V, false, K_CG_EVENT_FLAG_MASK_COMMAND),
                (KEYCODE_LEFT_CMD, false, 0),
            ];

            for (keycode, key_down, flags) in sequence {
                let event = CGEventCreateKeyboardEvent(source, keycode, key_down);
                if event.is_null() {
                    CFRelease(source as *const c_void);
                    return Err("CGEventCreateKeyboardEvent failed".into());
                }
                CGEventSetFlags(event, flags);
                CGEventPost(K_CG_HID_EVENT_TAP, event);
                CFRelease(event as *const c_void);
            }

            CFRelease(source as *const c_void);
        }
        Ok(())
    }
}
