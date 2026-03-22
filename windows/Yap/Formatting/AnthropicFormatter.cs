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
    /// Anthropic Claude text formatting provider.
    /// Mirrors callAnthropic() from the macOS TextFormatter.
    /// Includes assistant prefill with "{" to force JSON structure.
    /// Uses stop_sequences: ["}"] and reconstructs the full JSON.
    /// </summary>
    public class AnthropicFormatter : IFormattingProvider
    {
        private readonly string _apiKey;
        private readonly string _model;
        private readonly string _style;

        public string ProviderName => "anthropic";

        public AnthropicFormatter(string apiKey, string? model = null, string style = "formatted")
        {
            _apiKey = apiKey;
            _model = string.IsNullOrEmpty(model) ? "claude-haiku-4-5-20251001" : model;
            _style = style;
        }

        public async Task<TranscriptionResult> FormatAsync(string text)
        {
            var trimmed = text.Trim();
            if (trimmed.Length < 3 || string.IsNullOrEmpty(_apiKey))
            {
                return TranscriptionResult.Ok(text);
            }

            Logger.Log($"Formatting with Anthropic, model={_model}, style={_style}");

            const string url = "https://api.anthropic.com/v1/messages";
            var prompt = Prompts.GetFormattingPrompt(_style);

            // Anthropic-specific: prefill assistant with "{" and use stop_sequences ["}"]
            var body = new
            {
                model = _model,
                system = prompt,
                messages = new object[]
                {
                    new { role = "user", content = $"<input>{text}</input>" },
                    new { role = "assistant", content = "{" }
                },
                max_tokens = 2048,
                temperature = 0.0,
                stop_sequences = new[] { "}" }
            };

            var jsonBody = JsonSerializer.Serialize(body);
            var request = new HttpRequestMessage(HttpMethod.Post, url)
            {
                Content = new StringContent(jsonBody, Encoding.UTF8, "application/json")
            };
            request.Headers.Add("x-api-key", _apiKey);
            request.Headers.Add("anthropic-version", "2023-06-01");

            try
            {
                using var cts = new System.Threading.CancellationTokenSource(TimeSpan.FromSeconds(15));
                var response = await TranscriptionHelpers.HttpClient.SendAsync(request, cts.Token);
                var responseBody = await response.Content.ReadAsStringAsync();

                using var doc = JsonDocument.Parse(responseBody);
                var root = doc.RootElement;

                if (root.TryGetProperty("content", out var contentArray) &&
                    contentArray.GetArrayLength() > 0)
                {
                    var textBlock = contentArray[0];
                    if (textBlock.TryGetProperty("text", out var textProp))
                    {
                        var responseText = textProp.GetString() ?? "";

                        // Reconstruct the full JSON: prepend "{" (the prefill)
                        var fullJson = "{" + responseText + "}";

                        try
                        {
                            using var innerDoc = JsonDocument.Parse(fullJson);
                            if (innerDoc.RootElement.TryGetProperty("text", out var cleaned) &&
                                !string.IsNullOrEmpty(cleaned.GetString()))
                            {
                                return TranscriptionResult.Ok(cleaned.GetString()!);
                            }
                        }
                        catch
                        {
                            // JSON parse failed, return raw text
                        }

                        return TranscriptionResult.Ok(responseText.Trim());
                    }
                }

                return TranscriptionResult.Ok(text);
            }
            catch (Exception ex)
            {
                Logger.Log($"Anthropic format error: {ex.Message}");
                return TranscriptionResult.Fail(ex);
            }
        }
    }
}
