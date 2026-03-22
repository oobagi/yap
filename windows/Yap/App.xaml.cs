using System;
using System.Threading;
using System.Windows;
using Yap.Core;
using Yap.UI;
using Application = System.Windows.Application;
using MessageBox = System.Windows.MessageBox;

namespace Yap
{
    public partial class App : Application
    {
        private static Mutex? _singleInstanceMutex;
        private TrayIcon? _trayIcon;
        private AppOrchestrator? _orchestrator;

        protected override void OnStartup(StartupEventArgs e)
        {
            base.OnStartup(e);

            // Single-instance enforcement via named Mutex
            const string mutexName = "Global\\YapSingleInstanceMutex";
            _singleInstanceMutex = new Mutex(true, mutexName, out bool createdNew);

            if (!createdNew)
            {
                // Another instance is already running
                MessageBox.Show(
                    "Yap is already running. Check the system tray.",
                    "Yap",
                    MessageBoxButton.OK,
                    MessageBoxImage.Information);
                Shutdown();
                return;
            }

            // Global exception handler — prevent silent crashes
            DispatcherUnhandledException += (_, args) =>
            {
                Logger.Log($"UNHANDLED EXCEPTION: {args.Exception}");
                args.Handled = true;
            };

            Logger.Log("App launched");

            // Initialize the orchestrator (central coordinator)
            _orchestrator = new AppOrchestrator();

            // Initialize system tray icon
            _trayIcon = new TrayIcon(_orchestrator);

            // Wire tray icon to orchestrator so it can update the icon
            _orchestrator.SetTrayIcon(_trayIcon);

            // Start the orchestrator
            _orchestrator.Initialize();

            Logger.Log("Setup complete - ready");
        }

        protected override void OnExit(ExitEventArgs e)
        {
            Logger.Log("App shutting down");

            _orchestrator?.Shutdown();
            _trayIcon?.Dispose();
            _singleInstanceMutex?.ReleaseMutex();
            _singleInstanceMutex?.Dispose();

            base.OnExit(e);
        }
    }
}
