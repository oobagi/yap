using System;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Media.Effects;
using System.Windows.Shapes;
using System.Windows.Threading;
using Color = System.Windows.Media.Color;
using MouseEventArgs = System.Windows.Input.MouseEventArgs;
using Rectangle = System.Windows.Shapes.Rectangle;
using System.Runtime.InteropServices;
using Yap.Core;
using Yap.Onboarding;

namespace Yap.UI
{
    /// <summary>
    /// Floating pill overlay window. Transparent, always-on-top, click-through except for the pill.
    /// Positioned at bottom-center of primary screen.
    /// Mirrors OverlayPanel from the macOS version.
    ///
    /// Modes:
    /// - Idle: minimized pill (or hidden)
    /// - Recording: FFT-reactive waveform bars
    /// - Processing: shimmer wave sweep
    /// - HandsFree: expanded pill with pause/stop buttons
    /// - Error: error message auto-dismissing after 2s
    /// </summary>
    public partial class OverlayWindow : Window
    {
        private const int BarCount = 11;

        // Win32 constants for extended window styles
        private const int GWL_EXSTYLE = -20;
        private const int WS_EX_NOACTIVATE = 0x08000000;
        private const int WS_EX_TOOLWINDOW = 0x00000080;
        private const int WS_EX_TRANSPARENT = 0x00000020;

        [DllImport("user32.dll")]
        private static extern int GetWindowLong(IntPtr hwnd, int index);
        [DllImport("user32.dll")]
        private static extern int SetWindowLong(IntPtr hwnd, int index, int newStyle);

        // Waveform bar rectangles
        private readonly Rectangle[] _bars = new Rectangle[BarCount];
        private readonly Rectangle[] _processingBars = new Rectangle[BarCount];

        // State
        private OverlayMode _mode = OverlayMode.Idle;
        private float _audioLevel;
        private float[] _bandLevels = new float[BarCount];
        private bool _isHandsFree;
        private bool _isPaused;
        private bool _alwaysVisible = true;
        private bool _isOnboarding;
        private bool _isPressed;
        private IntPtr _hwnd;

        // Timers
        private DispatcherTimer? _processingTimer;
        private DispatcherTimer? _errorDismissTimer;
        private DispatcherTimer? _elapsedTimer;
        private DateTime _recordingStartTime;
        private TimeSpan _elapsedAccumulated;

        // Gradient (lava lamp)
        private readonly Ellipse[] _gradientBlobs = new Ellipse[4];
        private DateTime _gradientStartTime;
        private bool _gradientEnabled;
        private float _gradientAudioEnergy;

        // Callbacks
        public Action? OnClickToRecord { get; set; }
        public Action? OnPauseResume { get; set; }
        public Action? OnStop { get; set; }

        // Position scaling for center emphasis (same as macOS)
        private static readonly double[] PositionScale =
            { 0.35, 0.45, 0.6, 0.78, 0.92, 1.0, 0.94, 0.8, 0.63, 0.48, 0.38 };

        // Gradient blob colors (match macOS: purple, blue, cyan, indigo)
        private static readonly Color[] BlobBaseColors =
        {
            Color.FromArgb(180, 128, 0, 128),   // purple
            Color.FromArgb(180, 40, 80, 255),    // blue
            Color.FromArgb(180, 0, 200, 220),    // cyan
            Color.FromArgb(180, 75, 0, 130),     // indigo
        };

        public OverlayWindow()
        {
            InitializeComponent();
            CreateBars();
            CreateGradientBlobs();
            PositionOnScreen();
            Loaded += (_, _) => PositionOnScreen();
        }

        protected override void OnSourceInitialized(EventArgs e)
        {
            base.OnSourceInitialized(e);
            _hwnd = new WindowInteropHelper(this).Handle;
            var extStyle = GetWindowLong(_hwnd, GWL_EXSTYLE);
            SetWindowLong(_hwnd, GWL_EXSTYLE, extStyle | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT);
        }

        private void SetClickThrough(bool transparent)
        {
            if (_hwnd == IntPtr.Zero) return;
            var extStyle = GetWindowLong(_hwnd, GWL_EXSTYLE);
            if (transparent)
            {
                SetWindowLong(_hwnd, GWL_EXSTYLE, extStyle | WS_EX_TRANSPARENT);
            }
            else
            {
                SetWindowLong(_hwnd, GWL_EXSTYLE, extStyle & ~WS_EX_TRANSPARENT);
            }
        }

        private void CreateBars()
        {
            WaveformPanel.Children.Clear();
            ProcessingPanel.Children.Clear();

            for (int i = 0; i < BarCount; i++)
            {
                // Recording bars
                var bar = new Rectangle
                {
                    Width = 3,
                    Height = 5,
                    RadiusX = 1.5,
                    RadiusY = 1.5,
                    Fill = new SolidColorBrush(Color.FromArgb(230, 255, 255, 255)),
                    Margin = new Thickness(1, 0, 1, 0),
                    VerticalAlignment = VerticalAlignment.Center
                };
                _bars[i] = bar;
                WaveformPanel.Children.Add(bar);

                // Processing bars
                var pBar = new Rectangle
                {
                    Width = 3,
                    Height = 5,
                    RadiusX = 1.5,
                    RadiusY = 1.5,
                    Fill = new SolidColorBrush(Color.FromArgb(90, 255, 255, 255)),
                    Margin = new Thickness(1, 0, 1, 0),
                    VerticalAlignment = VerticalAlignment.Center
                };
                _processingBars[i] = pBar;
                ProcessingPanel.Children.Add(pBar);
            }
        }

        private void CreateGradientBlobs()
        {
            GradientCanvas.Children.Clear();
            for (int i = 0; i < 4; i++)
            {
                var ellipse = new Ellipse
                {
                    Width = 120,
                    Height = 120,
                    Fill = new RadialGradientBrush(
                        BlobBaseColors[i],
                        Color.FromArgb(0, BlobBaseColors[i].R, BlobBaseColors[i].G, BlobBaseColors[i].B))
                };
                Canvas.SetLeft(ellipse, 140);
                Canvas.SetTop(ellipse, 90);
                _gradientBlobs[i] = ellipse;
                GradientCanvas.Children.Add(ellipse);
            }
        }

        private void PositionOnScreen()
        {
            var screen = SystemParameters.WorkArea;
            Left = screen.Left + (screen.Width - Width) / 2;
            // Position so the pill sits 60px above the taskbar.
            // The pill is at the bottom of this tall overlay window,
            // so the window bottom aligns with the work area bottom.
            // The pill has Margin="0,0,0,30" in XAML — update to 60px offset.
            Top = screen.Top + screen.Height - Height;
        }

        // MARK: - Slide animations

        private void SlideIn()
        {
            var animation = new DoubleAnimation(100, 0, TimeSpan.FromMilliseconds(500))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            };
            SlideTransform.BeginAnimation(TranslateTransform.YProperty, animation);
        }

        private void SlideOut(Action? onComplete = null)
        {
            var animation = new DoubleAnimation(0, 100, TimeSpan.FromMilliseconds(400))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseIn }
            };
            if (onComplete != null)
            {
                animation.Completed += (_, _) => onComplete();
            }
            SlideTransform.BeginAnimation(TranslateTransform.YProperty, animation);
        }

        // MARK: - Gradient (lava lamp) animation

        private void StartGradientAnimation()
        {
            _gradientEnabled = Config.Current.GradientEnabled;
            if (!_gradientEnabled) return;

            _gradientStartTime = DateTime.Now;
            GradientCanvas.Visibility = Visibility.Visible;
            CompositionTarget.Rendering += OnGradientRender;
        }

        private void StopGradientAnimation()
        {
            CompositionTarget.Rendering -= OnGradientRender;
            GradientCanvas.Visibility = Visibility.Collapsed;
        }

        private void OnGradientRender(object? sender, EventArgs e)
        {
            double t = (DateTime.Now - _gradientStartTime).TotalSeconds;
            double energy = Math.Min(_gradientAudioEnergy, 1.0);

            // Lissajous-style drift for each blob
            double[] freqX = { 0.7, 0.5, 0.9, 0.6 };
            double[] freqY = { 0.5, 0.8, 0.6, 0.7 };
            double[] phaseX = { 0, 1.2, 2.4, 3.6 };
            double[] phaseY = { 0.5, 1.8, 3.0, 0.3 };

            double canvasW = GradientCanvas.Width;
            double canvasH = GradientCanvas.Height;

            for (int i = 0; i < 4; i++)
            {
                double driftX = Math.Sin(t * freqX[i] + phaseX[i]) * (80 + energy * 40);
                double driftY = Math.Cos(t * freqY[i] + phaseY[i]) * (50 + energy * 30);

                double cx = canvasW / 2 + driftX - _gradientBlobs[i].Width / 2;
                double cy = canvasH / 2 + driftY - _gradientBlobs[i].Height / 2;

                Canvas.SetLeft(_gradientBlobs[i], cx);
                Canvas.SetTop(_gradientBlobs[i], cy);

                // Scale blobs with audio energy
                double blobScale = 1.0 + energy * 0.6;
                double blobSize = 120 * blobScale;
                _gradientBlobs[i].Width = blobSize;
                _gradientBlobs[i].Height = blobSize;

                // Shift colors with audio energy and time
                double hueShift = Math.Sin(t * 0.3 + i * 1.5) * 0.15;
                var baseColor = BlobBaseColors[i];
                byte r = (byte)Math.Clamp(baseColor.R + (int)(hueShift * 80 + energy * 40), 0, 255);
                byte g = (byte)Math.Clamp(baseColor.G + (int)(hueShift * 60 + energy * 20), 0, 255);
                byte b = (byte)Math.Clamp(baseColor.B + (int)(-hueShift * 40 + energy * 30), 0, 255);
                byte a = (byte)Math.Clamp(120 + (int)(energy * 80), 0, 255);

                _gradientBlobs[i].Fill = new RadialGradientBrush(
                    Color.FromArgb(a, r, g, b),
                    Color.FromArgb(0, r, g, b));
            }
        }

        // MARK: - Public API

        public void ShowRecording()
        {
            _mode = OverlayMode.Recording;
            _audioLevel = 0;
            _isHandsFree = false;
            _isPaused = false;

            WaveformPanel.Visibility = Visibility.Visible;
            ProcessingPanel.Visibility = Visibility.Collapsed;
            HandsFreePanel.Visibility = Visibility.Collapsed;
            ErrorBorder.Visibility = Visibility.Collapsed;
            OnboardingBorder.Visibility = Visibility.Collapsed;
            MicIcon.Visibility = Visibility.Collapsed;

            StopProcessingAnimation();
            StartElapsedTimer();
            StartGradientAnimation();

            // Allow clicks on the pill during recording (for hands-free)
            SetClickThrough(false);

            Show();
            SlideIn();

            AnimatePillScale(1.0);
        }

        public void ShowHandsFreeRecording(Action onPauseResume, Action onStop)
        {
            OnPauseResume = onPauseResume;
            OnStop = onStop;
            _isHandsFree = true;
            _isPaused = false;

            WaveformPanel.Visibility = Visibility.Collapsed;
            HandsFreePanel.Visibility = Visibility.Visible;
            PauseIcon.Text = "\u2016"; // pause icon (double vertical bar ‖)

            // Pill must be clickable for hands-free controls
            SetClickThrough(false);
        }

        public void SetHandsFreePaused(bool paused)
        {
            _isPaused = paused;
            PauseIcon.Text = paused ? "\u25B6" : "\u2016"; // play ▶ : pause ‖

            if (paused)
            {
                PauseElapsedTimer();
            }
            else
            {
                ResumeElapsedTimer();
            }
        }

        public void ShowProcessing()
        {
            _mode = OverlayMode.Processing;
            _isHandsFree = false;
            _isPaused = false;

            WaveformPanel.Visibility = Visibility.Collapsed;
            ProcessingPanel.Visibility = Visibility.Visible;
            HandsFreePanel.Visibility = Visibility.Collapsed;
            ErrorBorder.Visibility = Visibility.Collapsed;
            OnboardingBorder.Visibility = Visibility.Collapsed;
            MicIcon.Visibility = Visibility.Collapsed;
            ElapsedTimer.Visibility = Visibility.Collapsed;

            StopElapsedTimer();
            StartProcessingAnimation();
            StartGradientAnimation();

            // Click-through during processing
            SetClickThrough(true);

            Show();
            SlideIn();

            AnimatePillScale(0.8);
        }

        public void ShowError(string message)
        {
            _mode = OverlayMode.Error;

            WaveformPanel.Visibility = Visibility.Collapsed;
            ProcessingPanel.Visibility = Visibility.Collapsed;
            HandsFreePanel.Visibility = Visibility.Collapsed;
            OnboardingBorder.Visibility = Visibility.Collapsed;
            MicIcon.Visibility = Visibility.Collapsed;
            ElapsedTimer.Visibility = Visibility.Collapsed;

            ErrorText.Text = message;
            ErrorBorder.Visibility = Visibility.Visible;

            StopProcessingAnimation();
            StopElapsedTimer();
            StopGradientAnimation();

            SetClickThrough(true);

            Show();
            SlideIn();

            // Auto-dismiss after 2 seconds
            _errorDismissTimer?.Stop();
            _errorDismissTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(2) };
            _errorDismissTimer.Tick += (_, _) =>
            {
                _errorDismissTimer.Stop();
                Dismiss();
            };
            _errorDismissTimer.Start();

            // Shake the pill
            Shake();
        }

        /// <summary>
        /// Show the noSpeech state: bars at static heights matching PositionScale,
        /// opacity 0.25, with a shake animation.
        /// When autoDismiss is true (default), auto-dismisses after 2 seconds.
        /// When false, the caller (e.g., onboarding manager) controls dismissal.
        /// </summary>
        public void ShowNoSpeech(bool autoDismiss = true)
        {
            _mode = OverlayMode.NoSpeech;
            _isHandsFree = false;
            _isPaused = false;

            WaveformPanel.Visibility = Visibility.Visible;
            ProcessingPanel.Visibility = Visibility.Collapsed;
            HandsFreePanel.Visibility = Visibility.Collapsed;
            ErrorBorder.Visibility = Visibility.Collapsed;
            MicIcon.Visibility = Visibility.Collapsed;
            ElapsedTimer.Visibility = Visibility.Collapsed;

            StopProcessingAnimation();
            StopElapsedTimer();
            StopGradientAnimation();

            // Set bars to static heights based on PositionScale, at 0.25 opacity
            for (int i = 0; i < BarCount; i++)
            {
                double scale = PositionScale[i];
                double minH = 5;
                double maxH = 28;
                _bars[i].Height = minH + (maxH - minH) * scale;
                _bars[i].Fill = new SolidColorBrush(Color.FromArgb(64, 255, 255, 255)); // 0.25 opacity = 64/255
            }

            SetClickThrough(true);

            Show();
            SlideIn();

            // Trigger shake animation
            Shake();

            if (autoDismiss)
            {
                // Auto-dismiss after 2 seconds
                _errorDismissTimer?.Stop();
                _errorDismissTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(2) };
                _errorDismissTimer.Tick += (_, _) =>
                {
                    _errorDismissTimer.Stop();
                    Dismiss();
                };
                _errorDismissTimer.Start();
            }
        }

        public void Dismiss()
        {
            _mode = OverlayMode.Idle;
            _isHandsFree = false;
            _isPaused = false;
            OnPauseResume = null;
            OnStop = null;

            WaveformPanel.Visibility = Visibility.Collapsed;
            ProcessingPanel.Visibility = Visibility.Collapsed;
            HandsFreePanel.Visibility = Visibility.Collapsed;
            ErrorBorder.Visibility = Visibility.Collapsed;
            OnboardingBorder.Visibility = Visibility.Collapsed;
            ElapsedTimer.Visibility = Visibility.Collapsed;

            StopProcessingAnimation();
            StopElapsedTimer();
            StopGradientAnimation();

            if (!_alwaysVisible && !_isOnboarding)
            {
                SlideOut(() =>
                {
                    Hide();
                });
            }
            else
            {
                MicIcon.Visibility = _isOnboarding ? Visibility.Collapsed : Visibility.Visible;
                AnimatePillScale(_isOnboarding ? 1.0 : 0.5);
                // In idle with always-visible, allow clicks on pill to start recording
                SetClickThrough(false);
            }
        }

        public void UpdateLevel(float level)
        {
            _audioLevel = level;
            _gradientAudioEnergy = level;
            UpdateBars();
        }

        public void UpdateBandLevels(float[] levels)
        {
            if (levels.Length == BarCount)
            {
                _bandLevels = levels;
            }
            // Compute average energy for gradient
            if (levels.Length > 0)
            {
                float sum = 0;
                foreach (var b in levels) sum += b;
                _gradientAudioEnergy = sum / levels.Length;
            }
            UpdateBars();
        }

        public void SetAlwaysVisible(bool visible)
        {
            _alwaysVisible = visible;
            if (_mode == OverlayMode.Idle)
            {
                if (visible)
                {
                    MicIcon.Visibility = Visibility.Visible;
                    AnimatePillScale(0.5);
                    SetClickThrough(false);
                    Show();
                    SlideIn();
                }
                else
                {
                    SlideOut(() =>
                    {
                        Hide();
                    });
                }
            }
        }

        public void Shake()
        {
            var storyboard = (Storyboard)FindResource("ShakeAnimation");
            storyboard.Begin();
        }

        /// <summary>Contract hands-free UI without changing overall mode.</summary>
        public void ContractHandsFree()
        {
            _isHandsFree = false;
            _isPaused = false;
            OnPauseResume = null;
            OnStop = null;
            HandsFreePanel.Visibility = Visibility.Collapsed;
            StopElapsedTimer();
        }

        // MARK: - Private helpers

        private void UpdateBars()
        {
            if (_mode != OverlayMode.Recording) return;

            for (int i = 0; i < BarCount; i++)
            {
                double scale = PositionScale[i];
                double minH = 5;
                double maxH = 28;

                // Audio-reactive height
                double bandLevel = i < _bandLevels.Length ? _bandLevels[i] : 0;
                double overall = 0;
                if (_bandLevels.Length > 0)
                {
                    float sum = 0;
                    foreach (var b in _bandLevels) sum += b;
                    overall = sum / _bandLevels.Length;
                }
                double blended = (overall * 0.7 + bandLevel * 0.3);
                double barCeiling = minH + (maxH - minH) * scale;
                double scaled = Math.Min(blended / 0.75, 1.0);
                double driven = Math.Pow(scaled, 0.6);
                double audioH = Math.Max(minH, Math.Min(barCeiling, minH + (barCeiling - minH) * driven));

                _bars[i].Height = _isPaused ? 5 : audioH;
                _bars[i].Fill = new SolidColorBrush(Color.FromArgb(
                    (byte)(_isPaused ? 64 : 230), 255, 255, 255));
            }

            // Bounce the pill with audio level
            if (!_isPaused && _mode == OverlayMode.Recording)
            {
                double level = Math.Min(_audioLevel, 1.0);
                double bounce = 1.0 + Math.Pow(level, 1.5) * 0.25;
                PillScale.ScaleX = bounce;
                PillScale.ScaleY = bounce;
            }
        }

        private void StartProcessingAnimation()
        {
            _processingTimer?.Stop();
            _processingTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(33) }; // ~30fps

            var startTime = DateTime.Now;
            _processingTimer.Tick += (_, _) =>
            {
                double elapsed = (DateTime.Now - startTime).TotalSeconds;
                double margin = 5.0;
                double sweepRange = (BarCount - 1) + margin * 2;
                double t = (elapsed % 1.2) / 1.2;
                double waveCenter = -margin + t * sweepRange;

                for (int i = 0; i < BarCount; i++)
                {
                    double distance = Math.Abs(i - waveCenter);
                    double wave = Math.Exp(-distance * distance / 6.0);

                    double height = 5 + 14 * wave;
                    double opacity = 0.35 + (0.95 - 0.35) * wave;

                    _processingBars[i].Height = height;
                    _processingBars[i].Fill = new SolidColorBrush(
                        Color.FromArgb((byte)(opacity * 255), 255, 255, 255));
                }
            };
            _processingTimer.Start();
        }

        private void StopProcessingAnimation()
        {
            _processingTimer?.Stop();
            _processingTimer = null;
        }

        private void StartElapsedTimer()
        {
            _recordingStartTime = DateTime.Now;
            _elapsedAccumulated = TimeSpan.Zero;
            ElapsedTimer.Visibility = Visibility.Collapsed;

            _elapsedTimer?.Stop();
            _elapsedTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(500) };
            _elapsedTimer.Tick += (_, _) =>
            {
                var total = _elapsedAccumulated + (DateTime.Now - _recordingStartTime);
                if (total.TotalSeconds >= 60)
                {
                    ElapsedTimer.Visibility = Visibility.Visible;
                    int secs = (int)total.TotalSeconds;
                    ElapsedTimer.Text = $"{secs / 60}:{secs % 60:D2}";
                }
            };
            _elapsedTimer.Start();
        }

        private void PauseElapsedTimer()
        {
            _elapsedAccumulated += DateTime.Now - _recordingStartTime;
            _elapsedTimer?.Stop();
        }

        private void ResumeElapsedTimer()
        {
            _recordingStartTime = DateTime.Now;
            _elapsedTimer?.Start();
        }

        private void StopElapsedTimer()
        {
            _elapsedTimer?.Stop();
            _elapsedTimer = null;
            ElapsedTimer.Visibility = Visibility.Collapsed;
        }

        private void AnimatePillScale(double target)
        {
            var scaleX = new DoubleAnimation(target, TimeSpan.FromMilliseconds(300))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            };
            var scaleY = new DoubleAnimation(target, TimeSpan.FromMilliseconds(300))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            };
            PillScale.BeginAnimation(ScaleTransform.ScaleXProperty, scaleX);
            PillScale.BeginAnimation(ScaleTransform.ScaleYProperty, scaleY);
        }

        // MARK: - Onboarding overlay

        /// <summary>
        /// Show onboarding instruction text above the pill.
        /// </summary>
        public void ShowOnboardingStep(OnboardingStep step, string text)
        {
            _isOnboarding = true;
            OnboardingText.Text = text;
            OnboardingBorder.Visibility = Visibility.Visible;

            // Hide error if showing onboarding
            ErrorBorder.Visibility = Visibility.Collapsed;

            // During onboarding idle states, show the pill with waveform bars at rest
            if (_mode == OverlayMode.Idle || _mode == OverlayMode.NoSpeech)
            {
                _mode = OverlayMode.Idle;
                WaveformPanel.Visibility = Visibility.Visible;
                ProcessingPanel.Visibility = Visibility.Collapsed;
                HandsFreePanel.Visibility = Visibility.Collapsed;
                MicIcon.Visibility = Visibility.Collapsed;
                ElapsedTimer.Visibility = Visibility.Collapsed;

                StopProcessingAnimation();
                StopElapsedTimer();

                bool isCelebration = step == OnboardingStep.Nice;

                // Set bars to resting state (brighter during celebration)
                for (int i = 0; i < BarCount; i++)
                {
                    double scale = PositionScale[i];
                    double minH = 5;
                    double maxH = 28;
                    double heightFactor = isCelebration ? 0.6 : 0.3;
                    byte barAlpha = isCelebration ? (byte)200 : (byte)128;
                    _bars[i].Height = minH + (maxH - minH) * scale * heightFactor;
                    _bars[i].Fill = new SolidColorBrush(Color.FromArgb(barAlpha, 255, 255, 255));
                }

                // Start gradient animation during celebration
                if (isCelebration)
                {
                    StartGradientAnimation();
                }
                else
                {
                    StopGradientAnimation();
                }
            }

            // Pill must be visible and clickable during onboarding
            SetClickThrough(false);
            AnimatePillScale(1.0);

            Show();
            SlideIn();
        }

        /// <summary>
        /// Hide onboarding UI and return to normal behavior.
        /// </summary>
        public void HideOnboarding()
        {
            _isOnboarding = false;
            OnboardingBorder.Visibility = Visibility.Collapsed;
            Dismiss();
        }

        /// <summary>
        /// Press-down animation for hold-to-confirm (scale 0.85, opacity 0.7).
        /// </summary>
        public void PressDown()
        {
            _isPressed = true;
            var scaleX = new DoubleAnimation(0.85, TimeSpan.FromMilliseconds(200))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            };
            var scaleY = new DoubleAnimation(0.85, TimeSpan.FromMilliseconds(200))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            };
            PillScale.BeginAnimation(ScaleTransform.ScaleXProperty, scaleX);
            PillScale.BeginAnimation(ScaleTransform.ScaleYProperty, scaleY);
            PillBorder.Opacity = 0.7;
        }

        /// <summary>
        /// Release animation for hold-to-confirm (restore scale and opacity).
        /// </summary>
        public void PressRelease()
        {
            _isPressed = false;
            var scaleX = new DoubleAnimation(1.0, TimeSpan.FromMilliseconds(350))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            };
            var scaleY = new DoubleAnimation(1.0, TimeSpan.FromMilliseconds(350))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            };
            PillScale.BeginAnimation(ScaleTransform.ScaleXProperty, scaleX);
            PillScale.BeginAnimation(ScaleTransform.ScaleYProperty, scaleY);
            PillBorder.Opacity = 1.0;
        }

        // MARK: - Event handlers

        private void Pill_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
        {
            if (_mode == OverlayMode.Recording || _mode == OverlayMode.Processing)
                return;
            OnClickToRecord?.Invoke();
        }

        private void Pill_MouseEnter(object sender, MouseEventArgs e)
        {
            if (_mode != OverlayMode.Idle || _isOnboarding) return;
            // Scale up from 0.5 to 0.65 on hover
            AnimatePillScale(0.65);
        }

        private void Pill_MouseLeave(object sender, MouseEventArgs e)
        {
            if (_mode != OverlayMode.Idle || _isOnboarding) return;
            // Scale back down to 0.5
            AnimatePillScale(0.5);
        }

        private void PauseButton_Click(object sender, MouseButtonEventArgs e)
        {
            e.Handled = true;
            OnPauseResume?.Invoke();
        }

        private void StopButton_Click(object sender, MouseButtonEventArgs e)
        {
            e.Handled = true;
            OnStop?.Invoke();
        }
    }

    public enum OverlayMode
    {
        Idle,
        Recording,
        Processing,
        Error,
        NoSpeech
    }
}
