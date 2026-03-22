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
    /// Google Gemini transcription provider.
    /// Supports one-shot transcription+formatting when Gemini is also the formatter.
    /// Mirrors callGemini() from the macOS AudioTranscriber.
    /// </summary>
    public class GeminiTranscriber : ITranscriptionProvider
    {
        private readonly string _apiKey;
        private readonly string _model;
        private readonly double _temperature;

        public string ProviderName => "gemini";
        public bool CanAlsoFormat => true;

        public GeminiTranscriber(string apiKey, string? model = null, double temperature = 0.0)
        {
            _apiKey = apiKey;
            _model = string.IsNullOrEmpty(model) ? "gemini-2.5-flash" : model;
            _temperature = temperature;
        }

        public async Task<TranscriptionResult> TranscribeAsync(string audioFilePath, string? formattingStyle = null)
        {
            byte[] audioData;
            try
            {
                audioData = await File.ReadAllBytesAsync(audioFilePath);
            }
            catch (Exception ex)
            {
                return TranscriptionResult.Fail(new TranscriptionException(
                    "Failed to read audio file", ex, TranscriptionErrorKind.AudioReadFailed));
            }

            var timeout = TranscriptionHelpers.CalculateTimeout(audioData.Length);
            Logger.Log($"Transcribing with Gemini, model={_model}, audio={audioData.Length} bytes, timeout={timeout.TotalSeconds:F0}s");

            return await TranscriptionHelpers.WithRetryAsync(async () =>
            {
                return await CallGeminiAsync(audioData, formattingStyle, timeout);
            }, "Gemini");
        }

        private async Task<TranscriptionResult> CallGeminiAsync(byte[] audioData, string? style, TimeSpan timeout)
        {
            var base64Audio = Convert.ToBase64String(audioData);
            var prompt = style != null ? Prompts.GetAudioPrompt(style) : Prompts.PlainTranscription;
            var url = $"https://generativelanguage.googleapis.com/v1beta/models/{_model}:generateContent?key={_apiKey}";

            var body = new
            {
                contents = new[]
                {
                    new
                    {
                        parts = new object[]
                        {
                            new { inline_data = new { mime_type = "audio/wav", data = base64Audio } },
                            new { text = prompt }
                        }
                    }
                },
                generationConfig = new
                {
                    temperature = _temperature,
                    maxOutputTokens = 2048,
                    responseMimeType = "application/json"
                }
            };

            var jsonBody = JsonSerializer.Serialize(body);
            var request = new HttpRequestMessage(HttpMethod.Post, url)
            {
                Content = new StringContent(jsonBody, Encoding.UTF8, "application/json")
            };

            using var cts = new System.Threading.CancellationTokenSource(timeout);

            try
            {
                var response = await TranscriptionHelpers.HttpClient.SendAsync(request, cts.Token);
                var responseBody = await response.Content.ReadAsStringAsync();
                Logger.Log($"Gemini status: {(int)response.StatusCode}");
                Logger.Log($"Gemini response: {responseBody[..Math.Min(responseBody.Length, 300)]}");

                using var doc = JsonDocument.Parse(responseBody);
                var root = doc.RootElement;

                // Check for API error
                if (root.TryGetProperty("error", out var error) &&
                    error.TryGetProperty("message", out var errorMsg))
                {
                    return TranscriptionResult.Fail(TranscriptionErrors.ApiError(errorMsg.GetString() ?? "Unknown error"));
                }

                // Parse candidates
                if (!root.TryGetProperty("candidates", out var candidates) ||
                    candidates.GetArrayLength() == 0)
                {
                    return TranscriptionResult.Fail(TranscriptionErrors.ParseFailed());
                }

                var candidate = candidates[0];

                // Check finishReason - anything other than STOP means truncated/blocked
                var finishReason = candidate.TryGetProperty("finishReason", out var fr)
                    ? fr.GetString() ?? "UNKNOWN"
                    : "UNKNOWN";

                if (finishReason != "STOP")
                {
                    Logger.Log($"[Warning] Gemini finishReason: {finishReason} (expected STOP)");
                    return TranscriptionResult.Fail(TranscriptionErrors.TruncatedResponse(finishReason));
                }

                // Extract text from response
                if (candidate.TryGetProperty("content", out var content) &&
                    content.TryGetProperty("parts", out var parts) &&
                    parts.GetArrayLength() > 0 &&
                    parts[0].TryGetProperty("text", out var textProp))
                {
                    var text = textProp.GetString() ?? "";
                    return TranscriptionResult.Ok(TranscriptionHelpers.ExtractJsonText(text));
                }

                return TranscriptionResult.Fail(TranscriptionErrors.ParseFailed());
            }
            catch (TaskCanceledException)
            {
                return TranscriptionResult.Fail(new TranscriptionException("Request timed out", TranscriptionErrorKind.Timeout));
            }
            catch (HttpRequestException ex)
            {
                return TranscriptionResult.Fail(new TranscriptionException(ex.Message, ex, TranscriptionErrorKind.NetworkError));
            }
        }
    }
}
