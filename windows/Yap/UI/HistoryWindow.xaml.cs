using System;
using System.Collections.Generic;
using System.Linq;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using Yap.Models;
using Button = System.Windows.Controls.Button;
using Clipboard = System.Windows.Clipboard;
using KeyEventArgs = System.Windows.Input.KeyEventArgs;
using MessageBox = System.Windows.MessageBox;

namespace Yap.UI
{
    /// <summary>
    /// Transcription history window.
    /// Mirrors HistoryWindow from the macOS version.
    /// </summary>
    public partial class HistoryWindow : Window
    {
        public HistoryWindow()
        {
            InitializeComponent();
            Loaded += (_, _) => RefreshList();
        }

        private void RefreshList()
        {
            var entries = HistoryManager.Shared.Entries;

            if (entries.Count == 0)
            {
                HistoryList.Visibility = Visibility.Collapsed;
                EmptyState.Visibility = Visibility.Visible;
                ClearButton.IsEnabled = false;
                EntryCountText.Text = "";
            }
            else
            {
                HistoryList.Visibility = Visibility.Visible;
                EmptyState.Visibility = Visibility.Collapsed;
                ClearButton.IsEnabled = true;
                EntryCountText.Text = $"{entries.Count} transcription{(entries.Count == 1 ? "" : "s")}";

                HistoryList.ItemsSource = entries.Select(e => new HistoryEntryViewModel(e)).ToList();
            }
        }

        private void CopyEntry_Click(object sender, RoutedEventArgs e)
        {
            if (sender is Button btn && btn.Tag is string text)
            {
                CopyToClipboard(text, btn);
            }
        }

        private void EntryBorder_Click(object sender, MouseButtonEventArgs e)
        {
            if (sender is System.Windows.Controls.Border border && border.Tag is string text)
            {
                // Find the Copy button in this item's visual tree
                if (border.Child is System.Windows.Controls.Grid grid)
                {
                    for (int i = 0; i < grid.Children.Count; i++)
                    {
                        if (grid.Children[i] is Button btn)
                        {
                            CopyToClipboard(text, btn);
                            break;
                        }
                    }
                }
            }
        }

        private static void CopyToClipboard(string text, Button feedbackButton)
        {
            try
            {
                Clipboard.SetText(text);
                var original = feedbackButton.Content;
                feedbackButton.Content = "Copied!";
                feedbackButton.Foreground = new System.Windows.Media.SolidColorBrush(
                    System.Windows.Media.Color.FromRgb(0x55, 0xCC, 0x55));

                var timer = new System.Windows.Threading.DispatcherTimer
                {
                    Interval = TimeSpan.FromSeconds(1.5)
                };
                timer.Tick += (_, _) =>
                {
                    timer.Stop();
                    feedbackButton.Content = "Copy";
                    feedbackButton.Foreground = new System.Windows.Media.SolidColorBrush(
                        System.Windows.Media.Color.FromRgb(0xCC, 0xCC, 0xCC));
                };
                timer.Start();
            }
            catch
            {
                // Clipboard may fail if locked by another process
            }
        }

        private void ClearHistory_Click(object sender, RoutedEventArgs e)
        {
            var result = MessageBox.Show(
                "Are you sure you want to clear all transcription history?",
                "Clear History",
                MessageBoxButton.YesNo,
                MessageBoxImage.Question);

            if (result == MessageBoxResult.Yes)
            {
                HistoryManager.Shared.Clear();
                RefreshList();
            }
        }

        private void Window_KeyDown(object sender, KeyEventArgs e)
        {
            if (e.Key == Key.Escape)
            {
                e.Handled = true;
                Close();
            }
        }
    }

    /// <summary>
    /// View model for displaying a history entry in the list.
    /// </summary>
    public class HistoryEntryViewModel
    {
        private readonly HistoryEntry _entry;

        public HistoryEntryViewModel(HistoryEntry entry) { _entry = entry; }

        /// <summary>Full text for copying.</summary>
        public string FullText => _entry.Text;

        /// <summary>Truncated text (first 100 chars) for display.</summary>
        public string TruncatedText
        {
            get
            {
                var text = _entry.Text.Replace("\n", " ").Replace("\r", "");
                if (text.Length <= 100) return text;
                return text[..97] + "...";
            }
        }

        public string RelativeTime
        {
            get
            {
                var interval = DateTime.UtcNow - _entry.Timestamp;
                return interval.TotalSeconds switch
                {
                    < 60 => "just now",
                    < 120 => "1m ago",
                    < 3600 => $"{(int)(interval.TotalMinutes)}m ago",
                    < 7200 => "1h ago",
                    < 86400 => $"{(int)(interval.TotalHours)}h ago",
                    < 172800 => "Yesterday",
                    < 604800 => $"{(int)(interval.TotalDays)}d ago",
                    _ => _entry.Timestamp.ToLocalTime().ToString("MMM d, yyyy")
                };
            }
        }

        public string ProviderLabel
        {
            get
            {
                var txLabel = CapitalizeProvider(_entry.TranscriptionProvider);
                if (!string.IsNullOrEmpty(_entry.FormattingProvider) && _entry.FormattingProvider != "none")
                {
                    var fmtLabel = CapitalizeProvider(_entry.FormattingProvider);
                    return $"{txLabel} + {fmtLabel}";
                }
                return txLabel;
            }
        }

        private static string CapitalizeProvider(string provider)
        {
            return provider.ToLowerInvariant() switch
            {
                "none" or "windows_speech" => "Windows Speech",
                "gemini" => "Gemini",
                "openai" => "OpenAI",
                "deepgram" => "Deepgram",
                "elevenlabs" => "ElevenLabs",
                "anthropic" => "Anthropic",
                "groq" => "Groq",
                _ => provider
            };
        }
    }
}
