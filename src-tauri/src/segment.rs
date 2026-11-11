//! Resample to 16 kHz mono and RMS-based voice activity segmentation (Whisper input format).

use std::collections::VecDeque;

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
/// Audio kept before speech onset is confirmed (helps first syllables for Whisper).
pub const VAD_PREROLL_MS: u32 = 300;
/// Consecutive speech frames required before recording starts.
pub const VAD_SPEECH_ONSET_FRAMES: u32 = 3;

/// Peak-normalize quiet segments before Whisper (skip if already loud or near-silent).
pub const STT_NORMALIZE_PEAK_TARGET: f32 = 0.9;
/// Below this peak → scale up toward `STT_NORMALIZE_PEAK_TARGET`.
pub const STT_NORMALIZE_SKIP_ABOVE: f32 = 0.5;
pub const STT_NORMALIZE_SILENCE_PEAK: f32 = 1e-5;

/// Scale f32 PCM toward `STT_NORMALIZE_PEAK_TARGET` when the segment is quiet but not silent.
pub fn normalize_peak_for_stt(samples: &[f32]) -> Vec<f32> {
    let peak = samples.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
    if peak < STT_NORMALIZE_SILENCE_PEAK || peak >= STT_NORMALIZE_SKIP_ABOVE {
        return samples.to_vec();
    }
    let gain = STT_NORMALIZE_PEAK_TARGET / peak;
    samples
        .iter()
        .map(|&s| (s * gain).clamp(-1.0, 1.0))
        .collect()
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSegmentPayload {
    pub index: usize,
    pub sample_rate: u32,
    pub sample_count: usize,
    pub duration_secs: f32,
}

impl AudioSegmentPayload {
    pub fn from_segment(samples: &[f32], index: usize) -> Self {
        let count = samples.len();
        Self {
            index,
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
    preroll_samples: usize,
    speech_onset_frames: usize,
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
            preroll_samples: rate * VAD_PREROLL_MS as usize / 1000,
            speech_onset_frames: VAD_SPEECH_ONSET_FRAMES as usize,
        }
    }
}

pub struct VadProcessor {
    config: VadConfig,
    pending: Vec<f32>,
    recording: bool,
    recording_buffer: Vec<f32>,
    silence_samples: usize,
    preroll: VecDeque<f32>,
    speech_streak: usize,
}

impl VadProcessor {
    pub fn with_rms_threshold(rms_threshold: f32) -> Self {
        let mut config = VadConfig::default();
        config.rms_threshold = rms_threshold;
        Self {
            config,
            pending: Vec::new(),
            recording: false,
            recording_buffer: Vec::new(),
            silence_samples: 0,
            preroll: VecDeque::new(),
            speech_streak: 0,
        }
    }

    fn push_preroll(&mut self, frame: &[f32]) {
        for &sample in frame {
            if self.preroll.len() >= self.config.preroll_samples {
                self.preroll.pop_front();
            }
            self.preroll.push_back(sample);
        }
    }

    fn begin_recording(&mut self) {
        self.recording = true;
        self.recording_buffer.clear();
        self.recording_buffer.reserve(self.preroll.len());
        self.recording_buffer.extend(self.preroll.iter().copied());
        self.silence_samples = 0;
        self.speech_streak = 0;
    }

    /// Push 16 kHz mono samples; returns completed segments (may be empty).
    pub fn push_resampled(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        self.pending.extend_from_slice(samples);
        let mut done = Vec::new();

        while self.pending.len() >= self.config.frame_samples {
            let frame: Vec<f32> = self.pending.drain(..self.config.frame_samples).collect();
            self.push_preroll(&frame);
            let rms = frame_rms(&frame);
            let speech = rms >= self.config.rms_threshold;

            if speech {
                if !self.recording {
                    self.speech_streak += 1;
                    if self.speech_streak >= self.config.speech_onset_frames {
                        self.begin_recording();
                    }
                } else {
                    self.silence_samples = 0;
                    self.recording_buffer.extend_from_slice(&frame);
                }
            } else if self.recording {
                self.speech_streak = 0;
                self.silence_samples += self.config.frame_samples;
                self.recording_buffer.extend_from_slice(&frame);

                if self.silence_samples >= self.config.silence_hang_samples {
                    if let Some(seg) = self.take_segment() {
                        done.push(seg);
                    }
                    self.recording = false;
                    self.silence_samples = 0;
                }
            } else {
                self.speech_streak = 0;
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

    #[test]
    fn normalize_quiet_segment() {
        let quiet = vec![0.05_f32; 400];
        let out = normalize_peak_for_stt(&quiet);
        let peak = out.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
        assert!((peak - STT_NORMALIZE_PEAK_TARGET).abs() < 0.01);
    }

    #[test]
    fn normalize_skips_loud_segment() {
        let loud = vec![0.8_f32; 400];
        let out = normalize_peak_for_stt(&loud);
        assert_eq!(out, loud);
    }

    #[test]
    fn vad_preroll_prefixes_segment() {
        let rate = WHISPER_SAMPLE_RATE as usize;
        let frame_samples = rate * VAD_FRAME_MS as usize / 1000;
        let preroll_samples = rate * VAD_PREROLL_MS as usize / 1000;
        let onset = VAD_SPEECH_ONSET_FRAMES as usize;

        let mut vad = VadProcessor::with_rms_threshold(0.001);
        let loud = 0.05_f32;

        // Fill preroll with quiet audio, then trigger onset with loud frames.
        for _ in 0..(preroll_samples / frame_samples + 2) {
            let quiet: Vec<f32> = vec![0.0; frame_samples];
            let _ = vad.push_resampled(&quiet);
        }

        let mut segments = Vec::new();
        for _ in 0..onset {
            let frame: Vec<f32> = vec![loud; frame_samples];
            segments.extend(vad.push_resampled(&frame));
        }
        // Close segment with silence hang (many quiet frames).
        let hang_frames = VAD_SILENCE_HANG_MS / VAD_FRAME_MS + 2;
        for _ in 0..hang_frames {
            let quiet: Vec<f32> = vec![0.0; frame_samples];
            segments.extend(vad.push_resampled(&quiet));
        }

        let seg = segments
            .last()
            .expect("expected one completed segment after silence hang");
        assert!(
            seg.len() >= preroll_samples.saturating_sub(frame_samples),
            "segment should include roughly one preroll window (onset frames are inside the ring)"
        );
    }
}
