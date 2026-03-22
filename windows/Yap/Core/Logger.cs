using System;
using System.Diagnostics;
using System.IO;

namespace Yap.Core
{
    /// <summary>
    /// Global logger that writes to %APPDATA%\yap\debug.log and Debug output.
    /// Thread-safe via locking.
    /// </summary>
    public static class Logger
    {
        private static readonly object _lock = new();
        private static readonly string _logDirectory;
        private static readonly string _logFilePath;

        static Logger()
        {
            _logDirectory = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                "yap");
            _logFilePath = Path.Combine(_logDirectory, "debug.log");

            try
            {
                Directory.CreateDirectory(_logDirectory);
            }
            catch
            {
                // Silently fail if we can't create the log directory
            }
        }

        /// <summary>
        /// The directory where Yap stores its configuration and log files.
        /// %APPDATA%\yap\
        /// </summary>
        public static string AppDataDirectory => _logDirectory;

        public static void Log(string message)
        {
            var timestamp = DateTime.UtcNow.ToString("O");
            var line = $"[{timestamp}] {message}";

            Debug.WriteLine(line);

            lock (_lock)
            {
                try
                {
                    File.AppendAllText(_logFilePath, line + Environment.NewLine);
                }
                catch
                {
                    // Silently fail on write errors
                }
            }
        }
    }
}
