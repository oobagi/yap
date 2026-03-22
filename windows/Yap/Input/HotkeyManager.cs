using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Windows.Input;
using Yap.Core;

namespace Yap.Input
{
    /// <summary>
    /// Global keyboard hook using SetWindowsHookEx (WH_KEYBOARD_LL).
    /// Supports configurable hotkey with press/release/double-tap detection.
    /// Mirrors HotkeyManager from the macOS version.
    ///
    /// Default hotkey: Caps Lock.
    /// Also supports: F1-F24, ScrollLock, Pause, Insert, or any virtual key code.
    /// </summary>
    public class HotkeyManager : IDisposable
    {
        // Win32 API
        private const int WH_KEYBOARD_LL = 13;
        private const int WM_KEYDOWN = 0x0100;
        private const int WM_KEYUP = 0x0101;
        private const int WM_SYSKEYDOWN = 0x0104;
        private const int WM_SYSKEYUP = 0x0105;

        // Virtual key codes for modifier checks
        private const int VK_SHIFT = 0x10;
        private const int VK_CONTROL = 0x11;
        private const int VK_MENU = 0x12;    // Alt
        private const int VK_LWIN = 0x5B;
        private const int VK_RWIN = 0x5C;

        [DllImport("user32.dll", SetLastError = true)]
        private static extern IntPtr SetWindowsHookEx(int idHook, LowLevelKeyboardProc lpfn, IntPtr hMod, uint dwThreadId);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool UnhookWindowsHookEx(IntPtr hhk);

        [DllImport("user32.dll")]
        private static extern IntPtr CallNextHookEx(IntPtr hhk, int nCode, IntPtr wParam, IntPtr lParam);

        [DllImport("kernel32.dll")]
        private static extern IntPtr GetModuleHandle(string? lpModuleName);

        [DllImport("user32.dll")]
        private static extern short GetAsyncKeyState(int vKey);

        private delegate IntPtr LowLevelKeyboardProc(int nCode, IntPtr wParam, IntPtr lParam);

        [StructLayout(LayoutKind.Sequential)]
        private struct KBDLLHOOKSTRUCT
        {
            public uint vkCode;
            public uint scanCode;
            public uint flags;
            public uint time;
            public IntPtr dwExtraInfo;
        }

        // State
        private IntPtr _hookHandle = IntPtr.Zero;
        private LowLevelKeyboardProc? _hookProc; // prevent GC
        private int _hotkeyVkCode;
        private bool _disposed;

        /// <summary>Whether the hotkey is currently held down.</summary>
        public bool IsHeld { get; private set; }

        private DateTime? _lastKeyUpTime;
        private const double DoubleTapWindowSeconds = 0.35;

        // Callbacks
        public Action? OnKeyDown { get; set; }
        public Action? OnKeyUp { get; set; }
        public Action? OnDoubleTap { get; set; }

        /// <summary>
        /// Create a hotkey manager for the specified virtual key code.
        /// </summary>
        /// <param name="vkCode">Virtual key code (e.g., 0x14 for Caps Lock, 0x87 for F24)</param>
        public HotkeyManager(int vkCode = 0x14) // VK_CAPITAL = 0x14 (Caps Lock)
        {
            _hotkeyVkCode = vkCode;
        }

        /// <summary>
        /// Update the hotkey to a different virtual key code.
        /// </summary>
        public void SetHotkey(int vkCode)
        {
            _hotkeyVkCode = vkCode;
            Logger.Log($"Hotkey set to VK=0x{vkCode:X2}");
        }

        /// <summary>
        /// Set the hotkey from a config string name.
        /// Supported: "capslock", "f1"-"f24", "scrolllock", "pause", "insert", or any single key name.
        /// </summary>
        public void SetHotkeyFromName(string name)
        {
            int vk = NameToVirtualKey(name);
            SetHotkey(vk);
        }

        /// <summary>
        /// Convert a key name string to a Win32 virtual key code.
        /// Handles explicit names (capslock, f1-f24, etc.) and falls back to
        /// parsing WPF Key enum names for arbitrary keys captured by the settings dialog.
        /// </summary>
        public static int NameToVirtualKey(string name)
        {
            int? explicitVk = name.ToLowerInvariant() switch
            {
                "capslock" => 0x14,
                "f1" => 0x70,
                "f2" => 0x71,
                "f3" => 0x72,
                "f4" => 0x73,
                "f5" => 0x74,
                "f6" => 0x75,
                "f7" => 0x76,
                "f8" => 0x77,
                "f9" => 0x78,
                "f10" => 0x79,
                "f11" => 0x7A,
                "f12" => 0x7B,
                "f13" => 0x7C,
                "f14" => 0x7D,
                "f15" => 0x7E,
                "f16" => 0x7F,
                "f17" => 0x80,
                "f18" => 0x81,
                "f19" => 0x82,
                "f20" => 0x83,
                "f21" => 0x84,
                "f22" => 0x85,
                "f23" => 0x86,
                "f24" => 0x87,
                "scrolllock" => 0x91,
                "pause" => 0x13,
                "insert" => 0x2D,
                _ => null
            };

            if (explicitVk.HasValue) return explicitVk.Value;

            // Try parsing as a WPF Key enum name (e.g. "A", "Space", "OemTilde")
            if (Enum.TryParse<Key>(name, true, out var wpfKey))
            {
                int vk = KeyInterop.VirtualKeyFromKey(wpfKey);
                if (vk != 0) return vk;
            }

            return 0x14; // default to Caps Lock
        }

        /// <summary>
        /// Convert a Win32 virtual key code to a display-friendly key name.
        /// </summary>
        public static string VirtualKeyToName(int vkCode)
        {
            return vkCode switch
            {
                0x14 => "Caps Lock",
                0x70 => "F1",
                0x71 => "F2",
                0x72 => "F3",
                0x73 => "F4",
                0x74 => "F5",
                0x75 => "F6",
                0x76 => "F7",
                0x77 => "F8",
                0x78 => "F9",
                0x79 => "F10",
                0x7A => "F11",
                0x7B => "F12",
                0x7C => "F13",
                0x7D => "F14",
                0x7E => "F15",
                0x7F => "F16",
                0x80 => "F17",
                0x81 => "F18",
                0x82 => "F19",
                0x83 => "F20",
                0x84 => "F21",
                0x85 => "F22",
                0x86 => "F23",
                0x87 => "F24",
                0x91 => "Scroll Lock",
                0x13 => "Pause",
                0x2D => "Insert",
                _ => $"Key 0x{vkCode:X2}"
            };
        }

        /// <summary>
        /// Convert a display-friendly key name to a config string (lowercase, no spaces).
        /// </summary>
        public static string DisplayNameToConfigName(string displayName)
        {
            return displayName.Replace(" ", "").ToLowerInvariant();
        }

        /// <summary>
        /// Install the low-level keyboard hook. Returns false if hook installation fails.
        /// </summary>
        public bool Start()
        {
            if (_hookHandle != IntPtr.Zero) return true; // already started

            _hookProc = HookCallback;

            using var curProcess = Process.GetCurrentProcess();
            using var curModule = curProcess.MainModule!;
            _hookHandle = SetWindowsHookEx(
                WH_KEYBOARD_LL,
                _hookProc,
                GetModuleHandle(curModule.ModuleName),
                0);

            if (_hookHandle == IntPtr.Zero)
            {
                Logger.Log("HotkeyManager: Failed to install keyboard hook");
                return false;
            }

            Logger.Log($"HotkeyManager: started (VK=0x{_hotkeyVkCode:X2})");
            return true;
        }

        /// <summary>
        /// Remove the keyboard hook.
        /// </summary>
        public void Stop()
        {
            if (_hookHandle != IntPtr.Zero)
            {
                UnhookWindowsHookEx(_hookHandle);
                _hookHandle = IntPtr.Zero;
                Logger.Log("HotkeyManager: stopped");
            }
        }

        /// <summary>
        /// Check if any other modifier key (Shift, Ctrl, Alt, Win) is currently held.
        /// GetAsyncKeyState returns negative (high bit set) if the key is currently down.
        /// </summary>
        private static bool IsAnyModifierHeld()
        {
            return IsKeyDown(VK_SHIFT) ||
                   IsKeyDown(VK_CONTROL) ||
                   IsKeyDown(VK_MENU) ||
                   IsKeyDown(VK_LWIN) ||
                   IsKeyDown(VK_RWIN);
        }

        private static bool IsKeyDown(int vk)
        {
            return (GetAsyncKeyState(vk) & 0x8000) != 0;
        }

        private IntPtr HookCallback(int nCode, IntPtr wParam, IntPtr lParam)
        {
            if (nCode >= 0)
            {
                var hookStruct = Marshal.PtrToStructure<KBDLLHOOKSTRUCT>(lParam);
                int msg = wParam.ToInt32();

                if ((int)hookStruct.vkCode == _hotkeyVkCode)
                {
                    // Don't trigger hotkey if other modifiers are held
                    if (IsAnyModifierHeld())
                    {
                        return CallNextHookEx(_hookHandle, nCode, wParam, lParam);
                    }

                    if (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN)
                    {
                        if (!IsHeld)
                        {
                            IsHeld = true;

                            // Check for double-tap
                            if (_lastKeyUpTime.HasValue &&
                                (DateTime.Now - _lastKeyUpTime.Value).TotalSeconds < DoubleTapWindowSeconds)
                            {
                                _lastKeyUpTime = null;
                                System.Windows.Application.Current?.Dispatcher.BeginInvoke(() =>
                                {
                                    OnDoubleTap?.Invoke();
                                });
                            }
                            else
                            {
                                System.Windows.Application.Current?.Dispatcher.BeginInvoke(() =>
                                {
                                    OnKeyDown?.Invoke();
                                });
                            }
                        }

                        // Consume the event to prevent other apps from seeing it
                        return (IntPtr)1;
                    }
                    else if (msg == WM_KEYUP || msg == WM_SYSKEYUP)
                    {
                        if (IsHeld)
                        {
                            IsHeld = false;
                            _lastKeyUpTime = DateTime.Now;

                            System.Windows.Application.Current?.Dispatcher.BeginInvoke(() =>
                            {
                                OnKeyUp?.Invoke();
                            });
                        }

                        // Consume the release too
                        return (IntPtr)1;
                    }
                }
            }

            return CallNextHookEx(_hookHandle, nCode, wParam, lParam);
        }

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            Stop();
            GC.SuppressFinalize(this);
        }
    }
}
