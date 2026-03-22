using System;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows;
using Yap.Core;
using Application = System.Windows.Application;
using Clipboard = System.Windows.Clipboard;

namespace Yap.Input
{
    /// <summary>
    /// Saves clipboard, writes text, simulates Ctrl+V, then restores clipboard.
    /// Mirrors PasteManager from the macOS version.
    ///
    /// Timing: 50ms paste delay, 300ms restore delay (same as macOS).
    /// </summary>
    public class PasteManager
    {
        // Win32 SendInput API
        [DllImport("user32.dll", SetLastError = true)]
        private static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

        [StructLayout(LayoutKind.Sequential)]
        private struct INPUT
        {
            public uint type;
            public InputUnion U;
        }

        [StructLayout(LayoutKind.Explicit)]
        private struct InputUnion
        {
            [FieldOffset(0)] public KEYBDINPUT ki;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct KEYBDINPUT
        {
            public ushort wVk;
            public ushort wScan;
            public uint dwFlags;
            public uint time;
            public IntPtr dwExtraInfo;
        }

        private const uint INPUT_KEYBOARD = 1;
        private const uint KEYEVENTF_KEYUP = 0x0002;
        private const ushort VK_CONTROL = 0x11;
        private const ushort VK_V = 0x56;

        /// <summary>
        /// Put text on the clipboard and simulate Ctrl+V into the focused app.
        /// Restores the previous clipboard contents after a short delay.
        /// </summary>
        public async Task PasteAsync(string text)
        {
            Logger.Log($"PasteManager: pasting {text.Length} chars");

            string? previous = null;
            try
            {
                // Save current clipboard content (Clipboard requires STA thread)
                Application.Current.Dispatcher.Invoke(() =>
                {
                    if (Clipboard.ContainsText())
                    {
                        previous = Clipboard.GetText();
                    }

                    // Set our text on the clipboard
                    Clipboard.SetText(text);
                });
            }
            catch (Exception ex)
            {
                Logger.Log($"PasteManager: clipboard write failed: {ex.Message}");
                return;
            }

            // Small delay to ensure clipboard is ready (50ms, same as macOS)
            await Task.Delay(50);

            // Simulate Ctrl+V
            SimulateCtrlV();

            // Restore previous clipboard after paste completes (300ms, same as macOS)
            await Task.Delay(300);

            try
            {
                Application.Current.Dispatcher.Invoke(() =>
                {
                    Clipboard.Clear();
                    if (previous != null)
                    {
                        Clipboard.SetText(previous);
                    }
                });
            }
            catch (Exception ex)
            {
                Logger.Log($"PasteManager: clipboard restore failed: {ex.Message}");
            }
        }

        /// <summary>
        /// Simulate Ctrl+V keypress using SendInput.
        /// </summary>
        private void SimulateCtrlV()
        {
            var inputs = new INPUT[4];

            // Ctrl down
            inputs[0] = new INPUT
            {
                type = INPUT_KEYBOARD,
                U = new InputUnion
                {
                    ki = new KEYBDINPUT { wVk = VK_CONTROL, dwFlags = 0 }
                }
            };

            // V down
            inputs[1] = new INPUT
            {
                type = INPUT_KEYBOARD,
                U = new InputUnion
                {
                    ki = new KEYBDINPUT { wVk = VK_V, dwFlags = 0 }
                }
            };

            // V up
            inputs[2] = new INPUT
            {
                type = INPUT_KEYBOARD,
                U = new InputUnion
                {
                    ki = new KEYBDINPUT { wVk = VK_V, dwFlags = KEYEVENTF_KEYUP }
                }
            };

            // Ctrl up
            inputs[3] = new INPUT
            {
                type = INPUT_KEYBOARD,
                U = new InputUnion
                {
                    ki = new KEYBDINPUT { wVk = VK_CONTROL, dwFlags = KEYEVENTF_KEYUP }
                }
            };

            uint result = SendInput((uint)inputs.Length, inputs, Marshal.SizeOf<INPUT>());
            if (result != inputs.Length)
            {
                Logger.Log($"PasteManager: SendInput returned {result}, expected {inputs.Length}");
            }
        }
    }
}
