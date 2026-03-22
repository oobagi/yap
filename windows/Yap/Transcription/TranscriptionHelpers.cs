using System;
using System.IO;
using System.Net.Http;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using Yap.Core;
using Yap.Models;

namespace Yap.Transcription
{
    /// <summary>
    /// Shared helpers for transcription/formatting providers.
    /// Includes retry logic, JSON extraction, multipart body building, and prompt strings.
    /// </summary>
    public static class TranscriptionHelpers
    {
        /// <summary>Maximum total attempts (1 original + 2 retries) for transient failures.</summary>
        public const int MaxAttempts = 3;

        /// <summary>Shared HttpClient for all API calls.</summary>
        public static readonly HttpClient HttpClient = new();

        /// <summary>
        /// Execute a request with retry logic (3 total attempts, exponential backoff 0.5s/1.0s).
        /// Mirrors transcribeWithRetry from the macOS version.
        /// </summary>
        public static async Task<TranscriptionResult> WithRetryAsync(
            Func<Task<TranscriptionResult>> action,
            string label)
        {
            for (int attempt = 1; attempt <= MaxAttempts; attempt++)
            {
                var result = await action();

                if (result.Success) return result;

                // Determine if error is retryable
                bool isRetryable = false;
                if (result.Error is TranscriptionException txEx)
                {
                    isRetryable = txEx.Kind is
                        TranscriptionErrorKind.TruncatedResponse or
                        TranscriptionErrorKind.NoResponse or
                        TranscriptionErrorKind.ParseFailed or
                        TranscriptionErrorKind.Timeout or
                        TranscriptionErrorKind.NetworkError;
                }
                else if (result.Error is TaskCanceledException or HttpRequestException)
                {
                    isRetryable = true;
                }

                if (isRetryable && attempt < MaxAttempts)
                {
                    Logger.Log($"[Warning] {label} attempt {attempt} failed ({result.Error?.Message}), retrying ({attempt + 1}/{MaxAttempts})...");
                    await Task.Delay(TimeSpan.FromSeconds(attempt * 0.5));
                    continue;
                }

                return result;
            }

            return TranscriptionResult.Fail("Max retries exceeded");
        }

        /// <summary>
        /// Calculate timeout based on audio data size.
        /// 30s base + 1s per second of audio (estimated at 32KB/s for 16-bit mono 16kHz).
        /// </summary>
        public static TimeSpan CalculateTimeout(long audioBytes)
        {
            double estimatedSeconds = audioBytes / 64000.0;
            double timeoutSeconds = Math.Max(30.0, 30.0 + estimatedSeconds);
            return TimeSpan.FromSeconds(timeoutSeconds);
        }

        /// <summary>
        /// Extract the "text" field from a JSON response, handling markdown code fences.
        /// Mirrors extractJSON() from the macOS version.
        /// </summary>
        public static string ExtractJsonText(string text)
        {
            var s = text.Trim();

            // Strip markdown code fences
            if (s.StartsWith("```json", StringComparison.OrdinalIgnoreCase))
                s = s[7..];
            else if (s.StartsWith("```"))
                s = s[3..];
            if (s.EndsWith("```"))
                s = s[..^3];
            s = s.Trim();

            // Try direct JSON parse
            try
            {
                using var doc = JsonDocument.Parse(s);
                if (doc.RootElement.TryGetProperty("text", out var textProp))
                {
                    return textProp.GetString() ?? s;
                }
            }
            catch { /* not valid JSON, try extracting */ }

            // Try to find JSON object anywhere in the string
            int start = s.IndexOf('{');
            int end = s.LastIndexOf('}');
            if (start >= 0 && end > start)
            {
                var jsonSlice = s[start..(end + 1)];
                try
                {
                    using var doc = JsonDocument.Parse(jsonSlice);
                    if (doc.RootElement.TryGetProperty("text", out var textProp))
                    {
                        return textProp.GetString() ?? s;
                    }
                }
                catch { /* fall through */ }
            }

            return s;
        }

        /// <summary>
        /// Build a multipart/form-data body with audio file and string fields.
        /// </summary>
        public static (byte[] body, string contentType) BuildMultipartBody(
            byte[] audioData,
            Dictionary<string, string> fields)
        {
            var boundary = Guid.NewGuid().ToString();
            using var ms = new MemoryStream();
            var encoding = Encoding.UTF8;

            // Audio file part
            var header = $"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"recording.wav\"\r\nContent-Type: audio/wav\r\n\r\n";
            ms.Write(encoding.GetBytes(header));
            ms.Write(audioData);
            ms.Write(encoding.GetBytes("\r\n"));

            // String fields
            foreach (var (key, value) in fields)
            {
                var fieldHeader = $"--{boundary}\r\nContent-Disposition: form-data; name=\"{key}\"\r\n\r\n{value}\r\n";
                ms.Write(encoding.GetBytes(fieldHeader));
            }

            // Closing boundary
            ms.Write(encoding.GetBytes($"--{boundary}--\r\n"));

            return (ms.ToArray(), $"multipart/form-data; boundary={boundary}");
        }
    }

    /// <summary>
    /// All prompt strings used for transcription and formatting.
    /// Mirrors FormattingStyle prompts from the macOS TextFormatter.swift.
    /// </summary>
    public static class Prompts
    {
        // Shared rules
        private const string NoiseRule =
            "IGNORE all background noise, sound effects, music, and non-speech sounds. " +
            "Only transcribe human speech. If there is no speech, respond with {\"text\":\"\"}.";

        private const string DictationCommands =
            "DICTATION COMMANDS -- when the speaker says any of these, insert the symbol instead of the words: " +
            "\"period\" or \"full stop\" -> . | \"comma\" -> , | \"question mark\" -> ? | \"exclamation mark\" or \"exclamation point\" -> ! " +
            "\"colon\" -> : | \"semicolon\" -> ; | \"open parenthesis\" or \"open paren\" -> ( | \"close parenthesis\" or \"close paren\" -> ) " +
            "\"open bracket\" -> [ | \"close bracket\" -> ] | \"open brace\" or \"open curly\" -> { | \"close brace\" or \"close curly\" -> } " +
            "\"open quote\" or \"open quotes\" -> \" | \"close quote\" or \"close quotes\" or \"end quote\" -> \" " +
            "\"dash\" or \"em dash\" -> -- | \"hyphen\" -> - | \"ellipsis\" or \"dot dot dot\" -> ... " +
            "\"new line\" or \"newline\" -> insert a line break | \"new paragraph\" -> insert two line breaks " +
            "\"ampersand\" -> & | \"at sign\" -> @ | \"hashtag\" or \"hash\" -> # | \"dollar sign\" -> $ | \"percent\" or \"percent sign\" -> % " +
            "\"asterisk\" or \"star\" -> * | \"slash\" or \"forward slash\" -> / | \"backslash\" -> \\ " +
            "\"underscore\" -> _ | \"pipe\" -> | | \"tilde\" -> ~ | \"caret\" -> ^ " +
            "Only convert these when the speaker clearly intends them as punctuation commands, not when used naturally in speech.";

        // Plain transcription prompt (no formatting)
        public const string PlainTranscription =
            "Transcribe this audio exactly as spoken, with proper punctuation and capitalization. " +
            DictationCommands + " " +
            NoiseRule + " " +
            "You MUST respond with ONLY a JSON object: {\"text\":\"transcription here\"}";

        /// <summary>
        /// Get the audio transcription + formatting prompt for one-shot providers (Gemini).
        /// </summary>
        public static string GetAudioPrompt(string style) => style switch
        {
            "casual" =>
                "Transcribe this audio. Remove filler sounds (um, uh, er) but keep everything else exactly as spoken -- " +
                "casual phrases, slang, sentence structure, contractions. All lowercase. Minimal punctuation. " +
                DictationCommands + " " +
                NoiseRule + " " +
                "You MUST respond with ONLY a JSON object: {\"text\":\"transcription here\"}",

            "professional" =>
                "Transcribe this audio. Remove all filler words. Elevate the language to sound polished and professional. " +
                "Fix grammar, improve word choice, use proper punctuation and capitalization. " +
                "Expand contractions. You MAY rephrase for clarity and professionalism, but keep the original meaning. " +
                DictationCommands + " " +
                NoiseRule + " " +
                "You MUST respond with ONLY a JSON object: {\"text\":\"transcription here\"}",

            _ => // "formatted" (default)
                "Transcribe this audio. Remove filler words (um, uh, er, like, you know). " +
                "Fix punctuation and capitalization. Keep the speaker's EXACT words and sentence structure -- " +
                "do not rephrase or rewrite. Keep contractions as spoken. Only fix obvious grammar errors. " +
                DictationCommands + " " +
                NoiseRule + " " +
                "You MUST respond with ONLY a JSON object: {\"text\":\"transcription here\"}"
        };

        /// <summary>
        /// Get the text formatting prompt (for already-transcribed text).
        /// </summary>
        public static string GetFormattingPrompt(string style) => style switch
        {
            "casual" =>
                "You clean up spoken text. You MUST respond with ONLY a JSON object: {\"text\":\"cleaned version here\"} " +
                "Rules: remove ONLY filler sounds (um, uh, er). Keep everything else exactly as spoken -- " +
                "casual phrases, slang, sentence structure, contractions. All lowercase. Minimal punctuation. " +
                "PRESERVE all existing symbols -- parentheses, quotes, brackets, etc. " +
                "Convert spoken punctuation commands to symbols (e.g. \"period\" -> ., \"open parenthesis\" -> (, \"comma\" -> ,). " +
                "NEVER respond conversationally. ONLY output the JSON object.",

            "professional" =>
                "You clean up spoken text. You MUST respond with ONLY a JSON object: {\"text\":\"cleaned version here\"} " +
                "Rules: remove all filler words. Elevate the language to sound polished and professional. " +
                "Fix grammar, improve word choice, use proper punctuation and capitalization. " +
                "Expand contractions. You MAY rephrase for clarity and professionalism, but keep the original meaning. " +
                "PRESERVE all existing symbols -- parentheses, quotes, brackets, etc. " +
                "Convert spoken punctuation commands to symbols (e.g. \"period\" -> ., \"open parenthesis\" -> (, \"comma\" -> ,). " +
                "NEVER respond conversationally. ONLY output the JSON object.",

            _ => // "formatted" (default)
                "You clean up spoken text. You MUST respond with ONLY a JSON object: {\"text\":\"cleaned version here\"} " +
                "Rules: remove filler words (um, uh, er, like, you know). Fix punctuation and capitalization. " +
                "Keep the speaker's EXACT words and sentence structure -- do not rephrase or rewrite. " +
                "Keep contractions as spoken. Only fix obvious grammar errors. " +
                "PRESERVE all existing symbols -- parentheses, quotes, brackets, etc. " +
                "Convert spoken punctuation commands to symbols (e.g. \"period\" -> ., \"open parenthesis\" -> (, \"comma\" -> ,). " +
                "NEVER respond conversationally. ONLY output the JSON object."
        };
    }
}
