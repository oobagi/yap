using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;
using Yap.Core;

namespace Yap.Models
{
    /// <summary>
    /// A single transcription history entry.
    /// </summary>
    public class HistoryEntry
    {
        [JsonPropertyName("id")]
        public string Id { get; set; } = Guid.NewGuid().ToString();

        [JsonPropertyName("timestamp")]
        public DateTime Timestamp { get; set; } = DateTime.UtcNow;

        [JsonPropertyName("text")]
        public string Text { get; set; } = "";

        [JsonPropertyName("transcription_provider")]
        public string TranscriptionProvider { get; set; } = "";

        [JsonPropertyName("formatting_provider")]
        public string? FormattingProvider { get; set; }

        [JsonPropertyName("formatting_style")]
        public string? FormattingStyle { get; set; }
    }

    /// <summary>
    /// Manages transcription history. Persists to %APPDATA%\yap\history.json.
    /// Mirrors HistoryManager from the macOS version.
    /// </summary>
    public class HistoryManager
    {
        public static readonly HistoryManager Shared = new();

        private readonly string _historyPath;
        private readonly object _lock = new();
        private List<HistoryEntry> _entries = new();

        public IReadOnlyList<HistoryEntry> Entries
        {
            get
            {
                lock (_lock) { return _entries.AsReadOnly(); }
            }
        }

        private HistoryManager()
        {
            _historyPath = Path.Combine(Logger.AppDataDirectory, "history.json");
            Load();
        }

        public void Append(string text, string txProvider, string? fmtProvider, string? fmtStyle)
        {
            var config = Config.Current;
            if (!config.HistoryEnabled) return;

            lock (_lock)
            {
                var entry = new HistoryEntry
                {
                    Id = Guid.NewGuid().ToString(),
                    Timestamp = DateTime.UtcNow,
                    Text = text,
                    TranscriptionProvider = txProvider,
                    FormattingProvider = fmtProvider,
                    FormattingStyle = fmtStyle
                };

                _entries.Insert(0, entry);
                if (_entries.Count > 10)
                {
                    _entries = new List<HistoryEntry>(_entries.GetRange(0, 10));
                }

                Save();
            }
        }

        public void Clear()
        {
            lock (_lock)
            {
                _entries.Clear();
                Save();
            }
        }

        private void Load()
        {
            try
            {
                if (File.Exists(_historyPath))
                {
                    var json = File.ReadAllText(_historyPath);
                    var entries = JsonSerializer.Deserialize<List<HistoryEntry>>(json);
                    if (entries != null)
                    {
                        _entries = entries;
                    }
                }
            }
            catch (Exception ex)
            {
                Logger.Log($"Failed to load history: {ex.Message}");
            }
        }

        private void Save()
        {
            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(_historyPath)!);
                var json = JsonSerializer.Serialize(_entries, new JsonSerializerOptions { WriteIndented = true });
                File.WriteAllText(_historyPath, json);
            }
            catch (Exception ex)
            {
                Logger.Log($"Failed to save history: {ex.Message}");
            }
        }
    }
}
