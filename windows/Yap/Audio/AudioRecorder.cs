using System;
using System.Collections.Generic;
using System.IO;
using NAudio.CoreAudioApi;
using NAudio.Wave;
using Yap.Core;

namespace Yap.Audio
{
    /// <summary>
    /// Records audio from a capture device using WASAPI.
    /// WASAPI provides IEEE Float32 samples which are converted to 16-bit PCM for WAV output.
    /// </summary>
    public class AudioRecorder : IDisposable
    {
        private WasapiCapture? _capture;
        private WaveFileWriter? _writer;
        private readonly FftProcessor _fftProcessor = new();
        private bool _disposed;
        private int _nativeChannels;
        private int _nativeSampleRate;

        /// <summary>
        /// Returns a list of active capture (microphone) devices.
        /// Each entry is (deviceId, friendlyName).
        /// </summary>
        public static List<(string Id, string Name)> GetCaptureDevices()
        {
            var devices = new List<(string, string)>();
            try
            {
                var enumerator = new MMDeviceEnumerator();
                foreach (var device in enumerator.EnumerateAudioEndPoints(DataFlow.Capture, DeviceState.Active))
                {
                    devices.Add((device.ID, device.FriendlyName));
                }
            }
            catch (Exception ex)
            {
                Logger.Log($"AudioRecorder: failed to enumerate devices: {ex.Message}");
            }
            return devices;
        }

        /// <summary>Temporary file path for the current recording.</summary>
        public string TempFilePath { get; } = Path.Combine(Path.GetTempPath(), "yap_recording.wav");

        /// <summary>Called on the UI thread with the overall RMS level (0.0 - 1.0).</summary>
        public Action<float>? OnLevelUpdate { get; set; }

        /// <summary>Called on the UI thread with 11-bar mirrored band levels (each 0.0 - 1.0).</summary>
        public Action<float[]>? OnBandLevels { get; set; }

        /// <summary>Whether recording is paused (audio data not written, but levels still computed).</summary>
        public bool IsPaused { get; private set; }

        /// <summary>
        /// Start recording from the default microphone using WASAPI.
        /// WASAPI uses the device's native format (IEEE Float32, typically 48kHz).
        /// </summary>
        public void Start()
        {
            // Clean up any previous temp file
            try { if (File.Exists(TempFilePath)) File.Delete(TempFilePath); }
            catch { /* ignore */ }

            IsPaused = false;

            // Get the capture device — use configured device ID, or fall back to system default
            var enumerator = new MMDeviceEnumerator();
            MMDevice device;
            try
            {
                var configDeviceId = Config.Current.CaptureDeviceId;
                if (!string.IsNullOrEmpty(configDeviceId))
                {
                    try
                    {
                        device = enumerator.GetDevice(configDeviceId);
                        Logger.Log($"AudioRecorder: using configured device='{device.FriendlyName}'");
                    }
                    catch
                    {
                        Logger.Log($"AudioRecorder: configured device not found, falling back to default");
                        device = enumerator.GetDefaultAudioEndpoint(DataFlow.Capture, Role.Communications);
                        Logger.Log($"AudioRecorder: device='{device.FriendlyName}'");
                    }
                }
                else
                {
                    device = enumerator.GetDefaultAudioEndpoint(DataFlow.Capture, Role.Communications);
                    Logger.Log($"AudioRecorder: device='{device.FriendlyName}'");
                }
            }
            catch (Exception ex)
            {
                Logger.Log($"AudioRecorder: no capture device found: {ex.Message}");
                return;
            }

            // Create WASAPI capture in shared mode (allows other apps to use mic)
            _capture = new WasapiCapture(device, useEventSync: true)
            {
                ShareMode = AudioClientShareMode.Shared,
            };

            // WASAPI dictates the format — typically IEEE Float32, 48000Hz, stereo
            var nativeFormat = _capture.WaveFormat;
            _nativeChannels = nativeFormat.Channels;
            _nativeSampleRate = nativeFormat.SampleRate;
            Logger.Log($"AudioRecorder: native format={nativeFormat.Encoding}, {nativeFormat.BitsPerSample}-bit, {_nativeSampleRate}Hz, {_nativeChannels}ch");

            // Write as 16-bit PCM mono WAV (what speech-to-text APIs expect)
            var outputFormat = new WaveFormat(_nativeSampleRate, 16, 1);
            _writer = new WaveFileWriter(TempFilePath, outputFormat);

            _capture.DataAvailable += OnDataAvailable;
            _capture.RecordingStopped += OnRecordingStopped;

            _capture.StartRecording();
            Logger.Log($"AudioRecorder: started at {_nativeSampleRate}Hz");
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
                if (info.Length > 44)
                {
                    Logger.Log($"AudioRecorder: stopped, file size={info.Length}");
                    return TempFilePath;
                }
            }

            Logger.Log("AudioRecorder: stopped, no audio data");
            return null;
        }

        /// <summary>Cancel recording without returning data.</summary>
        public void Cancel()
        {
            StopInternal();
            IsPaused = false;

            try { if (File.Exists(TempFilePath)) File.Delete(TempFilePath); }
            catch { /* ignore */ }

            Logger.Log("AudioRecorder: cancelled");
        }

        /// <summary>Pause recording — audio data is not written but levels continue updating.</summary>
        public void Pause()
        {
            IsPaused = true;
            Logger.Log("AudioRecorder: paused");
        }

        /// <summary>Resume recording — audio data is written again.</summary>
        public void Resume()
        {
            IsPaused = false;
            Logger.Log("AudioRecorder: resumed");
        }

        private void OnDataAvailable(object? sender, WaveInEventArgs e)
        {
            if (e.BytesRecorded == 0) return;

            // WASAPI provides IEEE Float32: 4 bytes per sample per channel
            int bytesPerFrame = 4 * _nativeChannels;
            int frameCount = e.BytesRecorded / bytesPerFrame;

            // Convert Float32 multi-channel → mono Float32 samples + 16-bit PCM for file
            var monoSamples = new float[frameCount];
            var pcmBuffer = new byte[frameCount * 2]; // 16-bit mono

            for (int i = 0; i < frameCount; i++)
            {
                // Mix all channels to mono by averaging
                float sum = 0;
                for (int ch = 0; ch < _nativeChannels; ch++)
                {
                    sum += BitConverter.ToSingle(e.Buffer, i * bytesPerFrame + ch * 4);
                }
                float sample = sum / _nativeChannels;
                monoSamples[i] = sample;

                // Convert to 16-bit PCM for WAV file
                sample = Math.Clamp(sample, -1.0f, 1.0f);
                short pcm = (short)(sample * 32767f);
                pcmBuffer[i * 2] = (byte)(pcm & 0xFF);
                pcmBuffer[i * 2 + 1] = (byte)((pcm >> 8) & 0xFF);
            }

            // Write to file (unless paused)
            if (!IsPaused && _writer != null)
            {
                _writer.Write(pcmBuffer, 0, pcmBuffer.Length);
            }

            // Compute RMS level
            float rmsSum = 0;
            for (int i = 0; i < monoSamples.Length; i++)
            {
                rmsSum += monoSamples[i] * monoSamples[i];
            }
            float rms = MathF.Sqrt(rmsSum / Math.Max(monoSamples.Length, 1));
            float level = Math.Min(rms * 18.0f, 1.0f);

            // Compute FFT band levels
            var rawBands = _fftProcessor.ComputeBands(monoSamples, _nativeSampleRate);
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
            try { _capture?.StopRecording(); }
            catch { /* ignore */ }

            try
            {
                _writer?.Dispose();
                _writer = null;
            }
            catch { /* ignore */ }

            try
            {
                _capture?.Dispose();
                _capture = null;
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
