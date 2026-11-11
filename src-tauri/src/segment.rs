//! Resample to 16 kHz mono and RMS-based voice activity segmentation (Whisper input format).

pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// RMS above this → speech (tune per mic; documented for README).
pub const VAD_RMS_THRESHOLD: f32 = 0.008;
/// Trailing silence before a segment is finalized.
pub const VAD_SILENCE_HANG_MS: u32 = 700;
/// Hard cap on one utterance.
pub const VAD_MAX_SEGMENT_MS: u32 = 30_000;
/// Ignore shorter clips (clicks / noise).
pub const VAD_MIN_SEGMENT_MS: u32 = 200;
/// Analysis frame size at 16 kHz.
pub const VAD_FRAME_MS: u32 = 20;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSegmentPayload {
    pub sample_rate: u32,
    pub sample_count: usize,
    pub duration_secs: f32,
}

impl AudioSegmentPayload {
    pub fn from_segment(samples: &[f32]) -> Self {
        let count = samples.len();
        Self {
            sample_rate: WHISPER_SAMPLE_RATE,
            sample_count: count,
            duration_secs: count as f32 / WHISPER_SAMPLE_RATE as f32,
        }
    }
}

pub fn interleaved_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    let ch = channels.max(1);
    if ch == 1 {
        return samples.to_vec();
    }
    let frames = samples.len() / ch;
    let mut out = Vec::with_capacity(frames);
    for frame in 0..frames {
        let base = frame * ch;
        let sum: f32 = (0..ch).map(|c| samples[base + c]).sum();
        out.push(sum / ch as f32);
    }
    out
}

/// Linear interpolation resample (good enough for speech → Whisper).
pub fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (samples.len() as f64 / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src = i as f64 * ratio;
        let idx = src.floor() as usize;
        let frac = (src - idx as f64) as f32;
        let i0 = idx.min(samples.len() - 1);
        let i1 = (idx + 1).min(samples.len() - 1);
        let s0 = samples[i0];
        let s1 = samples[i1];
        out.push(s0 + (s1 - s0) * frac);
    }
    out
}

fn frame_rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = frame.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / frame.len() as f64).sqrt() as f32
}

struct VadConfig {
    rms_threshold: f32,
    silence_hang_samples: usize,
    max_segment_samples: usize,
    min_segment_samples: usize,
    frame_samples: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        let rate = WHISPER_SAMPLE_RATE as usize;
        Self {
            rms_threshold: VAD_RMS_THRESHOLD,
            silence_hang_samples: rate * VAD_SILENCE_HANG_MS as usize / 1000,
            max_segment_samples: rate * VAD_MAX_SEGMENT_MS as usize / 1000,
            min_segment_samples: rate * VAD_MIN_SEGMENT_MS as usize / 1000,
            frame_samples: rate * VAD_FRAME_MS as usize / 1000,
        }
    }
}

pub struct VadProcessor {
    config: VadConfig,
    pending: Vec<f32>,
    recording: bool,
    recording_buffer: Vec<f32>,
    silence_samples: usize,
}

impl VadProcessor {
    pub fn new() -> Self {
        Self {
            config: VadConfig::default(),
            pending: Vec::new(),
            recording: false,
            recording_buffer: Vec::new(),
            silence_samples: 0,
        }
    }

    /// Push 16 kHz mono samples; returns completed segments (may be empty).
    pub fn push_resampled(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        self.pending.extend_from_slice(samples);
        let mut done = Vec::new();

        while self.pending.len() >= self.config.frame_samples {
            let frame: Vec<f32> = self.pending.drain(..self.config.frame_samples).collect();
            let rms = frame_rms(&frame);
            let speech = rms >= self.config.rms_threshold;

            if speech {
                if !self.recording {
                    self.recording = true;
                    self.recording_buffer.clear();
                }
                self.silence_samples = 0;
                self.recording_buffer.extend_from_slice(&frame);
            } else if self.recording {
                self.silence_samples += self.config.frame_samples;
                self.recording_buffer.extend_from_slice(&frame);

                if self.silence_samples >= self.config.silence_hang_samples {
                    if let Some(seg) = self.take_segment() {
                        done.push(seg);
                    }
                    self.recording = false;
                    self.silence_samples = 0;
                }
            }

            if self.recording && self.recording_buffer.len() >= self.config.max_segment_samples {
                if let Some(seg) = self.take_segment() {
                    done.push(seg);
                }
                self.recording = false;
                self.silence_samples = 0;
            }
        }

        done
    }

    fn take_segment(&mut self) -> Option<Vec<f32>> {
        let trim = self.silence_samples.min(self.recording_buffer.len());
        let end = self.recording_buffer.len().saturating_sub(trim);
        if end < self.config.min_segment_samples {
            self.recording_buffer.clear();
            return None;
        }
        let seg = self.recording_buffer[..end].to_vec();
        self.recording_buffer.clear();
        Some(seg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_identity() {
        let s: Vec<f32> = (0..480).map(|i| (i as f32).sin()).collect();
        let out = resample_linear(&s, 48_000, 48_000);
        assert_eq!(out.len(), s.len());
    }

    #[test]
    fn resample_downsample_ratio() {
        let s = vec![0.0_f32; 4800];
        let out = resample_linear(&s, 48_000, 16_000);
        assert_eq!(out.len(), 1600);
    }
}
