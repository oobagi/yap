using System.Threading.Tasks;
using Yap.Models;

namespace Yap.Transcription
{
    /// <summary>
    /// Common interface for all transcription providers.
    /// </summary>
    public interface ITranscriptionProvider
    {
        /// <summary>The provider identifier (e.g., "gemini", "openai").</summary>
        string ProviderName { get; }

        /// <summary>Whether this provider can also perform formatting in one shot (e.g., Gemini).</summary>
        bool CanAlsoFormat { get; }

        /// <summary>
        /// Transcribe an audio file.
        /// </summary>
        /// <param name="audioFilePath">Path to the WAV audio file.</param>
        /// <param name="formattingStyle">Optional formatting style for one-shot transcribe+format (Gemini only).</param>
        /// <returns>Transcription result containing the text or an error.</returns>
        Task<TranscriptionResult> TranscribeAsync(string audioFilePath, string? formattingStyle = null);
    }

    /// <summary>
    /// Available transcription provider types. Mirrors TranscriptionProvider from the macOS version.
    /// </summary>
    public enum TranscriptionProviderType
    {
        None,
        Gemini,
        OpenAI,
        Deepgram,
        ElevenLabs
    }

    /// <summary>
    /// Default model names for each transcription provider.
    /// </summary>
    public static class TranscriptionDefaults
    {
        public static string GetDefaultModel(TranscriptionProviderType provider) => provider switch
        {
            TranscriptionProviderType.Gemini => "gemini-2.5-flash",
            TranscriptionProviderType.OpenAI => "gpt-4o-transcribe",
            TranscriptionProviderType.Deepgram => "nova-3",
            TranscriptionProviderType.ElevenLabs => "scribe_v1",
            _ => ""
        };

        public static string GetLabel(TranscriptionProviderType provider) => provider switch
        {
            TranscriptionProviderType.None => "None (Windows Speech)",
            TranscriptionProviderType.Gemini => "Google Gemini",
            TranscriptionProviderType.OpenAI => "OpenAI",
            TranscriptionProviderType.Deepgram => "Deepgram",
            TranscriptionProviderType.ElevenLabs => "ElevenLabs",
            _ => "Unknown"
        };

        public static TranscriptionProviderType FromString(string name) => name.ToLowerInvariant() switch
        {
            "gemini" => TranscriptionProviderType.Gemini,
            "openai" => TranscriptionProviderType.OpenAI,
            "deepgram" => TranscriptionProviderType.Deepgram,
            "elevenlabs" => TranscriptionProviderType.ElevenLabs,
            _ => TranscriptionProviderType.None
        };
    }
}
