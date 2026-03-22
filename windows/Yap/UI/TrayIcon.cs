using System;
using System.Drawing;
using System.Linq;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Forms;
using Yap.Core;
using Yap.Models;
using Application = System.Windows.Application;

namespace Yap.UI
{
    /// <summary>
    /// System tray (NotifyIcon) with context menu.
    /// Mirrors the macOS status item menu:
    /// - Yap (title, disabled)
    /// - Enabled (toggle)
    /// - History (submenu with recent entries + Show All + Clear)
    /// - Settings...
    /// - Quit
    /// </summary>
    public class TrayIcon : IDisposable
    {
        [DllImport("user32.dll", CharSet = CharSet.Auto)]
        private static extern bool DestroyIcon(IntPtr handle);

        private readonly NotifyIcon _notifyIcon;
        private readonly AppOrchestrator _orchestrator;
        private ToolStripMenuItem? _enabledItem;
        private ToolStripMenuItem? _historyItem;
        private bool _disposed;

        public TrayIcon(AppOrchestrator orchestrator)
        {
            _orchestrator = orchestrator;

            _notifyIcon = new NotifyIcon
            {
                Text = "Yap",
                Visible = true
            };

            // Use a simple generated icon (mic icon)
            _notifyIcon.Icon = CreateDefaultIcon();

            BuildContextMenu();

            _notifyIcon.DoubleClick += (_, _) => _orchestrator.OpenSettings();
        }

        private void BuildContextMenu()
        {
            var menu = new ContextMenuStrip();

            // Title
            var titleItem = new ToolStripMenuItem("Yap") { Enabled = false };
            titleItem.Font = new Font(titleItem.Font, System.Drawing.FontStyle.Bold);
            menu.Items.Add(titleItem);

            menu.Items.Add(new ToolStripSeparator());

            // Enabled toggle
            _enabledItem = new ToolStripMenuItem("Enabled")
            {
                Checked = true,
                CheckOnClick = true,
                ShortcutKeyDisplayString = "Ctrl+E"
            };
            _enabledItem.CheckedChanged += (_, _) =>
            {
                _orchestrator.IsEnabled = _enabledItem.Checked;
            };
            menu.Items.Add(_enabledItem);

            // History submenu
            _historyItem = new ToolStripMenuItem("History");
            menu.Items.Add(_historyItem);

            // Settings
            var settingsItem = new ToolStripMenuItem("Settings...")
            {
                ShortcutKeyDisplayString = "Ctrl+,"
            };
            settingsItem.Click += (_, _) => _orchestrator.OpenSettings();
            menu.Items.Add(settingsItem);

            menu.Items.Add(new ToolStripSeparator());

            // Quit
            var quitItem = new ToolStripMenuItem("Quit")
            {
                ShortcutKeyDisplayString = "Ctrl+Q"
            };
            quitItem.Click += (_, _) =>
            {
                _orchestrator.Shutdown();
                Application.Current.Shutdown();
            };
            menu.Items.Add(quitItem);

            // Rebuild history submenu when menu opens
            menu.Opening += (_, _) => RebuildHistoryMenu();

            _notifyIcon.ContextMenuStrip = menu;
        }

        private void RebuildHistoryMenu()
        {
            if (_historyItem == null) return;

            _historyItem.DropDownItems.Clear();

            var entries = HistoryManager.Shared.Entries;

            if (entries.Count == 0)
            {
                var empty = new ToolStripMenuItem("No History") { Enabled = false };
                _historyItem.DropDownItems.Add(empty);
            }
            else
            {
                var shown = entries.Take(10);
                foreach (var entry in shown)
                {
                    var truncated = entry.Text.Length <= 60
                        ? entry.Text
                        : entry.Text[..59] + "\u2026";

                    var item = new ToolStripMenuItem(truncated);
                    var textToCopy = entry.Text;
                    item.Click += (_, _) =>
                    {
                        System.Windows.Clipboard.SetText(textToCopy);
                    };
                    _historyItem.DropDownItems.Add(item);
                }

                _historyItem.DropDownItems.Add(new ToolStripSeparator());

                var showAll = new ToolStripMenuItem("Show All...");
                showAll.Click += (_, _) => _orchestrator.OpenHistory();
                _historyItem.DropDownItems.Add(showAll);
            }

            _historyItem.DropDownItems.Add(new ToolStripSeparator());

            var clear = new ToolStripMenuItem("Clear History");
            clear.Enabled = entries.Count > 0;
            clear.Click += (_, _) => HistoryManager.Shared.Clear();
            _historyItem.DropDownItems.Add(clear);
        }

        /// <summary>
        /// Update the tray icon to reflect the current app state.
        /// </summary>
        public void UpdateIcon(AppState state)
        {
            _notifyIcon.Icon = state switch
            {
                AppState.Recording or AppState.HandsFreeRecording or AppState.HandsFreePaused
                    => CreateRecordingIcon(),
                AppState.Processing
                    => CreateProcessingIcon(),
                _ => CreateDefaultIcon()
            };
        }

        // Generate icons programmatically at high resolution (32x32) and let the
        // system scale them down, producing cleaner results at any DPI.
        // GetHicon() creates an unmanaged GDI handle that must be freed with DestroyIcon.

        private static Icon CreateDefaultIcon()
        {
            // Microphone shape rendered at 32x32 for crisp appearance
            using var bmp = new Bitmap(32, 32);
            using var g = Graphics.FromImage(bmp);
            g.SmoothingMode = System.Drawing.Drawing2D.SmoothingMode.HighQuality;
            g.InterpolationMode = System.Drawing.Drawing2D.InterpolationMode.HighQualityBicubic;
            g.Clear(Color.Transparent);

            using var whitePen = new Pen(Color.White, 2.0f);
            using var whiteBrush = new SolidBrush(Color.White);

            // Mic head (rounded capsule top)
            g.FillEllipse(whiteBrush, 11, 2, 10, 10);    // top dome
            g.FillRectangle(whiteBrush, 11, 7, 10, 7);    // body
            g.FillEllipse(whiteBrush, 11, 11, 10, 6);     // bottom round

            // Mic arc (curved cradle beneath)
            g.DrawArc(whitePen, 7, 6, 18, 18, 0, 180);

            // Mic stand (vertical line + base)
            g.FillRectangle(whiteBrush, 15, 24, 2, 4);    // stem
            g.FillRectangle(whiteBrush, 11, 27, 10, 2);   // base

            return CreateIconFromBitmap(bmp);
        }

        private static Icon CreateRecordingIcon()
        {
            using var bmp = new Bitmap(32, 32);
            using var g = Graphics.FromImage(bmp);
            g.SmoothingMode = System.Drawing.Drawing2D.SmoothingMode.HighQuality;
            g.Clear(Color.Transparent);

            // Red filled circle, centered
            using var redBrush = new SolidBrush(Color.FromArgb(230, 40, 40));
            g.FillEllipse(redBrush, 5, 5, 22, 22);

            return CreateIconFromBitmap(bmp);
        }

        private static Icon CreateProcessingIcon()
        {
            using var bmp = new Bitmap(32, 32);
            using var g = Graphics.FromImage(bmp);
            g.SmoothingMode = System.Drawing.Drawing2D.SmoothingMode.HighQuality;
            g.Clear(Color.Transparent);

            // Three dots (ellipsis), vertically centered, evenly spaced
            using var whiteBrush = new SolidBrush(Color.White);
            g.FillEllipse(whiteBrush, 4, 12, 6, 6);
            g.FillEllipse(whiteBrush, 13, 12, 6, 6);
            g.FillEllipse(whiteBrush, 22, 12, 6, 6);

            return CreateIconFromBitmap(bmp);
        }

        private static Icon CreateIconFromBitmap(Bitmap bmp)
        {
            IntPtr hIcon = bmp.GetHicon();
            var icon = Icon.FromHandle(hIcon);
            var clonedIcon = (Icon)icon.Clone();
            DestroyIcon(hIcon);
            return clonedIcon;
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            _notifyIcon.Visible = false;
            _notifyIcon.Dispose();
            GC.SuppressFinalize(this);
        }
    }
}
