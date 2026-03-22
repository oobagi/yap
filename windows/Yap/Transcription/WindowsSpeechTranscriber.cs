using System;
using System.IO;
using System.Speech.Recognition;
using System.Threading;
using System.Threading.Tasks;
using Yap.Core;
using Yap.Models;

namespace Yap.Transcription
{
    /// <summary>
    /// On-device transcription using Windows Desktop Speech Recognition (System.Speech).
    /// Equivalent to the macOS SFSpeechRecognizer-based Transcriber.
    ///
    /// Used as:
    /// 1. Primary transcriber when no API provider is configured.
    /// 2. Pre-check before expensive API calls (to confirm speech exists in audio).
    ///
    /// Uses SpeechRecognitionEngine with DictationGrammar for file-based recognition
    /// (no UI shown, runs entirely in the background).
    /// </summary>
    public class WindowsSpeechTranscriber : ITranscriptionProvider
    {
        public string ProviderName => "windows_speech";
        public bool CanAlsoFormat => false;

        public Task<TranscriptionResult> TranscribeAsync(string audioFilePath, string? formattingStyle = null)
        {
            return Task.Run(() =>
            {
                try
                {
                    Logger.Log("WindowsSpeech: starting transcription");

                    if (!File.Exists(audioFilePath))
                    {
                        return TranscriptionResult.Fail(TranscriptionErrors.AudioReadFailed());
                    }

                    var locale = Config.Current.SpeechLocale;
                    var culture = string.IsNullOrWhiteSpace(locale)
                        ? System.Globalization.CultureInfo.CurrentCulture
                        : new System.Globalization.CultureInfo(locale);
                    Logger.Log($"WindowsSpeech: using locale '{culture.Name}'");

                    using var recognizer = new SpeechRecognitionEngine(culture);

                    // Load dictation grammar (free-form speech recognition)
                    recognizer.LoadGrammar(new DictationGrammar());

                    // Set input to the audio file
                    recognizer.SetInputToWaveFile(audioFilePath);

                    // Perform synchronous recognition with timeout
                    string? recognizedText = null;
                    Exception? recognitionError = null;
                    var completed = new ManualResetEventSlim(false);

                    recognizer.RecognizeCompleted += (sender, e) =>
                    {
                        if (e.Error != null)
                        {
                            recognitionError = e.Error;
                        }
                        else if (e.Result != null)
                        {
                            recognizedText = e.Result.Text;
                        }
                        completed.Set();
                    };

                    recognizer.RecognizeAsync(RecognizeMode.Single);

                    // Wait up to 30 seconds for recognition
                    if (!completed.Wait(TimeSpan.FromSeconds(30)))
                    {
                        recognizer.RecognizeAsyncCancel();
                        Logger.Log("WindowsSpeech: timed out");
                        return TranscriptionResult.Fail(new TranscriptionException(
                            "Speech recognition timed out",
                            TranscriptionErrorKind.Timeout));
                    }

                    if (recognitionError != null)
                    {
                        Logger.Log($"WindowsSpeech: error: {recognitionError.Message}");
                        return TranscriptionResult.Fail(new TranscriptionException(
                            $"Speech recognition failed: {recognitionError.Message}",
                            recognitionError,
                            TranscriptionErrorKind.General));
                    }

                    if (!string.IsNullOrWhiteSpace(recognizedText))
                    {
                        Logger.Log($"WindowsSpeech: transcribed '{recognizedText}'");
                        return TranscriptionResult.Ok(recognizedText);
                    }

                    Logger.Log("WindowsSpeech: no text recognized");
                    return TranscriptionResult.Ok(""); // Empty = no speech detected
                }
                catch (Exception ex)
                {
                    Logger.Log($"WindowsSpeech: error: {ex.Message}");
                    return TranscriptionResult.Fail(new TranscriptionException(
                        $"Windows Speech Recognition failed: {ex.Message}",
                        ex,
                        TranscriptionErrorKind.General));
                }
            });
        }

        /// <summary>
        /// Quick pre-check: transcribe using Windows Speech to detect if audio contains speech.
        /// Returns true if speech was detected.
        /// </summary>
        public async Task<bool> HasSpeechAsync(string audioFilePath)
        {
            var result = await TranscribeAsync(audioFilePath);
            return result.Success && !string.IsNullOrWhiteSpace(result.Text);
        }
    }
}
