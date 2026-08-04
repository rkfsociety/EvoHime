using Microsoft.UI.Xaml;
using EvoHime.Desktop.Services;

namespace EvoHime.Desktop;

public partial class App : Application
{
    private Window? _window;
    private TrayIconService? _tray;

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        _window = new MainWindow();
        _window.Closed += (_, _) => _tray?.Dispose();
        _window.Activate();
        _tray = new TrayIconService(
            show: () => _window.Activate(),
            exit: () => _window.Close());
    }
}
