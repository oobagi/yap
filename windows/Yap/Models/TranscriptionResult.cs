using System;

namespace Yap.Models
{
    /// <summary>
    /// Represents the result of a transcription or formatting operation.
    /// </summary>
    public class TranscriptionResult
    {
        public bool Success { get; }
        public string Text { get; }
        public Exception? Error { get; }

        private TranscriptionResult(bool success, string text, Exception? error)
        {
            Success = success;
            Text = text;
            Error = error;
        }

        public static TranscriptionResult Ok(string text) => new(true, text, null);
        public static TranscriptionResult Fail(Exception error) => new(false, string.Empty, error);
        public static TranscriptionResult Fail(string message) => new(false, string.Empty, new TranscriptionException(message));
    }

    /// <summary>
    /// Exception types for transcription/formatting errors.
    /// Mirrors FormatterError from the macOS version.
    /// </summary>
    public class TranscriptionException : Exception
    {
        public TranscriptionErrorKind Kind { get; }

        public TranscriptionException(string message, TranscriptionErrorKind kind = TranscriptionErrorKind.General)
            : base(message)
        {
            Kind = kind;
        }

        public TranscriptionException(string message, Exception innerException, TranscriptionErrorKind kind = TranscriptionErrorKind.General)
            : base(message, innerException)
        {
            Kind = kind;
        }
    }

    public enum TranscriptionErrorKind
    {
        General,
        InvalidEndpoint,
        UnsupportedProvider,
        AudioReadFailed,
        NoResponse,
        ParseFailed,
        ApiError,
        TruncatedResponse,
        Timeout,
        NetworkError
    }

    /// <summary>
    /// Static factory for common transcription errors.
    /// </summary>
    public static class TranscriptionErrors
    {
        public static TranscriptionException InvalidEndpoint()
            => new("Invalid API endpoint URL", TranscriptionErrorKind.InvalidEndpoint);

        public static TranscriptionException UnsupportedProvider()
            => new("Provider does not support this operation", TranscriptionErrorKind.UnsupportedProvider);

        public static TranscriptionException AudioReadFailed()
            => new("Failed to read audio file", TranscriptionErrorKind.AudioReadFailed);

        public static TranscriptionException NoResponse()
            => new("No response from API", TranscriptionErrorKind.NoResponse);

        public static TranscriptionException ParseFailed()
            => new("Failed to parse API response", TranscriptionErrorKind.ParseFailed);

        public static TranscriptionException ApiError(string message)
            => new($"API error: {message}", TranscriptionErrorKind.ApiError);

        public static TranscriptionException TruncatedResponse(string reason)
            => new($"Response truncated (finishReason: {reason})", TranscriptionErrorKind.TruncatedResponse);
    }
}
