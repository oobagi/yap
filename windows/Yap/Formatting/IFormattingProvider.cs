using System.Threading.Tasks;
using Yap.Models;

namespace Yap.Formatting
{
    /// <summary>
    /// Common interface for all text formatting providers.
    /// </summary>
    public interface IFormattingProvider
    {
        /// <summary>The provider identifier (e.g., "gemini", "openai").</summary>
        string ProviderName { get; }

        /// <summary>
        /// Format transcribed text using the configured LLM and style.
        /// </summary>
        /// <param name="text">Raw transcribed text to format.</param>
        /// <returns>Formatted text result.</returns>
        Task<TranscriptionResult> FormatAsync(string text);
    }

    /// <summary>
    /// Available formatting provider types. Mirrors FormattingProvider from the macOS version.
    /// </summary>
    public enum FormattingProviderType
    {
        None,
        Gemini,
        OpenAI,
        Anthropic,
        Groq
    }

    /// <summary>
    /// Default model names for each formatting provider.
    /// </summary>
    public static class FormattingDefaults
    {
        public static string GetDefaultModel(FormattingProviderType provider) => provider switch
        {
            FormattingProviderType.Gemini => "gemini-2.5-flash",
            FormattingProviderType.OpenAI => "gpt-4o-mini",
            FormattingProviderType.Anthropic => "claude-haiku-4-5-20251001",
            FormattingProviderType.Groq => "llama-3.3-70b-versatile",
            _ => ""
        };

        public static string GetLabel(FormattingProviderType provider) => provider switch
        {
            FormattingProviderType.None => "None",
            FormattingProviderType.Gemini => "Google Gemini",
            FormattingProviderType.OpenAI => "OpenAI",
            FormattingProviderType.Anthropic => "Anthropic",
            FormattingProviderType.Groq => "Groq",
            _ => "Unknown"
        };

        public static FormattingProviderType FromString(string name) => name.ToLowerInvariant() switch
        {
            "gemini" => FormattingProviderType.Gemini,
            "openai" => FormattingProviderType.OpenAI,
            "anthropic" => FormattingProviderType.Anthropic,
            "groq" => FormattingProviderType.Groq,
            _ => FormattingProviderType.None
        };
    }

    /// <summary>
    /// Formatting style options. Mirrors FormattingStyle from the macOS version.
    /// </summary>
    public static class FormattingStyles
    {
        public static string GetLabel(string style) => style switch
        {
            "casual" => "Casual",
            "formatted" => "Formatted",
            "professional" => "Professional",
            _ => "Formatted"
        };

        public static string GetDescription(string style) => style switch
        {
            "casual" => "Light cleanup, keeps your voice",
            "formatted" => "Clean formatting, faithful to what you said",
            "professional" => "Polished writing, elevated language",
            _ => "Clean formatting, faithful to what you said"
        };

        public static string ExampleInput =>
            "um so like i was thinking we should probably you know move the meeting to friday because uh thursdays not gonna work for me";

        public static string GetExampleOutput(string style) => style switch
        {
            "casual" =>
                "so like i was thinking we should probably move the meeting to friday because thursdays not gonna work for me",
            "professional" =>
                "I believe we should reschedule the meeting to Friday, as Thursday will not work for my schedule.",
            _ => // formatted
                "So I was thinking we should probably move the meeting to Friday, because Thursday's not going to work for me."
        };
    }
}
