//! Fast local speech-activity gate for recorded WAV files.
//!
//! This is intentionally conservative. Its job is to reject definite silence
//! and short impulse noise before paid transcription providers, not to replace
//! the provider's own speech/no-speech judgment.

use std::path::Path;

use earshot::Detector;
use hound::{SampleFormat, WavReader};

use crate::orchestrator::log;

const TARGET_SAMPLE_RATE: u32 = 16_000;
const VAD_FRAME_LEN: usize = 256;
const VAD_FRAME_MS: f32 = 16.0;
const MIN_AUDIO_MS: f32 = 120.0;
const MIN_SPEECH_MS: f32 = 96.0;
const MIN_CONSECUTIVE_SPEECH_MS: f32 = 64.0;
const ABS_RMS_FLOOR: f32 = 0.008;
const ABS_PEAK_FLOOR: f32 = 0.025;
const HIGH_PASS_HZ: f32 = 120.0;
const SPEECH_THRESHOLD: f32 = 0.65;
const MIN_SPEECH_ZCR: f32 = 0.006;
const MAX_SPEECH_ZCR: f32 = 0.35;

#[derive(Debug, Clone, Copy)]
struct SpeechActivity {
    duration_ms: f32,
    frame_ms: f32,
    speech_frames: usize,
    max_consecutive_speech_frames: usize,
    processed_frames: usize,
    max_rms: f32,
    max_peak: f32,
    max_score: f32,
}

impl SpeechActivity {
    fn speech_ms(self) -> f32 {
        self.speech_frames as f32 * self.frame_ms
    }

    fn max_consecutive_speech_ms(self) -> f32 {
        self.max_consecutive_speech_frames as f32 * self.frame_ms
    }

    fn has_speech(self) -> bool {
        if self.duration_ms < MIN_AUDIO_MS {
            return false;
        }

        if self.max_rms < ABS_RMS_FLOOR || self.max_peak < ABS_PEAK_FLOOR {
            return false;
        }

        self.speech_ms() >= MIN_SPEECH_MS
            && self.max_consecutive_speech_ms() >= MIN_CONSECUTIVE_SPEECH_MS
    }
}

pub fn pre_check(audio_path: &Path) -> bool {
    match read_wav_mono(audio_path)
        .and_then(|(samples, sample_rate)| analyze_samples(&samples, sample_rate))
    {
        Ok(activity) => {
            let has = activity.has_speech();
            log::info(&format!(
                "Earshot VAD pre-check: {} duration={:.0}ms speech={:.0}ms consecutive={:.0}ms frames={}/{} score={:.3} peak={:.3} rms={:.3}",
                if has { "speech likely" } else { "no speech" },
                activity.duration_ms,
                activity.speech_ms(),
                activity.max_consecutive_speech_ms(),
                activity.speech_frames,
                activity.processed_frames,
                activity.max_score,
                activity.max_peak,
                activity.max_rms
            ));
            has
        }
        Err(e) => {
            log::info(&format!(
                "Local speech pre-check failed: {e} -- allowing transcription provider to decide"
            ));
            true
        }
    }
}

fn read_wav_mono(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let mut reader = WavReader::open(path).map_err(|e| format!("failed to open WAV: {e}"))?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let sample_rate = spec.sample_rate;

    let samples = match spec.sample_format {
        SampleFormat::Int if spec.bits_per_sample <= 16 => {
            let scale = i16::MAX as f32;
            let values = reader
                .samples::<i16>()
                .map(|sample| sample.map(|s| s as f32 / scale))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("failed to read WAV samples: {e}"))?;
            downmix_interleaved(values, channels)
        }
        SampleFormat::Int => {
            let max_amplitude = ((1_i64 << (u32::from(spec.bits_per_sample) - 1)) - 1) as f32;
            let values = reader
                .samples::<i32>()
                .map(|sample| sample.map(|s| s as f32 / max_amplitude))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("failed to read WAV samples: {e}"))?;
            downmix_interleaved(values, channels)
        }
        SampleFormat::Float => {
            let values = reader
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("failed to read WAV samples: {e}"))?;
            downmix_interleaved(values, channels)
        }
    };

    Ok((samples, sample_rate))
}

fn downmix_interleaved(samples: Vec<f32>, channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples;
    }

    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn analyze_samples(samples: &[f32], sample_rate: u32) -> Result<SpeechActivity, String> {
    if sample_rate == 0 {
        return Err("sample rate is zero".to_string());
    }

    if samples.is_empty() {
        return Err("WAV has no samples".to_string());
    }

    let duration_ms = samples.len() as f32 * 1000.0 / sample_rate as f32;
    let mut vad_samples = if TARGET_SAMPLE_RATE == sample_rate {
        samples.to_vec()
    } else {
        resample_linear(samples, sample_rate, TARGET_SAMPLE_RATE)
    };

    high_pass_filter(&mut vad_samples, TARGET_SAMPLE_RATE, HIGH_PASS_HZ);

    let processed_frames = vad_samples.len() / VAD_FRAME_LEN;
    if processed_frames == 0 {
        return Err("WAV has no analyzable frames".to_string());
    }

    let mut vad = Detector::default();

    let mut speech_frames = 0;
    let mut consecutive_speech_frames = 0;
    let mut max_consecutive_speech_frames = 0;
    let mut max_rms = 0.0;
    let mut max_peak = 0.0;
    let mut max_score = 0.0;

    for frame in vad_samples.chunks_exact(VAD_FRAME_LEN) {
        let stats = frame_stats(frame);
        max_rms = f32::max(max_rms, stats.rms);
        max_peak = f32::max(max_peak, stats.peak);

        let score = vad.predict_f32(frame);
        max_score = f32::max(max_score, score);

        let is_speech =
            score >= SPEECH_THRESHOLD && stats.zcr >= MIN_SPEECH_ZCR && stats.zcr <= MAX_SPEECH_ZCR;

        if is_speech {
            speech_frames += 1;
            consecutive_speech_frames += 1;
            max_consecutive_speech_frames =
                max_consecutive_speech_frames.max(consecutive_speech_frames);
        } else {
            consecutive_speech_frames = 0;
        }
    }

    Ok(SpeechActivity {
        duration_ms,
        frame_ms: VAD_FRAME_MS,
        speech_frames,
        max_consecutive_speech_frames,
        processed_frames,
        max_rms,
        max_peak,
        max_score,
    })
}

#[derive(Debug, Clone, Copy)]
struct FrameStats {
    rms: f32,
    peak: f32,
    zcr: f32,
}

fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if samples.is_empty() || from_rate == to_rate {
        return samples.to_vec();
    }

    let output_len = ((samples.len() as f64 * to_rate as f64) / from_rate as f64).round() as usize;
    let ratio = from_rate as f64 / to_rate as f64;
    let mut out = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let source_pos = i as f64 * ratio;
        let left = source_pos.floor() as usize;
        let right = (left + 1).min(samples.len() - 1);
        let frac = (source_pos - left as f64) as f32;
        out.push(samples[left] * (1.0 - frac) + samples[right] * frac);
    }

    out
}

fn high_pass_filter(samples: &mut [f32], sample_rate: u32, cutoff_hz: f32) {
    if samples.is_empty() || cutoff_hz <= 0.0 {
        return;
    }

    let dt = 1.0 / sample_rate as f32;
    let rc = 1.0 / (std::f32::consts::TAU * cutoff_hz);
    let alpha = rc / (rc + dt);
    let mut prev_input = samples[0];
    let mut prev_output = 0.0;

    for sample in samples {
        let input = *sample;
        let output = alpha * (prev_output + input - prev_input);
        *sample = output;
        prev_input = input;
        prev_output = output;
    }
}

fn frame_stats(frame: &[f32]) -> FrameStats {
    let mut sum_sq = 0.0;
    let mut peak = 0.0;
    let mut crossings = 0;
    let mut previous = frame[0];

    for &sample in frame {
        let sample = sample.clamp(-1.0, 1.0);
        sum_sq += sample * sample;
        peak = f32::max(peak, sample.abs());

        if (previous >= 0.0 && sample < 0.0) || (previous < 0.0 && sample >= 0.0) {
            crossings += 1;
        }
        previous = sample;
    }

    FrameStats {
        rms: (sum_sq / frame.len() as f32).sqrt(),
        peak,
        zcr: crossings as f32 / frame.len().saturating_sub(1).max(1) as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: u32 = 16_000;

    #[test]
    fn rejects_silence() {
        let samples = vec![0.0; SAMPLE_RATE as usize];
        let activity = analyze_samples(&samples, SAMPLE_RATE).unwrap();

        assert!(!activity.has_speech());
    }

    #[test]
    fn rejects_short_impulse() {
        let mut samples = vec![0.0; SAMPLE_RATE as usize];
        for sample in samples.iter_mut().skip(3_000).take(80) {
            *sample = 0.8;
        }

        let activity = analyze_samples(&samples, SAMPLE_RATE).unwrap();

        assert!(!activity.has_speech());
    }

    #[test]
    fn rejects_low_frequency_bump() {
        let samples = sine_wave(35.0, 0.8, 240);
        let activity = analyze_samples(&samples, SAMPLE_RATE).unwrap();

        assert!(!activity.has_speech());
    }

    #[test]
    fn resamples_unsupported_rates() {
        let samples = vec![0.0; 44_100];
        let activity = analyze_samples(&samples, 44_100).unwrap();

        assert!(!activity.has_speech());
    }

    fn sine_wave(freq: f32, amp: f32, duration_ms: u32) -> Vec<f32> {
        let len = (SAMPLE_RATE * duration_ms / 1000) as usize;
        (0..len)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                (std::f32::consts::TAU * freq * t).sin() * amp
            })
            .collect()
    }
}
