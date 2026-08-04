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
        _supervisor = new SupervisorProcessService();
        _supervisor.Start();
        _window = new MainWindow();
        _window.UpdateReadyToInstall += () => _window.Close();
        _window.Closed += (_, _) =>
        {
            _tray?.Dispose();
            _supervisor?.Dispose();
        };
        _window.Activate();
        _tray = new TrayIconService(
            show: () => _window.Activate(),
            exit: () => _window.Close());
        _window.NotificationRequested += (title, message) => _tray?.ShowNotification(title, message);
    }
}
