using System;
using System.Collections.Generic;
using System.IO;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Text.Json;
using System.Threading.Tasks;
using System.Web;
using Yap.Core;
using Yap.Models;

namespace Yap.Transcription
{
    /// <summary>
    /// Deepgram transcription provider.
    /// Mirrors callDeepgram() from the macOS AudioTranscriber.
    /// </summary>
    public class DeepgramTranscriber : ITranscriptionProvider
    {
        private readonly string _apiKey;
        private readonly string _model;
        private readonly bool _smartFormat;
        private readonly string _language;
        private readonly string[] _keywords;

        public string ProviderName => "deepgram";
        public bool CanAlsoFormat => false;

        public DeepgramTranscriber(
            string apiKey,
            string? model = null,
            bool smartFormat = true,
            string language = "",
            string[]? keywords = null)
        {
            _apiKey = apiKey;
            _model = string.IsNullOrEmpty(model) ? "nova-3" : model;
            _smartFormat = smartFormat;
            _language = language;
            _keywords = keywords ?? Array.Empty<string>();
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
            Logger.Log($"Transcribing with Deepgram, model={_model}, audio={audioData.Length} bytes, timeout={timeout.TotalSeconds:F0}s");

            return await TranscriptionHelpers.WithRetryAsync(async () =>
            {
                return await CallDeepgramAsync(audioData, timeout);
            }, "Deepgram");
        }

        private async Task<TranscriptionResult> CallDeepgramAsync(byte[] audioData, TimeSpan timeout)
        {
            // Build query parameters
            var queryParams = new List<string> { $"model={_model}" };
            if (_smartFormat) queryParams.Add("smart_format=true");
            if (!string.IsNullOrEmpty(_language)) queryParams.Add($"language={_language}");
            foreach (var kw in _keywords)
            {
                if (!string.IsNullOrWhiteSpace(kw))
                {
                    queryParams.Add($"keywords={HttpUtility.UrlEncode(kw.Trim())}");
                }
            }

            var url = $"https://api.deepgram.com/v1/listen?{string.Join("&", queryParams)}";

            var request = new HttpRequestMessage(HttpMethod.Post, url)
            {
                Content = new ByteArrayContent(audioData)
            };
            request.Content.Headers.ContentType = new MediaTypeHeaderValue("audio/wav");
            request.Headers.Add("Authorization", $"Token {_apiKey}");

            using var cts = new System.Threading.CancellationTokenSource(timeout);

            try
            {
                var response = await TranscriptionHelpers.HttpClient.SendAsync(request, cts.Token);
                var responseBody = await response.Content.ReadAsStringAsync();
                Logger.Log($"Deepgram status: {(int)response.StatusCode}");
                Logger.Log($"Deepgram response: {responseBody[..Math.Min(responseBody.Length, 300)]}");

                using var doc = JsonDocument.Parse(responseBody);
                var root = doc.RootElement;

                // Parse nested response: results.channels[0].alternatives[0].transcript
                if (root.TryGetProperty("results", out var results) &&
                    results.TryGetProperty("channels", out var channels) &&
                    channels.GetArrayLength() > 0)
                {
                    var channel = channels[0];
                    if (channel.TryGetProperty("alternatives", out var alts) &&
                        alts.GetArrayLength() > 0)
                    {
                        var alt = alts[0];
                        if (alt.TryGetProperty("transcript", out var transcript))
                        {
                            return TranscriptionResult.Ok(transcript.GetString() ?? "");
                        }
                    }
                }

                // Check for error
                if (root.TryGetProperty("err_msg", out var errMsg))
                {
                    return TranscriptionResult.Fail(TranscriptionErrors.ApiError(errMsg.GetString() ?? "Unknown error"));
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
