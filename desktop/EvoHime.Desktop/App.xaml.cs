using Microsoft.UI.Xaml;
using EvoHime.Desktop.Services;

namespace EvoHime.Desktop;

public partial class App : Application
{
    private MainWindow? _window;
    private TrayIconService? _tray;
    private SupervisorProcessService? _supervisor;

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        StartupDiagnostics.Write("OnLaunched: begin");
        try
        {
            _supervisor = new SupervisorProcessService();
            StartupDiagnostics.Write($"Supervisor start: {_supervisor.Start()}");
            StartupDiagnostics.Write("Creating MainWindow");
            _window = new MainWindow();
            StartupDiagnostics.Write("MainWindow created");
            _window.UpdateReadyToInstall += () => _window.Close();
            _window.Closed += (_, _) =>
            {
                _tray?.Dispose();
                _supervisor?.Dispose();
            };
            _window.Activate();
            StartupDiagnostics.Write("MainWindow activated");
            _tray = new TrayIconService(
                show: () => _window.Activate(),
                exit: () => _window.Close());
            StartupDiagnostics.Write("Tray created");
            _window.NotificationRequested += (title, message) => _tray?.ShowNotification(title, message);
        }
        catch (Exception error)
        {
            StartupDiagnostics.Write($"Startup failed: {error.GetType().FullName}, HResult=0x{error.HResult:X8}, Message={error.Message}, Inner={error.InnerException}");
            throw;
        }
    }
}

internal static class StartupDiagnostics
{
    private static readonly object Sync = new();

    public static void Write(string message)
    {
        try
        {
            var directory = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "EvoHime",
                "logs");
            Directory.CreateDirectory(directory);
            lock (Sync)
            {
                File.AppendAllText(
                    Path.Combine(directory, "desktop-startup.log"),
                    $"{DateTimeOffset.Now:O} {message}{Environment.NewLine}");
            }
        }
        catch
        {
            // Startup diagnostics must never prevent the client from launching.
        }
    }
}
