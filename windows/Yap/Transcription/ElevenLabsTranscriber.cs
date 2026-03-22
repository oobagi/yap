using System;
using System.Collections.Generic;
using System.IO;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Text.Json;
using System.Threading.Tasks;
using Yap.Core;
using Yap.Models;

namespace Yap.Transcription
{
    /// <summary>
    /// ElevenLabs transcription provider.
    /// Mirrors callElevenLabs() from the macOS AudioTranscriber.
    /// </summary>
    public class ElevenLabsTranscriber : ITranscriptionProvider
    {
        private readonly string _apiKey;
        private readonly string _model;
        private readonly string _languageCode;

        public string ProviderName => "elevenlabs";
        public bool CanAlsoFormat => false;

        public ElevenLabsTranscriber(string apiKey, string? model = null, string languageCode = "")
        {
            _apiKey = apiKey;
            _model = string.IsNullOrEmpty(model) ? "scribe_v1" : model;
            _languageCode = languageCode;
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
            Logger.Log($"Transcribing with ElevenLabs, model={_model}, audio={audioData.Length} bytes, timeout={timeout.TotalSeconds:F0}s");

            return await TranscriptionHelpers.WithRetryAsync(async () =>
            {
                return await CallElevenLabsAsync(audioData, timeout);
            }, "ElevenLabs");
        }

        private async Task<TranscriptionResult> CallElevenLabsAsync(byte[] audioData, TimeSpan timeout)
        {
            const string url = "https://api.elevenlabs.io/v1/speech-to-text";

            var fields = new Dictionary<string, string> { { "model_id", _model } };
            if (!string.IsNullOrEmpty(_languageCode)) fields["language_code"] = _languageCode;

            var (body, contentType) = TranscriptionHelpers.BuildMultipartBody(audioData, fields);

            var request = new HttpRequestMessage(HttpMethod.Post, url)
            {
                Content = new ByteArrayContent(body)
            };
            request.Content.Headers.ContentType = MediaTypeHeaderValue.Parse(contentType);
            request.Headers.Add("xi-api-key", _apiKey);

            using var cts = new System.Threading.CancellationTokenSource(timeout);

            try
            {
                var response = await TranscriptionHelpers.HttpClient.SendAsync(request, cts.Token);
                var responseBody = await response.Content.ReadAsStringAsync();
                Logger.Log($"ElevenLabs status: {(int)response.StatusCode}");
                Logger.Log($"ElevenLabs response: {responseBody[..Math.Min(responseBody.Length, 300)]}");

                using var doc = JsonDocument.Parse(responseBody);
                var root = doc.RootElement;

                // Check for text field (success)
                if (root.TryGetProperty("text", out var textProp))
                {
                    return TranscriptionResult.Ok(textProp.GetString() ?? "");
                }

                // Check for error
                if (root.TryGetProperty("detail", out var detail) &&
                    detail.TryGetProperty("message", out var errorMsg))
                {
                    return TranscriptionResult.Fail(TranscriptionErrors.ApiError(errorMsg.GetString() ?? "Unknown error"));
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
