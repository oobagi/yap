using System.Text.Json.Serialization;

namespace Yap.Models
{
    /// <summary>
    /// Root configuration model. Stored at %APPDATA%\yap\config.json.
    /// Mirrors the macOS config structure.
    /// </summary>
    public class YapConfig
    {
        // General
        [JsonPropertyName("hotkey")]
        public string Hotkey { get; set; } = "capslock";

        // Audio input device (empty = system default)
        [JsonPropertyName("captureDeviceId")]
        public string CaptureDeviceId { get; set; } = "";

        // Transcription
        [JsonPropertyName("txProvider")]
        public string TxProvider { get; set; } = "none";

        [JsonPropertyName("txApiKey")]
        public string TxApiKey { get; set; } = "";

        [JsonPropertyName("txModel")]
        public string TxModel { get; set; } = "";

        // Formatting
        [JsonPropertyName("fmtProvider")]
        public string FmtProvider { get; set; } = "none";

        [JsonPropertyName("fmtApiKey")]
        public string FmtApiKey { get; set; } = "";

        [JsonPropertyName("fmtModel")]
        public string FmtModel { get; set; } = "";

        [JsonPropertyName("fmtStyle")]
        public string FmtStyle { get; set; } = "formatted";

        // Deepgram options
        [JsonPropertyName("dgSmartFormat")]
        public bool DgSmartFormat { get; set; } = true;

        [JsonPropertyName("dgKeywords")]
        public string DgKeywords { get; set; } = "";

        [JsonPropertyName("dgLanguage")]
        public string DgLanguage { get; set; } = "";

        // OpenAI transcription options
        [JsonPropertyName("oaiLanguage")]
        public string OaiLanguage { get; set; } = "";

        [JsonPropertyName("oaiPrompt")]
        public string OaiPrompt { get; set; } = "";

        // Gemini transcription options
        [JsonPropertyName("geminiTemperature")]
        public double GeminiTemperature { get; set; } = 0.0;

        // ElevenLabs options
        [JsonPropertyName("elLanguageCode")]
        public string ElLanguageCode { get; set; } = "";

        // Speech recognition locale (BCP 47 / culture name, e.g. "en-US")
        // Empty string means use the system's current culture.
        [JsonPropertyName("speechLocale")]
        public string SpeechLocale { get; set; } = "";

        // Appearance options
        [JsonPropertyName("soundsEnabled")]
        public bool SoundsEnabled { get; set; } = true;

        [JsonPropertyName("gradientEnabled")]
        public bool GradientEnabled { get; set; } = true;

        [JsonPropertyName("alwaysVisiblePill")]
        public bool AlwaysVisiblePill { get; set; } = true;

        // History
        [JsonPropertyName("historyEnabled")]
        public bool HistoryEnabled { get; set; } = true;

        // Internal state (not user-facing in config UI but persisted)
        [JsonPropertyName("onboardingComplete")]
        public bool OnboardingComplete { get; set; } = false;

        /// <summary>
        /// Deep clone the config for safe editing.
        /// </summary>
        public YapConfig Clone()
        {
            return new YapConfig
            {
                Hotkey = Hotkey,
                CaptureDeviceId = CaptureDeviceId,
                TxProvider = TxProvider,
                TxApiKey = TxApiKey,
                TxModel = TxModel,
                FmtProvider = FmtProvider,
                FmtApiKey = FmtApiKey,
                FmtModel = FmtModel,
                FmtStyle = FmtStyle,
                DgSmartFormat = DgSmartFormat,
                DgKeywords = DgKeywords,
                DgLanguage = DgLanguage,
                OaiLanguage = OaiLanguage,
                OaiPrompt = OaiPrompt,
                GeminiTemperature = GeminiTemperature,
                ElLanguageCode = ElLanguageCode,
                SpeechLocale = SpeechLocale,
                SoundsEnabled = SoundsEnabled,
                GradientEnabled = GradientEnabled,
                AlwaysVisiblePill = AlwaysVisiblePill,
                HistoryEnabled = HistoryEnabled,
                OnboardingComplete = OnboardingComplete
            };
        }
    }
}
