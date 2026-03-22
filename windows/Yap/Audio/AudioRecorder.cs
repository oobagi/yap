using System;
using System.IO;
using NAudio.Wave;
using Yap.Core;

namespace Yap.Audio
{
    /// <summary>
    /// Records audio from the default input device using NAudio's WasapiCapture.
    /// Writes 16-bit PCM WAV to a temp file.
    /// Provides real-time RMS level and FFT band-level callbacks.
    /// Mirrors AudioRecorder from the macOS version.
    /// </summary>
    public class AudioRecorder : IDisposable
    {
        private WaveInEvent? _waveIn;
        private WaveFileWriter? _writer;
        private readonly FftProcessor _fftProcessor = new();
        private bool _disposed;

        /// <summary>Temporary file path for the current recording.</summary>
        public string TempFilePath { get; } = Path.Combine(Path.GetTempPath(), "yap_recording.wav");

        /// <summary>Called on the UI thread with the overall RMS level (0.0 - 1.0).</summary>
        public Action<float>? OnLevelUpdate { get; set; }

        /// <summary>Called on the UI thread with 11-bar mirrored band levels (each 0.0 - 1.0).</summary>
        public Action<float[]>? OnBandLevels { get; set; }

        /// <summary>Whether recording is paused (audio data not written, but levels still computed).</summary>
        public bool IsPaused { get; private set; }

        /// <summary>
        /// Start recording from the default microphone input.
        /// Creates a fresh capture device each call.
        /// Queries the device's native sample rate and keeps 16-bit mono.
        /// Falls back to 16kHz if the device query fails.
        /// </summary>
        public void Start()
        {
            // Clean up any previous temp file
            try { if (File.Exists(TempFilePath)) File.Delete(TempFilePath); }
            catch { /* ignore */ }

            IsPaused = false;

            // Use 44100Hz — universally supported by all modern microphones.
            // WaveInEvent defaults to 8kHz (telephony) which many devices don't capture properly.
            const int sampleRate = 44100;

            _waveIn = new WaveInEvent
            {
                WaveFormat = new WaveFormat(sampleRate, 16, 1), // device rate, 16-bit, mono
                BufferMilliseconds = 50
            };

            _writer = new WaveFileWriter(TempFilePath, _waveIn.WaveFormat);

            _waveIn.DataAvailable += OnDataAvailable;
            _waveIn.RecordingStopped += OnRecordingStopped;

            _waveIn.StartRecording();
            Logger.Log($"AudioRecorder: started at {sampleRate}Hz");
        }

        /// <summary>
        /// Stop recording and return the audio file path. Returns null on failure.
        /// </summary>
        public string? Stop()
        {
            StopInternal();
            IsPaused = false;

            if (File.Exists(TempFilePath))
            {
                var info = new FileInfo(TempFilePath);
                if (info.Length > 44) // WAV header is 44 bytes; must have actual audio data
                {
                    Logger.Log($"AudioRecorder: stopped, file size={info.Length}");
                    return TempFilePath;
                }
            }

            Logger.Log("AudioRecorder: stopped, no audio data");
            return null;
        }

        /// <summary>
        /// Cancel recording without returning data.
        /// </summary>
        public void Cancel()
        {
            StopInternal();
            IsPaused = false;

            try { if (File.Exists(TempFilePath)) File.Delete(TempFilePath); }
            catch { /* ignore */ }

            Logger.Log("AudioRecorder: cancelled");
        }

        /// <summary>Pause recording - audio data is not written but levels continue updating.</summary>
        public void Pause()
        {
            IsPaused = true;
            Logger.Log("AudioRecorder: paused");
        }

        /// <summary>Resume recording - audio data is written again.</summary>
        public void Resume()
        {
            IsPaused = false;
            Logger.Log("AudioRecorder: resumed");
        }

        private int _diagCount;

        private void OnDataAvailable(object? sender, WaveInEventArgs e)
        {
            // Write audio data to file (unless paused)
            if (!IsPaused && _writer != null)
            {
                _writer.Write(e.Buffer, 0, e.BytesRecorded);
            }

            // Always compute levels (even when paused, for visual feedback)
            if (e.BytesRecorded == 0) return;

            // Diagnostic: log first few buffers to see raw sample values
            _diagCount++;
            if (_diagCount <= 3)
            {
                short maxSample = 0;
                for (int j = 0; j < Math.Min(e.BytesRecorded / 2, 100); j++)
                {
                    short s = BitConverter.ToInt16(e.Buffer, j * 2);
                    if (Math.Abs(s) > Math.Abs(maxSample)) maxSample = s;
                }
                Logger.Log($"AudioRecorder DIAG: buffer #{_diagCount}, bytes={e.BytesRecorded}, maxSample(first100)={maxSample}, device={_waveIn?.DeviceNumber}");
            }

            // Convert 16-bit PCM bytes to float samples
            int sampleCount = e.BytesRecorded / 2; // 16-bit = 2 bytes per sample
            var samples = new float[sampleCount];
            for (int i = 0; i < sampleCount; i++)
            {
                short sample = BitConverter.ToInt16(e.Buffer, i * 2);
                samples[i] = sample / 32768f;
            }

            // Compute RMS level
            float sum = 0;
            for (int i = 0; i < samples.Length; i++)
            {
                sum += samples[i] * samples[i];
            }
            float rms = MathF.Sqrt(sum / Math.Max(samples.Length, 1));
            float level = Math.Min(rms * 18.0f, 1.0f);

            // Compute FFT band levels
            float sampleRate = _waveIn?.WaveFormat.SampleRate ?? 16000;
            var rawBands = _fftProcessor.ComputeBands(samples, sampleRate);
            var mirrored = _fftProcessor.MirrorBands(rawBands);

            // Dispatch to UI thread
            System.Windows.Application.Current?.Dispatcher.BeginInvoke(() =>
            {
                OnLevelUpdate?.Invoke(level);
                OnBandLevels?.Invoke(mirrored);
            });
        }

        private void OnRecordingStopped(object? sender, StoppedEventArgs e)
        {
            if (e.Exception != null)
            {
                Logger.Log($"AudioRecorder: recording stopped with error: {e.Exception.Message}");
            }
        }

        private void StopInternal()
        {
            try
            {
                _waveIn?.StopRecording();
            }
            catch { /* ignore */ }

            try
            {
                _writer?.Dispose();
                _writer = null;
            }
            catch { /* ignore */ }

            try
            {
                _waveIn?.Dispose();
                _waveIn = null;
            }
            catch { /* ignore */ }
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            StopInternal();
            GC.SuppressFinalize(this);
        }
    }
}
