using System;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using Microsoft.Win32;
using Yap.Audio;
using Yap.Core;
using Yap.Formatting;
using Yap.Input;
using Yap.Models;
using Yap.Transcription;
using ComboBox = System.Windows.Controls.ComboBox;
using KeyEventArgs = System.Windows.Input.KeyEventArgs;
using MessageBox = System.Windows.MessageBox;

namespace Yap.UI
{
    /// <summary>
    /// Settings window code-behind.
    /// Loads current config on open, saves to config.json on Save.
    /// Mirrors SettingsView from the macOS version.
    /// </summary>
    public partial class SettingsWindow : Window
    {
        /// <summary>Fired when settings are saved so AppOrchestrator can reload.</summary>
        public event Action? SettingsChanged;

        // Hotkey capture state
        private bool _isCapturingHotkey;
        private string _capturedHotkeyConfigName = "capslock";
        private string _capturedHotkeyDisplayName = "Caps Lock";

        // Registry key for startup
        private const string StartupRegistryKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
        private const string StartupValueName = "Yap";

        public SettingsWindow()
        {
            InitializeComponent();
            LoadFromConfig();
        }

        private void LoadFromConfig()
        {
            var config = Config.Reload();

            // Hotkey — convert config name to display name
            _capturedHotkeyConfigName = config.Hotkey;
            int vk = HotkeyManager.NameToVirtualKey(config.Hotkey);
            _capturedHotkeyDisplayName = HotkeyManager.VirtualKeyToName(vk);
            HotkeyButton.Content = _capturedHotkeyDisplayName;
            HotkeyCurrentLabel.Text = $"Current hotkey: {_capturedHotkeyDisplayName}";

            // Microphone device
            LoadMicDevices(config.CaptureDeviceId);

            // Transcription
            SelectComboByTag(TxProviderCombo, config.TxProvider);
            TxApiKeyBox.Text = config.TxApiKey;
            TxModelBox.Text = config.TxModel;

            // Formatting
            SelectComboByTag(FmtProviderCombo, config.FmtProvider);
            FmtApiKeyBox.Text = config.FmtApiKey;
            FmtModelBox.Text = config.FmtModel;
            FmtUseSameKeyCheck.IsChecked =
                string.IsNullOrEmpty(config.FmtApiKey) || config.FmtApiKey == config.TxApiKey;

            // Style
            switch (config.FmtStyle)
            {
                case "casual": StyleCasual.IsChecked = true; break;
                case "professional": StyleProfessional.IsChecked = true; break;
                default: StyleFormatted.IsChecked = true; break;
            }

            // Deepgram options
            DgSmartFormatCheck.IsChecked = config.DgSmartFormat;
            DgLanguageBox.Text = config.DgLanguage;
            DgKeywordsBox.Text = config.DgKeywords;

            // OpenAI options
            OaiLanguageBox.Text = config.OaiLanguage;
            OaiPromptBox.Text = config.OaiPrompt;

            // Gemini options
            GeminiTempSlider.Value = config.GeminiTemperature;
            GeminiTempLabel.Text = config.GeminiTemperature.ToString("F1");

            // ElevenLabs options
            ElLanguageBox.Text = config.ElLanguageCode;

            // Appearance
            SoundsEnabledCheck.IsChecked = config.SoundsEnabled;
            GradientEnabledCheck.IsChecked = config.GradientEnabled;
            AlwaysVisibleCheck.IsChecked = config.AlwaysVisiblePill;

            // Start with Windows
            StartWithWindowsCheck.IsChecked = IsStartupEnabled();

            // History
            HistoryEnabledCheck.IsChecked = config.HistoryEnabled;

            // Advanced
            SpeechLocaleBox.Text = config.SpeechLocale;

            UpdateTxProviderVisibility();
            UpdateFmtProviderVisibility();
            UpdateStylePreview();
        }

        private void Save_Click(object sender, RoutedEventArgs e)
        {
            SaveAndClose();
        }

        private void SaveAndClose()
        {
            // If we're capturing a hotkey, cancel capture first
            if (_isCapturingHotkey)
            {
                CancelHotkeyCapture();
            }

            var config = Config.Current.Clone();

            config.Hotkey = _capturedHotkeyConfigName;
            config.CaptureDeviceId = (MicDeviceCombo.SelectedItem as ComboBoxItem)?.Tag as string ?? "";
            config.TxProvider = GetComboTag(TxProviderCombo) ?? "none";
            config.TxApiKey = TxApiKeyBox.Text.Trim();
            config.TxModel = TxModelBox.Text.Trim();

            config.FmtProvider = GetComboTag(FmtProviderCombo) ?? "none";
            config.FmtApiKey = (FmtUseSameKeyCheck.IsChecked == true) ? "" : FmtApiKeyBox.Text.Trim();
            config.FmtModel = FmtModelBox.Text.Trim();
            config.FmtStyle = GetSelectedStyle();

            config.DgSmartFormat = DgSmartFormatCheck.IsChecked == true;
            config.DgLanguage = DgLanguageBox.Text.Trim();
            config.DgKeywords = DgKeywordsBox.Text.Trim();

            config.OaiLanguage = OaiLanguageBox.Text.Trim();
            config.OaiPrompt = OaiPromptBox.Text.Trim();

            config.GeminiTemperature = Math.Clamp(GeminiTempSlider.Value, 0.0, 2.0);

            config.ElLanguageCode = ElLanguageBox.Text.Trim();

            config.SoundsEnabled = SoundsEnabledCheck.IsChecked == true;
            config.GradientEnabled = GradientEnabledCheck.IsChecked == true;
            config.AlwaysVisiblePill = AlwaysVisibleCheck.IsChecked == true;

            config.HistoryEnabled = HistoryEnabledCheck.IsChecked == true;

            config.SpeechLocale = SpeechLocaleBox.Text.Trim();

            // Start with Windows
            SetStartupEnabled(StartWithWindowsCheck.IsChecked == true);

            Config.Save(config);
            SettingsChanged?.Invoke();
            Close();
        }

        private void Cancel_Click(object sender, RoutedEventArgs e)
        {
            Close();
        }

        // --- Fix 3: Enter/Escape keyboard shortcuts ---

        private void Window_KeyDown(object sender, KeyEventArgs e)
        {
            // If hotkey capture is active, route keys there instead
            if (_isCapturingHotkey)
            {
                HandleHotkeyCapture(e);
                return;
            }

            if (e.Key == Key.Escape)
            {
                e.Handled = true;
                Close();
            }
            else if (e.Key == Key.Enter || e.Key == Key.Return)
            {
                // Don't save if focus is in a multiline textbox (future-proofing)
                e.Handled = true;
                SaveAndClose();
            }
        }

        // --- Fix 1: Hotkey capture ---

        private void HotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            if (_isCapturingHotkey)
            {
                CancelHotkeyCapture();
                return;
            }

            _isCapturingHotkey = true;
            HotkeyButton.Content = "Press a key...";
            HotkeyButton.FontStyle = FontStyles.Italic;
            HotkeyHint.Text = "Press Escape to cancel";

            // Focus the button so it receives key events
            HotkeyButton.Focus();
        }

        private void HandleHotkeyCapture(KeyEventArgs e)
        {
            e.Handled = true;

            // Escape cancels capture
            if (e.Key == Key.Escape)
            {
                CancelHotkeyCapture();
                return;
            }

            // Get the actual key (resolve system keys like F10)
            Key key = e.Key == Key.System ? e.SystemKey : e.Key;

            // Ignore bare modifier keys (Shift, Ctrl, Alt, Win)
            if (key == Key.LeftShift || key == Key.RightShift ||
                key == Key.LeftCtrl || key == Key.RightCtrl ||
                key == Key.LeftAlt || key == Key.RightAlt ||
                key == Key.LWin || key == Key.RWin)
            {
                return;
            }

            // Convert WPF Key to virtual key code
            int vkCode = KeyInterop.VirtualKeyFromKey(key);
            if (vkCode == 0) return;

            // Get display name and config name
            string displayName = HotkeyManager.VirtualKeyToName(vkCode);
            string configName = HotkeyManager.DisplayNameToConfigName(displayName);

            // If VirtualKeyToName returned a generic "Key 0xNN" name, try using the WPF key name
            if (displayName.StartsWith("Key 0x"))
            {
                displayName = key.ToString();
                configName = displayName.ToLowerInvariant();
            }

            _capturedHotkeyDisplayName = displayName;
            _capturedHotkeyConfigName = configName;

            _isCapturingHotkey = false;
            HotkeyButton.Content = _capturedHotkeyDisplayName;
            HotkeyButton.FontStyle = FontStyles.Normal;
            HotkeyHint.Text = "Click to change";
            HotkeyCurrentLabel.Text = $"Current hotkey: {_capturedHotkeyDisplayName}";
        }

        private void CancelHotkeyCapture()
        {
            _isCapturingHotkey = false;
            HotkeyButton.Content = _capturedHotkeyDisplayName;
            HotkeyButton.FontStyle = FontStyles.Normal;
            HotkeyHint.Text = "Click to change";
        }

        // --- Fix 4: Start with Windows ---

        private static bool IsStartupEnabled()
        {
            try
            {
                using var key = Registry.CurrentUser.OpenSubKey(StartupRegistryKey, false);
                return key?.GetValue(StartupValueName) != null;
            }
            catch
            {
                return false;
            }
        }

        private static void SetStartupEnabled(bool enabled)
        {
            try
            {
                using var key = Registry.CurrentUser.OpenSubKey(StartupRegistryKey, true);
                if (key == null) return;

                if (enabled)
                {
                    string exePath = System.Diagnostics.Process.GetCurrentProcess().MainModule?.FileName ?? "";
                    if (!string.IsNullOrEmpty(exePath))
                    {
                        key.SetValue(StartupValueName, $"\"{exePath}\"");
                    }
                }
                else
                {
                    key.DeleteValue(StartupValueName, false);
                }
            }
            catch (Exception ex)
            {
                Logger.Log($"Failed to update startup registry: {ex.Message}");
            }
        }

        // --- Provider visibility handlers ---

        private void TxProviderCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            UpdateTxProviderVisibility();
        }

        private void FmtProviderCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            UpdateFmtProviderVisibility();
        }

        private void FmtUseSameKeyCheck_Changed(object sender, RoutedEventArgs e)
        {
            if (FmtApiKeyBox != null && FmtUseSameKeyCheck != null)
            {
                FmtApiKeyBox.IsEnabled = FmtUseSameKeyCheck.IsChecked != true;
            }
            ValidateFmtApiKey();
        }

        private void TxApiKeyBox_TextChanged(object sender, TextChangedEventArgs e)
        {
            ValidateTxApiKey();
        }

        private void FmtApiKeyBox_TextChanged(object sender, TextChangedEventArgs e)
        {
            ValidateFmtApiKey();
        }

        private void ValidateTxApiKey()
        {
            if (TxApiKeyWarning == null) return;
            var provider = GetComboTag(TxProviderCombo);
            bool needsKey = provider != "none" && provider != null;
            bool isEmpty = string.IsNullOrWhiteSpace(TxApiKeyBox?.Text);
            TxApiKeyWarning.Visibility = (needsKey && isEmpty) ? Visibility.Visible : Visibility.Collapsed;
        }

        private void ValidateFmtApiKey()
        {
            if (FmtApiKeyWarning == null) return;
            var provider = GetComboTag(FmtProviderCombo);
            bool needsKey = provider != "none" && provider != null;
            bool usingSameKey = FmtUseSameKeyCheck?.IsChecked == true;
            bool isEmpty = string.IsNullOrWhiteSpace(FmtApiKeyBox?.Text);
            FmtApiKeyWarning.Visibility = (needsKey && !usingSameKey && isEmpty) ? Visibility.Visible : Visibility.Collapsed;
        }

        private void GeminiTempSlider_ValueChanged(object sender, RoutedPropertyChangedEventArgs<double> e)
        {
            if (GeminiTempLabel != null)
            {
                GeminiTempLabel.Text = Math.Clamp(e.NewValue, 0.0, 2.0).ToString("F1");
            }
        }

        private void StyleRadio_Changed(object sender, RoutedEventArgs e)
        {
            UpdateStylePreview();
        }

        private void ResetOnboarding_Click(object sender, RoutedEventArgs e)
        {
            var config = Config.Current.Clone();
            config.OnboardingComplete = false;
            Config.Save(config);
            MessageBox.Show("Onboarding has been reset. It will start again next time you use Yap.",
                "Reset Onboarding", MessageBoxButton.OK, MessageBoxImage.Information);
        }

        private void LoadMicDevices(string selectedDeviceId)
        {
            MicDeviceCombo.Items.Clear();

            // Add "System Default" option
            var defaultItem = new ComboBoxItem { Content = "System Default", Tag = "" };
            MicDeviceCombo.Items.Add(defaultItem);
            MicDeviceCombo.SelectedItem = defaultItem;

            // Enumerate all active capture devices
            var devices = AudioRecorder.GetCaptureDevices();
            foreach (var (id, name) in devices)
            {
                var item = new ComboBoxItem { Content = name, Tag = id };
                MicDeviceCombo.Items.Add(item);

                if (id == selectedDeviceId)
                {
                    MicDeviceCombo.SelectedItem = item;
                }
            }
        }

        private void UpdateTxProviderVisibility()
        {
            var provider = GetComboTag(TxProviderCombo);
            bool hasProvider = provider != "none" && provider != null;

            if (TxOptionsPanel != null) TxOptionsPanel.Visibility = hasProvider ? Visibility.Visible : Visibility.Collapsed;

            // Provider-specific options
            if (DgOptionsPanel != null) DgOptionsPanel.Visibility = provider == "deepgram" ? Visibility.Visible : Visibility.Collapsed;
            if (OaiOptionsPanel != null) OaiOptionsPanel.Visibility = provider == "openai" ? Visibility.Visible : Visibility.Collapsed;
            if (GeminiOptionsPanel != null) GeminiOptionsPanel.Visibility = provider == "gemini" ? Visibility.Visible : Visibility.Collapsed;
            if (ElOptionsPanel != null) ElOptionsPanel.Visibility = provider == "elevenlabs" ? Visibility.Visible : Visibility.Collapsed;

            // Model hint and placeholder
            if (TxModelHint != null)
            {
                var txType = TranscriptionDefaults.FromString(provider ?? "none");
                var defaultModel = TranscriptionDefaults.GetDefaultModel(txType);
                TxModelHint.Text = $"Leave empty for default ({defaultModel})";
                if (TxModelPlaceholder != null)
                    TxModelPlaceholder.Text = defaultModel;
            }

            ValidateTxApiKey();
        }

        private void UpdateFmtProviderVisibility()
        {
            var provider = GetComboTag(FmtProviderCombo);
            bool hasProvider = provider != "none" && provider != null;

            if (FmtOptionsPanel != null) FmtOptionsPanel.Visibility = hasProvider ? Visibility.Visible : Visibility.Collapsed;

            // Check if same-key toggle should be visible
            if (FmtUseSameKeyCheck != null)
            {
                var txProvider = GetComboTag(TxProviderCombo);
                bool canShare = (provider == txProvider) && hasProvider && txProvider != "none";
                FmtUseSameKeyCheck.Visibility = canShare ? Visibility.Visible : Visibility.Collapsed;
            }

            // Model hint and placeholder
            if (FmtModelHint != null)
            {
                var fmtType = FormattingDefaults.FromString(provider ?? "none");
                var defaultModel = FormattingDefaults.GetDefaultModel(fmtType);
                FmtModelHint.Text = $"Leave empty for default ({defaultModel})";
                if (FmtModelPlaceholder != null)
                    FmtModelPlaceholder.Text = defaultModel;
            }

            ValidateFmtApiKey();
        }

        private void UpdateStylePreview()
        {
            var style = GetSelectedStyle();
            if (StyleLabel != null)
            {
                StyleLabel.Text = FormattingStyles.GetLabel(style);
                StyleDescription.Text = FormattingStyles.GetDescription(style);
                StyleBefore.Text = FormattingStyles.ExampleInput;
                StyleAfter.Text = FormattingStyles.GetExampleOutput(style);
            }
        }

        private string GetSelectedStyle()
        {
            if (StyleCasual?.IsChecked == true) return "casual";
            if (StyleProfessional?.IsChecked == true) return "professional";
            return "formatted";
        }

        // Helpers
        private static void SelectComboByTag(ComboBox combo, string tag)
        {
            for (int i = 0; i < combo.Items.Count; i++)
            {
                if (combo.Items[i] is ComboBoxItem item && item.Tag?.ToString() == tag)
                {
                    combo.SelectedIndex = i;
                    return;
                }
            }
            if (combo.Items.Count > 0)
                combo.SelectedIndex = 0;
        }

        private static string? GetComboTag(ComboBox combo)
        {
            return (combo.SelectedItem as ComboBoxItem)?.Tag?.ToString();
        }
    }
}
