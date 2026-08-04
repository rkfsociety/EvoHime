using Microsoft.UI.Xaml;
using EvoHime.Desktop.Services;

namespace EvoHime.Desktop;

public partial class MainWindow : Window
{
    private readonly CoreIpcClient _ipc = new("evohime-core-v1");
    private CancellationTokenSource? _eventCts;
    private string? _activeTaskId;
    private ulong _lastSequence;

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
            _eventCts = new CancellationTokenSource();
            _ = PumpEventsAsync(_eventCts.Token);
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
            _eventCts?.Cancel();
            _eventCts?.Dispose();
            _eventCts = null;
            _activeTaskId = null;
            StartButton.IsEnabled = true;
            StopButton.IsEnabled = false;
            await _ipc.DisposeAsync();
        }
    }

    private async Task PumpEventsAsync(CancellationToken cancellationToken)
    {
        try
        {
            while (!cancellationToken.IsCancellationRequested && _activeTaskId is not null)
            {
                _lastSequence = await _ipc.ReadReplayAsync(
                    _lastSequence,
                    envelope =>
                    {
                        var text = $"[{envelope.SequenceId}] {envelope.EventType}";
                        _ = DispatcherQueue.TryEnqueue(() => EventLog.Text += text + Environment.NewLine);
                        return Task.CompletedTask;
                    },
                    cancellationToken);
                await Task.Delay(300, cancellationToken);
            }
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception error)
        {
            _ = DispatcherQueue.TryEnqueue(() => ConnectionStatus.Text = $"Поток событий остановлен: {error.Message}");
        }
    }
}
