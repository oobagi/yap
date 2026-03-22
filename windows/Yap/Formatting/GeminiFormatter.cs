using System;
using System.Net.Http;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using Yap.Core;
using Yap.Models;
using Yap.Transcription;

namespace Yap.Formatting
{
    /// <summary>
    /// Google Gemini text formatting provider.
    /// Mirrors callGemini() from the macOS TextFormatter.
    /// </summary>
    public class GeminiFormatter : IFormattingProvider
    {
        private readonly string _apiKey;
        private readonly string _model;
        private readonly string _style;

        public string ProviderName => "gemini";

        public GeminiFormatter(string apiKey, string? model = null, string style = "formatted")
        {
            _apiKey = apiKey;
            _model = string.IsNullOrEmpty(model) ? "gemini-2.5-flash" : model;
            _style = style;
        }

        public async Task<TranscriptionResult> FormatAsync(string text)
        {
            var trimmed = text.Trim();
            if (trimmed.Length < 3 || string.IsNullOrEmpty(_apiKey))
            {
                return TranscriptionResult.Ok(text);
            }

            Logger.Log($"Formatting with Gemini, model={_model}, style={_style}");

            var url = $"https://generativelanguage.googleapis.com/v1beta/models/{_model}:generateContent?key={_apiKey}";
            var prompt = Prompts.GetFormattingPrompt(_style);

            var body = new
            {
                contents = new[]
                {
                    new
                    {
                        parts = new[]
                        {
                            new { text = $"{prompt}\n\n<input>{text}</input>" }
                        }
                    }
                },
                generationConfig = new
                {
                    temperature = 0.0,
                    maxOutputTokens = 2048,
                    responseMimeType = "application/json"
                }
            };

            var jsonBody = JsonSerializer.Serialize(body);
            var request = new HttpRequestMessage(HttpMethod.Post, url)
            {
                Content = new StringContent(jsonBody, Encoding.UTF8, "application/json")
            };

            try
            {
                using var cts = new System.Threading.CancellationTokenSource(TimeSpan.FromSeconds(15));
                var response = await TranscriptionHelpers.HttpClient.SendAsync(request, cts.Token);
                var responseBody = await response.Content.ReadAsStringAsync();

                using var doc = JsonDocument.Parse(responseBody);
                var root = doc.RootElement;

                if (!root.TryGetProperty("candidates", out var candidates) ||
                    candidates.GetArrayLength() == 0)
                {
                    return TranscriptionResult.Ok(text); // Fall back to unformatted
                }

                var candidate = candidates[0];

                // Check finishReason
                var finishReason = candidate.TryGetProperty("finishReason", out var fr)
                    ? fr.GetString() ?? "UNKNOWN"
                    : "UNKNOWN";

                if (finishReason != "STOP")
                {
                    Logger.Log($"[Warning] Gemini format finishReason: {finishReason} - falling back to raw text");
                    return TranscriptionResult.Ok(text);
                }

                if (candidate.TryGetProperty("content", out var content) &&
                    content.TryGetProperty("parts", out var parts) &&
                    parts.GetArrayLength() > 0 &&
                    parts[0].TryGetProperty("text", out var textProp))
                {
                    var responseText = textProp.GetString() ?? "";
                    return TranscriptionResult.Ok(TranscriptionHelpers.ExtractJsonText(responseText));
                }

                return TranscriptionResult.Ok(text);
            }
            catch (Exception ex)
            {
                Logger.Log($"Gemini format error: {ex.Message}");
                return TranscriptionResult.Fail(ex);
            }
        }
    }
}
