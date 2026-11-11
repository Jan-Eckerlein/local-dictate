use crate::segment::{
    interleaved_to_mono, resample_linear, AudioSegmentPayload, VadProcessor, WHISPER_SAMPLE_RATE,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex as StdMutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioLevelPayload {
    pub level: f32,
    pub rms: f32,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioMonitorInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
    pub device_name: String,
}

struct SharedLevel {
    bits: AtomicU32,
}

impl SharedLevel {
    fn new() -> Self {
        Self {
            bits: AtomicU32::new(0),
        }
    }

    fn store(&self, rms: f32, peak: f32) {
        let level = rms.max(peak).clamp(0.0, 1.0);
        self.bits.store(level.to_bits(), Ordering::Relaxed);
    }

    fn load(&self) -> AudioLevelPayload {
        let level = f32::from_bits(self.bits.load(Ordering::Relaxed));
        AudioLevelPayload { level, rms: level }
    }
}

fn rms_and_peak_f32(samples: &[f32], channels: usize) -> (f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    let ch = channels.max(1);
    let frames = samples.len() / ch;
    if frames == 0 {
        return (0.0, 0.0);
    }

    let mut sum_sq = 0.0_f64;
    let mut peak = 0.0_f32;

    for frame in 0..frames {
        let base = frame * ch;
        let mut frame_sq = 0.0_f64;
        for c in 0..ch {
            let s = samples[base + c];
            peak = peak.max(s.abs());
            frame_sq += (s as f64) * (s as f64);
        }
        sum_sq += frame_sq / ch as f64;
    }

    let rms = (sum_sq / frames as f64).sqrt() as f32;
    (rms, peak)
}

fn samples_to_f32_i16(samples: &[i16]) -> Vec<f32> {
    samples
        .iter()
        .map(|&s| s as f32 / i16::MAX as f32)
        .collect()
}

/// macOS: cpal’s thread must run a CoreAudio run loop or the input callback never fires.
#[cfg(target_os = "macos")]
fn wait_while_running(stop: &AtomicBool) {
    use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
    while !stop.load(Ordering::Relaxed) {
        unsafe {
            CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, Duration::from_millis(250), true);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn wait_while_running(stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(100));
    }
}

pub fn list_input_devices() -> Result<Vec<InputDeviceInfo>, String> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok());

    let devices = host
        .input_devices()
        .map_err(|e| format!("list input devices: {e}"))?;

    let mut out = Vec::new();
    for device in devices {
        let name = device.name().map_err(|e| format!("device name: {e}"))?;
        let is_default = default_name.as_deref() == Some(name.as_str());
        out.push(InputDeviceInfo { name, is_default });
    }
    Ok(out)
}

fn resolve_input_device(
    host: &cpal::Host,
    device_name: Option<&str>,
) -> Result<cpal::Device, String> {
    match device_name {
        None => host
            .default_input_device()
            .ok_or_else(|| "no default input device".to_string()),
        Some(name) if name.is_empty() => host
            .default_input_device()
            .ok_or_else(|| "no default input device".to_string()),
        Some(name) => {
            for device in host
                .input_devices()
                .map_err(|e| format!("list input devices: {e}"))?
            {
                if device.name().map_err(|e| format!("device name: {e}"))? == name {
                    return Ok(device);
                }
            }
            Err(format!("input device not found: {name}"))
        }
    }
}

pub fn start_monitor(
    app: AppHandle,
    stop: Arc<AtomicBool>,
    device_name: Option<String>,
    session_segments: Arc<StdMutex<Vec<Vec<f32>>>>,
) -> Result<AudioMonitorInfo, String> {
    let host = cpal::default_host();
    let device = resolve_input_device(&host, device_name.as_deref())?;

    let device_name = device.name().unwrap_or_else(|_| "unknown".into());

    let supported = device
        .default_input_config()
        .map_err(|e| format!("input config: {e}"))?;

    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let stream_config: StreamConfig = supported.clone().into();

    let info = AudioMonitorInfo {
        sample_rate,
        channels,
        sample_format: format!("{sample_format:?}"),
        device_name,
    };

    let level = Arc::new(SharedLevel::new());

    let (chunk_tx, chunk_rx) = mpsc::channel::<Vec<f32>>();
    let chunk_tx = Arc::new(StdMutex::new(Some(chunk_tx)));

    let stop_emit = stop.clone();
    let level_emit = level.clone();
    let app_emit = app.clone();
    thread::spawn(move || {
        while !stop_emit.load(Ordering::Relaxed) {
            let _ = app_emit.emit("audio-level", level_emit.load());
            thread::sleep(Duration::from_millis(33));
        }
    });

    let stop_vad = stop.clone();
    let app_vad = app.clone();
    let segments_store = session_segments;
    thread::spawn(move || {
        let mut vad = VadProcessor::new();
        while !stop_vad.load(Ordering::Relaxed) {
            match chunk_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(chunk) => {
                    let resampled = resample_linear(&chunk, sample_rate, WHISPER_SAMPLE_RATE);
                    for seg in vad.push_resampled(&resampled) {
                        let payload = if let Ok(mut guard) = segments_store.lock() {
                            guard.push(seg.clone());
                            let index = guard.len() - 1;
                            AudioSegmentPayload::from_segment(&seg, index)
                        } else {
                            continue;
                        };
                        let _ = app_vad.emit("audio-segment", payload);
                        // Milestone 6+: hand `seg` to whisper worker via mpsc.
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    let stop_capture = stop.clone();
    let level_capture = level;
    let ch = channels as usize;

    thread::spawn(move || {
        let err_fn = |err| eprintln!("audio stream error: {err}");

        let level_f32 = level_capture.clone();
        let level_i16 = level_capture;
        let chunk_tx_f32 = chunk_tx.clone();
        let chunk_tx_i16 = chunk_tx.clone();
        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    let (rms, peak) = rms_and_peak_f32(data, ch);
                    level_f32.store(rms, peak);
                    let mono = interleaved_to_mono(data, ch);
                    if let Ok(guard) = chunk_tx_f32.lock() {
                        if let Some(tx) = guard.as_ref() {
                            let _ = tx.send(mono);
                        }
                    }
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    let buf = samples_to_f32_i16(data);
                    let (rms, peak) = rms_and_peak_f32(&buf, ch);
                    level_i16.store(rms, peak);
                    let mono = interleaved_to_mono(&buf, ch);
                    if let Ok(guard) = chunk_tx_i16.lock() {
                        if let Some(tx) = guard.as_ref() {
                            let _ = tx.send(mono);
                        }
                    }
                },
                err_fn,
                None,
            ),
            other => {
                eprintln!("unsupported sample format: {other:?}");
                return;
            }
        };

        let Ok(stream) = stream else {
            eprintln!("failed to build input stream");
            return;
        };
        if let Err(e) = stream.play() {
            eprintln!("failed to start stream: {e}");
            return;
        }

        wait_while_running(&stop_capture);
        drop(stream);
        if let Ok(mut guard) = chunk_tx.lock() {
            *guard = None;
        }
    });

    Ok(info)
}

pub fn stop_monitor(stop: &Arc<AtomicBool>) {
    stop.store(true, Ordering::Relaxed);
}
