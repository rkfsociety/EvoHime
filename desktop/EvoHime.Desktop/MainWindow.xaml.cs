using Microsoft.UI.Xaml;
using EvoHime.Desktop.Services;

namespace EvoHime.Desktop;

public partial class MainWindow : Window
{
    private readonly CoreIpcClient _ipc = new("evohime-core-v1");
    private string? _activeTaskId;

    public MainWindow()
    {
        InitializeComponent();
    }

    private async void StartButton_Click(object sender, RoutedEventArgs e)
    {
        var prompt = PromptBox.Text.Trim();
        if (prompt.Length == 0)
        {
            ConnectionStatus.Text = "Введите задачу.";
            return;
        }

        try
        {
            await _ipc.ConnectAndHandshakeAsync(CancellationToken.None);
            _activeTaskId = Guid.NewGuid().ToString("N");
            await _ipc.StartTaskAsync(_activeTaskId, prompt, CancellationToken.None);
            ConnectionStatus.Text = $"Задача {_activeTaskId}: выполняется";
            StartButton.IsEnabled = false;
            StopButton.IsEnabled = true;
        }
        catch (Exception error)
        {
            ConnectionStatus.Text = $"Ошибка IPC: {error.Message}";
            await _ipc.DisposeAsync();
        }
    }

    private async void StopButton_Click(object sender, RoutedEventArgs e)
    {
        if (_activeTaskId is null)
        {
            return;
        }

        try
        {
            await _ipc.StopTaskAsync(_activeTaskId, CancellationToken.None);
            ConnectionStatus.Text = "Остановка отправлена";
        }
        catch (Exception error)
        {
            ConnectionStatus.Text = $"Ошибка остановки: {error.Message}";
        }
        finally
        {
            _activeTaskId = null;
            StartButton.IsEnabled = true;
            StopButton.IsEnabled = false;
            await _ipc.DisposeAsync();
        }
    }
}
