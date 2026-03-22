using System;
using System.Collections.Generic;
using System.IO;
using System.Media;
using Yap.Core;

namespace Yap.Audio
{
    /// <summary>
    /// Manages sound effect playback for UI feedback.
    /// Preloads sounds at startup for zero-latency playback.
    /// Mirrors preloadSounds()/playSound() from the macOS AppDelegate.
    /// </summary>
    public class SoundPlayer : IDisposable
    {
        private readonly Dictionary<string, System.Media.SoundPlayer> _players = new();
        private bool _disposed;

        /// <summary>
        /// Preload all sound effect files from the Resources/Sounds directory.
        /// </summary>
        public void PreloadSounds()
        {
            var soundsDir = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "Resources", "Sounds");

            // Map macOS sound names to Windows WAV files
            string[] soundNames = { "Pop", "Blow", "Submarine" };

            foreach (var name in soundNames)
            {
                var path = Path.Combine(soundsDir, $"{name}.wav");
                if (File.Exists(path))
                {
                    try
                    {
                        var player = new System.Media.SoundPlayer(path);
                        player.Load();
                        _players[name] = player;
                    }
                    catch (Exception ex)
                    {
                        Logger.Log($"Failed to preload sound '{name}': {ex.Message}");
                    }
                }
                else
                {
                    Logger.Log($"Sound file not found: {path}");
                }
            }
        }

        /// <summary>
        /// Play a named sound effect. Respects the SoundsEnabled config setting.
        /// </summary>
        public void Play(string name)
        {
            if (!Config.Current.SoundsEnabled) return;

            if (_players.TryGetValue(name, out var player))
            {
                try
                {
                    player.Play(); // Async playback
                }
                catch (Exception ex)
                {
                    Logger.Log($"Failed to play sound '{name}': {ex.Message}");
                }
            }
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;

            foreach (var player in _players.Values)
            {
                player.Dispose();
            }
            _players.Clear();
            GC.SuppressFinalize(this);
        }
    }
}
