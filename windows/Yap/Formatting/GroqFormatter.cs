using System;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;
using Yap.Core;
using Yap.Models;
using Yap.Transcription;

namespace Yap.Formatting
{
    /// <summary>
    /// Groq text formatting provider.
    /// Mirrors callGroq() from the macOS TextFormatter.
    /// Uses the OpenAI-compatible API at api.groq.com.
    /// </summary>
    public class GroqFormatter : IFormattingProvider
    {
        private readonly string _apiKey;
        private readonly string _model;
        private readonly string _style;

        public string ProviderName => "groq";

        public GroqFormatter(string apiKey, string? model = null, string style = "formatted")
        {
            _apiKey = apiKey;
            _model = string.IsNullOrEmpty(model) ? "llama-3.3-70b-versatile" : model;
            _style = style;
        }

        public async Task<TranscriptionResult> FormatAsync(string text)
        {
            var trimmed = text.Trim();
            if (trimmed.Length < 3 || string.IsNullOrEmpty(_apiKey))
            {
                return TranscriptionResult.Ok(text);
            }

            Logger.Log($"Formatting with Groq, model={_model}, style={_style}");

            const string url = "https://api.groq.com/openai/v1/chat/completions";
            var prompt = Prompts.GetFormattingPrompt(_style);

            var body = new
            {
                model = _model,
                messages = new object[]
                {
                    new { role = "system", content = prompt },
                    new { role = "user", content = $"<input>{text}</input>" }
                },
                max_tokens = 2048,
                temperature = 0.3
            };

            var jsonBody = JsonSerializer.Serialize(body);
            var request = new HttpRequestMessage(HttpMethod.Post, url)
            {
                Content = new StringContent(jsonBody, Encoding.UTF8, "application/json")
            };
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", _apiKey);

            try
            {
                using var cts = new System.Threading.CancellationTokenSource(TimeSpan.FromSeconds(10));
                var response = await TranscriptionHelpers.HttpClient.SendAsync(request, cts.Token);
                var responseBody = await response.Content.ReadAsStringAsync();

                using var doc = JsonDocument.Parse(responseBody);
                var root = doc.RootElement;

                if (root.TryGetProperty("choices", out var choices) &&
                    choices.GetArrayLength() > 0)
                {
                    var choice = choices[0];
                    if (choice.TryGetProperty("message", out var message) &&
                        message.TryGetProperty("content", out var content))
                    {
                        var responseText = content.GetString() ?? "";
                        return TranscriptionResult.Ok(TranscriptionHelpers.ExtractJsonText(responseText));
                    }
                }

                return TranscriptionResult.Ok(text);
            }
            catch (Exception ex)
            {
                Logger.Log($"Groq format error: {ex.Message}");
                return TranscriptionResult.Fail(ex);
            }
        }
    }
}
