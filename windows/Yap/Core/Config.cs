using System;
using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;
using Yap.Models;

namespace Yap.Core
{
    /// <summary>
    /// Manages reading and writing the Yap configuration file at %APPDATA%\yap\config.json.
    /// Mirrors the macOS config structure exactly.
    /// </summary>
    public static class Config
    {
        private static readonly string _configPath;
        private static readonly JsonSerializerOptions _jsonOptions;
        private static YapConfig? _cached;

        static Config()
        {
            _configPath = Path.Combine(Logger.AppDataDirectory, "config.json");
            _jsonOptions = new JsonSerializerOptions
            {
                WriteIndented = true,
                PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
                DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
            };
        }

        public static string ConfigPath => _configPath;

        /// <summary>
        /// Load the configuration from disk. Returns defaults if the file doesn't exist.
        /// </summary>
        public static YapConfig Load()
        {
            try
            {
                if (File.Exists(_configPath))
                {
                    var json = File.ReadAllText(_configPath);
                    var config = JsonSerializer.Deserialize<YapConfig>(json, _jsonOptions);
                    if (config != null)
                    {
                        _cached = config;
                        return config;
                    }
                }
            }
            catch (Exception ex)
            {
                Logger.Log($"Failed to load config: {ex.Message}");
            }

            _cached = new YapConfig();
            return _cached;
        }

        /// <summary>
        /// Save the configuration to disk.
        /// </summary>
        public static void Save(YapConfig config)
        {
            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(_configPath)!);
                var json = JsonSerializer.Serialize(config, _jsonOptions);
                File.WriteAllText(_configPath, json);
                _cached = config;
                Logger.Log("Config saved");
            }
            catch (Exception ex)
            {
                Logger.Log($"Failed to save config: {ex.Message}");
            }
        }

        /// <summary>
        /// Get the cached config (or load from disk if not yet loaded).
        /// </summary>
        public static YapConfig Current => _cached ?? Load();

        /// <summary>
        /// Reload config from disk, discarding cached values.
        /// </summary>
        public static YapConfig Reload()
        {
            _cached = null;
            return Load();
        }
    }
}
