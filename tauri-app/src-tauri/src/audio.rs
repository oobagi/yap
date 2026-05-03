use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(target_os = "windows")]
use cpal::{BufferSize, SupportedBufferSize};
use hound::{SampleFormat, WavSpec, WavWriter};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// Current real-time audio levels sent to the overlay.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioLevels {
    /// Overall RMS level (0.0 - 1.0).
    pub level: f32,
    /// 11 display bars derived from 6 FFT bands mirrored.
    pub bars: [f32; 11],
}

impl Default for AudioLevels {
    fn default() -> Self {
        Self {
            level: 0.0,
            bars: [0.0; 11],
        }
    }
}

/// Number of raw frequency bands computed via FFT.
const RAW_BAND_COUNT: usize = 6;

/// FFT size (must be power of 2).
const FFT_SIZE: usize = 1024;
#[cfg(target_os = "windows")]
const LOW_LATENCY_BUFFER_FRAMES: u32 = 256;

/// Shared audio levels (written by the callback thread, read by the main thread).
static CURRENT_LEVELS: once_cell::sync::Lazy<Arc<Mutex<AudioLevels>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(AudioLevels::default())));

/// Shared peak level.
static PEAK_LEVEL: once_cell::sync::Lazy<Arc<Mutex<f32>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(0.0)));

/// Shared WAV writer (written to by the callback, finalized on stop).
static WAV_WRITER: once_cell::sync::Lazy<Arc<Mutex<Option<WavWriter<BufWriter<std::fs::File>>>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

/// Flag to signal the recording thread to stop.
static STOP_FLAG: AtomicBool = AtomicBool::new(false);

/// Whether recording is active.
static RECORDING: AtomicBool = AtomicBool::new(false);

/// Whether recording is paused (hands-free mode).
static PAUSED: AtomicBool = AtomicBool::new(false);

/// The WAV file path for the current/last recording.
static WAV_PATH: once_cell::sync::Lazy<Mutex<Option<PathBuf>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

/// Start recording from the configured input device, or the system default.
///
/// Audio is written to a temporary 16-bit PCM WAV file. Real-time FFT
/// levels are computed and can be polled via `get_levels()`.
pub fn start_recording(device_name: Option<&str>) -> Result<PathBuf, String> {
    if RECORDING.load(Ordering::SeqCst) {
        return Err("recording already in progress".into());
    }

    let host = cpal::default_host();
    let device = resolve_input_device(&host, device_name)?;
    let resolved_device_name = device
        .name()
        .unwrap_or_else(|_| "unknown input device".to_string());

    let config = device
        .default_input_config()
        .map_err(|e| format!("failed to get input config: {e}"))?;

    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as u32;
    let sample_format = config.sample_format();
    let mut stream_config: cpal::StreamConfig = config.clone().into();
    configure_low_latency_input(&config, &mut stream_config);

    log_audio(&format!(
        "Starting audio: device={resolved_device_name:?}, sample_rate={sample_rate}, channels={channels}, sample_format={sample_format:?}, buffer_size={:?}",
        stream_config.buffer_size
    ));

    let wav_path = std::env::temp_dir().join("yap_recording.wav");

    let spec = WavSpec {
        channels: 1, // mono output
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let writer = WavWriter::create(&wav_path, spec)
        .map_err(|e| format!("failed to create WAV file: {e}"))?;

    // Store the writer and path
    if let Ok(mut w) = WAV_WRITER.lock() {
        *w = Some(writer);
    }
    if let Ok(mut p) = WAV_PATH.lock() {
        *p = Some(wav_path.clone());
    }

    // Reset state
    if let Ok(mut l) = CURRENT_LEVELS.lock() {
        *l = AudioLevels::default();
    }
    if let Ok(mut p) = PEAK_LEVEL.lock() {
        *p = 0.0;
    }
    STOP_FLAG.store(false, Ordering::SeqCst);
    PAUSED.store(false, Ordering::SeqCst);
    RECORDING.store(true, Ordering::SeqCst);

    let writer_ref = WAV_WRITER.clone();
    let levels_ref = CURRENT_LEVELS.clone();
    let peak_ref = PEAK_LEVEL.clone();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

    // Build the stream on a dedicated thread (cpal::Stream is !Send,
    // so it must live on the thread that created it).
    let thread = std::thread::Builder::new()
        .name("yap-audio".into())
        .spawn(move || {
            let err_fn = |err: cpal::StreamError| {
                log_audio(&format!("audio stream error: {err}"));
            };

            // Accumulate samples for FFT across callbacks
            let fft_buffer: Arc<Mutex<Vec<f32>>> =
                Arc::new(Mutex::new(Vec::with_capacity(FFT_SIZE)));

            let writer_cb = writer_ref.clone();
            let levels_cb = levels_ref.clone();
            let peak_cb = peak_ref.clone();
            let fft_buf_cb = fft_buffer.clone();
            let sr = sample_rate;

            let stream_result = match sample_format {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &stream_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        process_audio_callback(
                            data,
                            channels,
                            sr,
                            &writer_cb,
                            &levels_cb,
                            &peak_cb,
                            &fft_buf_cb,
                        );
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if STOP_FLAG.load(Ordering::Relaxed) {
                            return;
                        }
                        // Convert i16 to f32
                        let f32_data: Vec<f32> =
                            data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                        process_audio_callback(
                            &f32_data,
                            channels,
                            sr,
                            &writer_cb,
                            &levels_cb,
                            &peak_cb,
                            &fft_buf_cb,
                        );
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_input_stream(
                    &stream_config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        if STOP_FLAG.load(Ordering::Relaxed) {
                            return;
                        }
                        // Convert u16 to f32
                        let f32_data: Vec<f32> = data
                            .iter()
                            .map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0)
                            .collect();
                        process_audio_callback(
                            &f32_data,
                            channels,
                            sr,
                            &writer_cb,
                            &levels_cb,
                            &peak_cb,
                            &fft_buf_cb,
                        );
                    },
                    err_fn,
                    None,
                ),
                _ => {
                    let message = format!("unsupported input sample format: {sample_format:?}");
                    log_audio(&message);
                    RECORDING.store(false, Ordering::SeqCst);
                    let _ = ready_tx.send(Err(message));
                    return;
                }
            };

            let stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    let message = format!("failed to build input stream: {e}");
                    log_audio(&message);
                    RECORDING.store(false, Ordering::SeqCst);
                    let _ = ready_tx.send(Err(message));
                    return;
                }
            };

            if let Err(e) = stream.play() {
                let message = format!("failed to start audio stream: {e}");
                log_audio(&message);
                RECORDING.store(false, Ordering::SeqCst);
                let _ = ready_tx.send(Err(message));
                return;
            }

            let _ = ready_tx.send(Ok(()));

            // Keep the stream alive until stop is signaled
            while !STOP_FLAG.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }

            // Stream is dropped here, stopping audio capture
            drop(stream);

            RECORDING.store(false, Ordering::SeqCst);
        })
        .map_err(|e| format!("failed to spawn audio thread: {e}"))?;

    match ready_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            cleanup_failed_start();
            return Err(e);
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            STOP_FLAG.store(true, Ordering::SeqCst);
            cleanup_failed_start();
            return Err("audio stream start timed out".to_string());
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            cleanup_failed_start();
            return Err("audio thread exited before starting".to_string());
        }
    }

    drop(thread);

    Ok(wav_path)
}

fn resolve_input_device(
    host: &cpal::Host,
    device_name: Option<&str>,
) -> Result<cpal::Device, String> {
    let requested = device_name.map(str::trim).filter(|name| !name.is_empty());

    if let Some(name) = requested {
        match host.input_devices() {
            Ok(mut devices) => {
                if let Some(device) = devices.find(|device| {
                    device
                        .name()
                        .map(|candidate| candidate == name)
                        .unwrap_or(false)
                }) {
                    return Ok(device);
                }
                log_audio(&format!(
                    "Configured input device {name:?} was not found; falling back to system default"
                ));
            }
            Err(e) => {
                log_audio(&format!(
                    "Failed to enumerate input devices while looking for {name:?}: {e}"
                ));
            }
        }
    }

    host.default_input_device()
        .ok_or_else(|| "no default input device available".to_string())
}

fn configure_low_latency_input(
    supported: &cpal::SupportedStreamConfig,
    stream_config: &mut cpal::StreamConfig,
) {
    #[cfg(target_os = "windows")]
    {
        match *supported.buffer_size() {
            SupportedBufferSize::Range { min, max } => {
                let frames = LOW_LATENCY_BUFFER_FRAMES.clamp(min, max);
                stream_config.buffer_size = BufferSize::Fixed(frames);
            }
            SupportedBufferSize::Unknown => {
                stream_config.buffer_size = BufferSize::Default;
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = supported;
        let _ = stream_config;
    }
}

fn cleanup_failed_start() {
    RECORDING.store(false, Ordering::SeqCst);
    PAUSED.store(false, Ordering::SeqCst);
    STOP_FLAG.store(true, Ordering::SeqCst);

    if let Ok(mut guard) = WAV_WRITER.lock() {
        if let Some(writer) = guard.take() {
            let _ = writer.finalize();
        }
    }

    if let Ok(mut path) = WAV_PATH.lock() {
        *path = None;
    }

    if let Ok(mut levels) = CURRENT_LEVELS.lock() {
        *levels = AudioLevels::default();
    }

    if let Ok(mut peak) = PEAK_LEVEL.lock() {
        *peak = 0.0;
    }
}

fn log_audio(message: &str) {
    eprintln!("[yap audio] {message}");

    if let Ok(dir) = crate::config::config_dir() {
        let path = dir.join("debug.log");
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let line = format!("[{timestamp}] [audio] {message}\n");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
    }
}

/// Process incoming audio data from the input stream callback.
fn process_audio_callback(
    data: &[f32],
    channels: u32,
    sample_rate: u32,
    writer: &Arc<Mutex<Option<WavWriter<BufWriter<std::fs::File>>>>>,
    levels: &Arc<Mutex<AudioLevels>>,
    peak: &Arc<Mutex<f32>>,
    fft_buffer: &Arc<Mutex<Vec<f32>>>,
) {
    if STOP_FLAG.load(Ordering::Relaxed) {
        return;
    }

    // Convert to mono f32 samples
    let mono_samples: Vec<f32> = data
        .chunks(channels as usize)
        .map(|frame| {
            let sum: f32 = frame.iter().sum();
            sum / channels as f32
        })
        .collect();

    // Write to WAV if not paused
    if !PAUSED.load(Ordering::Relaxed) && !STOP_FLAG.load(Ordering::Relaxed) {
        if let Ok(mut guard) = writer.lock() {
            if let Some(ref mut w) = *guard {
                for &sample in &mono_samples {
                    if PAUSED.load(Ordering::Relaxed) || STOP_FLAG.load(Ordering::Relaxed) {
                        break;
                    }
                    let s16 =
                        (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                    let _ = w.write_sample(s16);
                }
            }
        }
    }

    // Compute RMS level
    let frame_count = mono_samples.len();
    if frame_count == 0 || STOP_FLAG.load(Ordering::Relaxed) {
        return;
    }

    let sum_sq: f32 = mono_samples.iter().map(|s| s * s).sum();
    let rms = (sum_sq / frame_count as f32).sqrt();
    let level = (rms * 18.0).min(1.0);

    // Update peak
    if let Ok(mut p) = peak.lock() {
        if level > *p {
            *p = level;
        }
    }

    // Accumulate samples for FFT
    if let Ok(mut buf) = fft_buffer.lock() {
        buf.extend_from_slice(&mono_samples);

        // Process when we have enough samples
        if buf.len() >= FFT_SIZE {
            let fft_samples: Vec<f32> = buf[..FFT_SIZE].to_vec();
            buf.drain(..FFT_SIZE);

            let raw_bands = compute_bands(&fft_samples, sample_rate as f32);
            let mirrored = mirror_bands(&raw_bands);

            if let Ok(mut l) = levels.lock() {
                l.level = level;
                l.bars = mirrored;
            }
        } else {
            // Still update the level even without FFT
            if let Ok(mut l) = levels.lock() {
                l.level = level;
            }
        }

        // Prevent unbounded growth
        if buf.len() > FFT_SIZE * 4 {
            let drain_to = buf.len() - FFT_SIZE;
            buf.drain(..drain_to);
        }
    }
}

/// Stop the current recording and return the path to the WAV file.
pub fn stop_recording() -> Result<PathBuf, String> {
    if !RECORDING.load(Ordering::SeqCst) {
        return Err("no active recording session".into());
    }

    // Signal the recording thread to stop
    STOP_FLAG.store(true, Ordering::SeqCst);

    // Wait briefly for the thread to acknowledge
    for _ in 0..20 {
        if !RECORDING.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    // Finalize the WAV writer
    if let Ok(mut guard) = WAV_WRITER.lock() {
        if let Some(writer) = guard.take() {
            writer
                .finalize()
                .map_err(|e| format!("failed to finalize WAV: {e}"))?;
        }
    }

    let path = WAV_PATH
        .lock()
        .ok()
        .and_then(|p| p.clone())
        .ok_or_else(|| "no WAV path available".to_string())?;

    Ok(path)
}

/// Pause audio capture (hands-free mode). Engine keeps running for levels.
pub fn pause_recording() {
    PAUSED.store(true, Ordering::SeqCst);
}

/// Resume audio capture after a pause.
pub fn resume_recording() {
    PAUSED.store(false, Ordering::SeqCst);
}

/// Get the current real-time audio levels (for overlay rendering).
pub fn get_levels() -> AudioLevels {
    CURRENT_LEVELS.lock().map(|l| l.clone()).unwrap_or_default()
}

/// List available audio input devices by name.
pub fn list_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => {
            let mut names: Vec<String> = devices.filter_map(|d| d.name().ok()).collect();
            names.sort();
            names.dedup();
            names
        }
        Err(e) => {
            log_audio(&format!("failed to list input devices: {e}"));
            vec![]
        }
    }
}

// ---------------------------------------------------------------------------
// FFT and band computation (ported from Swift AudioRecorder)
// ---------------------------------------------------------------------------

/// Compute frequency band levels using FFT.
/// Returns `RAW_BAND_COUNT` (6) normalized band values.
fn compute_bands(samples: &[f32], sample_rate: f32) -> [f32; RAW_BAND_COUNT] {
    let n = FFT_SIZE.min(samples.len());

    // Apply Hann window
    let mut windowed: Vec<Complex<f32>> = Vec::with_capacity(FFT_SIZE);
    for i in 0..FFT_SIZE {
        let window_val =
            0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos());
        let sample = if i < n { samples[i] } else { 0.0 };
        windowed.push(Complex::new(sample * window_val, 0.0));
    }

    // Forward FFT
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    fft.process(&mut windowed);

    // Compute magnitudes (only first half -- positive frequencies)
    let half = FFT_SIZE / 2;
    let magnitudes: Vec<f32> = windowed[..half]
        .iter()
        .map(|c| c.norm_sqr()) // magnitude squared, matches vDSP_zvmags
        .collect();

    // Logarithmic frequency bands (voice range ~80Hz - 8kHz)
    let nyquist = sample_rate / 2.0;
    let bin_width = nyquist / half as f32;
    let min_freq: f32 = 80.0;
    let max_freq: f32 = f32::min(8000.0, nyquist);
    let log_min = min_freq.log2();
    let log_max = max_freq.log2();

    let mut bands = [0.0f32; RAW_BAND_COUNT];
    for i in 0..RAW_BAND_COUNT {
        let freq_low =
            2.0f32.powf(log_min + (log_max - log_min) * i as f32 / RAW_BAND_COUNT as f32);
        let freq_high =
            2.0f32.powf(log_min + (log_max - log_min) * (i + 1) as f32 / RAW_BAND_COUNT as f32);
        let bin_low = (freq_low / bin_width).max(1.0) as usize;
        let bin_high = (freq_high / bin_width).min((half - 1) as f32) as usize;

        if bin_high >= bin_low {
            let sum: f32 = magnitudes[bin_low..=bin_high].iter().sum();
            bands[i] = sum / (bin_high - bin_low + 1) as f32;
        }
    }

    // Normalize to relative distribution (which bands are active vs others)
    let peak = bands.iter().cloned().fold(0.0f32, f32::max);
    if peak > 0.0 {
        for b in &mut bands {
            *b /= peak;
        }
    }

    // Compute overall RMS to use as a volume gate
    let rms_count = n.min(FFT_SIZE);
    let rms_sum: f32 = samples[..rms_count].iter().map(|s| s * s).sum();
    let rms = (rms_sum / rms_count.max(1) as f32).sqrt();
    // Aggressive scaling -- normal speech should hit 0.6-0.9
    let volume = (rms * 18.0).powf(0.6).min(1.0);

    // Multiply each band by volume -- silence = all zero, speech = distributed
    for b in &mut bands {
        *b *= volume;
    }

    bands
}

/// Mirror 6 raw bands into 11 display bars: center = band 0 (strongest), fanning out.
/// Outer bars blend in more of the neighboring bands so they're not starved.
fn mirror_bands(raw: &[f32; RAW_BAND_COUNT]) -> [f32; 11] {
    [
        raw[5] * 0.5 + raw[4] * 0.3 + raw[3] * 0.2, // bar 0  (leftmost)
        raw[4] * 0.5 + raw[3] * 0.3 + raw[5] * 0.2, // bar 1
        raw[3] * 0.6 + raw[2] * 0.25 + raw[4] * 0.15, // bar 2
        raw[2] * 0.7 + raw[1] * 0.2 + raw[3] * 0.1, // bar 3
        raw[1] * 0.8 + raw[0] * 0.15 + raw[2] * 0.05, // bar 4
        raw[0],                                     // bar 5  (center)
        raw[1] * 0.85 + raw[0] * 0.1 + raw[2] * 0.05, // bar 6
        raw[2] * 0.7 + raw[1] * 0.2 + raw[3] * 0.1, // bar 7
        raw[3] * 0.6 + raw[2] * 0.25 + raw[4] * 0.15, // bar 8
        raw[4] * 0.5 + raw[3] * 0.3 + raw[5] * 0.2, // bar 9
        raw[5] * 0.5 + raw[4] * 0.3 + raw[3] * 0.2, // bar 10 (rightmost)
    ]
}
