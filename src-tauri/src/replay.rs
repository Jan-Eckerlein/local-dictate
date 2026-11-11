use crate::segment::{resample_linear, WHISPER_SAMPLE_RATE};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
#[cfg(not(target_os = "macos"))]
use std::thread;
use std::time::Duration;

/// Play 16 kHz mono f32 samples on the default output device (debug).
pub fn replay_samples_16k_mono(samples: &[f32]) -> Result<(), String> {
    if samples.is_empty() {
        return Err("segment is empty".into());
    }

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no default output device".to_string())?;

    let supported = device
        .default_output_config()
        .map_err(|e| format!("output config: {e}"))?;

    let out_rate = supported.sample_rate().0;
    let channels = supported.channels() as usize;
    let sample_format = supported.sample_format();
    let stream_config: StreamConfig = supported.into();

    let pcm = resample_linear(samples, WHISPER_SAMPLE_RATE, out_rate);
    let pcm = Arc::new(StdMutex::new(pcm));
    let frame_idx = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicBool::new(false));

    let err_fn = |err| eprintln!("replay stream error: {err}");

    let stream = match sample_format {
        SampleFormat::F32 => {
            let idx = frame_idx.clone();
            let buf = pcm.clone();
            let done = finished.clone();
            let ch = channels;
            device.build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| {
                    write_output_f32(data, ch, &idx, &buf, &done);
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let idx = frame_idx.clone();
            let buf = pcm.clone();
            let done = finished.clone();
            let ch = channels;
            device.build_output_stream(
                &stream_config,
                move |data: &mut [i16], _| {
                    let mut scratch = vec![0.0_f32; data.len()];
                    write_output_f32(&mut scratch, ch, &idx, &buf, &done);
                    for (out, sample) in data.iter_mut().zip(scratch.iter()) {
                        *out = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                    }
                },
                err_fn,
                None,
            )
        }
        other => return Err(format!("unsupported output format: {other:?}")),
    }
    .map_err(|e| format!("build output stream: {e}"))?;

    stream
        .play()
        .map_err(|e| format!("start replay: {e}"))?;

    wait_until(&finished);
    drop(stream);
    Ok(())
}

fn write_output_f32(
    data: &mut [f32],
    channels: usize,
    idx: &AtomicUsize,
    buf: &StdMutex<Vec<f32>>,
    done: &AtomicBool,
) {
    let guard = buf.lock().expect("replay pcm lock");
    let ch = channels.max(1);
    let mut i = idx.load(Ordering::Relaxed);
    let frames = data.len() / ch;
    for f in 0..frames {
        let sample = if i < guard.len() { guard[i] } else { 0.0 };
        let base = f * ch;
        for c in 0..ch {
            data[base + c] = sample;
        }
        i += 1;
    }
    idx.store(i, Ordering::Relaxed);
    if i >= guard.len() {
        done.store(true, Ordering::Relaxed);
    }
}

fn wait_until(done: &AtomicBool) {
    while !done.load(Ordering::Relaxed) {
        #[cfg(target_os = "macos")]
        {
            use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
            unsafe {
                CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, Duration::from_millis(50), true);
            }
        }
        #[cfg(not(target_os = "macos"))]
        thread::sleep(Duration::from_millis(20));
    }
}
