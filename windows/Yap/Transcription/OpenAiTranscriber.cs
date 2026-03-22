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
    /// OpenAI Whisper/GPT-4o transcription provider.
    /// Mirrors callOpenAITranscribe() from the macOS AudioTranscriber.
    /// </summary>
    public class OpenAiTranscriber : ITranscriptionProvider
    {
        private readonly string _apiKey;
        private readonly string _model;
        private readonly string _language;
        private readonly string _prompt;

        public string ProviderName => "openai";
        public bool CanAlsoFormat => false;

        public OpenAiTranscriber(string apiKey, string? model = null, string language = "", string prompt = "")
        {
            _apiKey = apiKey;
            _model = string.IsNullOrEmpty(model) ? "gpt-4o-transcribe" : model;
            _language = language;
            _prompt = prompt;
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
            Logger.Log($"Transcribing with OpenAI, model={_model}, audio={audioData.Length} bytes, timeout={timeout.TotalSeconds:F0}s");

            return await TranscriptionHelpers.WithRetryAsync(async () =>
            {
                return await CallOpenAIAsync(audioData, timeout);
            }, "OpenAI");
        }

        private async Task<TranscriptionResult> CallOpenAIAsync(byte[] audioData, TimeSpan timeout)
        {
            const string url = "https://api.openai.com/v1/audio/transcriptions";

            var fields = new Dictionary<string, string> { { "model", _model } };
            if (!string.IsNullOrEmpty(_language)) fields["language"] = _language;
            if (!string.IsNullOrEmpty(_prompt)) fields["prompt"] = _prompt;

            var (body, contentType) = TranscriptionHelpers.BuildMultipartBody(audioData, fields);

            var request = new HttpRequestMessage(HttpMethod.Post, url)
            {
                Content = new ByteArrayContent(body)
            };
            request.Content.Headers.ContentType = MediaTypeHeaderValue.Parse(contentType);
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", _apiKey);

            using var cts = new System.Threading.CancellationTokenSource(timeout);

            try
            {
                var response = await TranscriptionHelpers.HttpClient.SendAsync(request, cts.Token);
                var responseBody = await response.Content.ReadAsStringAsync();
                Logger.Log($"OpenAI status: {(int)response.StatusCode}");
                Logger.Log($"OpenAI response: {responseBody[..Math.Min(responseBody.Length, 300)]}");

                using var doc = JsonDocument.Parse(responseBody);
                var root = doc.RootElement;

                // Check for text field (success)
                if (root.TryGetProperty("text", out var textProp))
                {
                    return TranscriptionResult.Ok(textProp.GetString() ?? "");
                }

                // Check for error
                if (root.TryGetProperty("error", out var error) &&
                    error.TryGetProperty("message", out var errorMsg))
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
