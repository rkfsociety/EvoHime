using Microsoft.UI.Xaml;
using EvoHime.Desktop.Services;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace EvoHime.Desktop;

public partial class MainWindow : Window
{
    private readonly CoreIpcClient _ipc = new("evohime-core-v1");
    private readonly NativeShellState _state = new();
    private readonly WorkspaceSettings _settings = new();
    private CancellationTokenSource? _eventCts;
    private string? _activeTaskId;
    private int _reconnectAttempt;

    public MainWindow()
    {
        InitializeComponent();
        _state.SelectWorkspace(Environment.CurrentDirectory);
        WorkspacePathText.Text = $"Workspace: {_state.WorkspacePath}";
        _ = RestoreWorkspaceAsync();
    }

    private async void ChooseWorkspaceButton_Click(object sender, RoutedEventArgs e)
    {
        var picker = new FolderPicker();
        picker.FileTypeFilter.Add("*");
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var folder = await picker.PickSingleFolderAsync();
        if (folder is null)
        {
            return;
        }

        _state.SelectWorkspace(folder.Path);
        await _settings.SaveWorkspaceAsync(folder.Path);
        WorkspacePathText.Text = $"Workspace: {_state.WorkspacePath}";
        ConnectionStatus.Text = "Workspace выбран.";
    }

    private async Task RestoreWorkspaceAsync()
    {
        var savedPath = await _settings.LoadWorkspaceAsync();
        if (savedPath is null || !Directory.Exists(savedPath))
        {
            return;
        }

        _state.SelectWorkspace(savedPath);
        WorkspacePathText.Text = $"Workspace: {_state.WorkspacePath}";
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
            _reconnectAttempt = 0;
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
        while (!cancellationToken.IsCancellationRequested && _activeTaskId is not null)
        {
            try
            {
                if (!_ipc.IsConnected)
                {
                    SetConnectionStatus("Восстановление IPC...");
                    await _ipc.ConnectAndHandshakeAsync(cancellationToken);
                    _reconnectAttempt = 0;
                }

                await _ipc.ReadReplayAsync(
                    _state.LastSequence,
                    envelope =>
                    {
                        if (_state.ApplyEvent(envelope))
                        {
                            var text = $"[{envelope.SequenceId}] {envelope.EventType}";
                            _ = DispatcherQueue.TryEnqueue(() => EventLog.Text += text + Environment.NewLine);
                        }
                        return Task.CompletedTask;
                    },
                    cancellationToken);
                SetConnectionStatus("Подключено");
                await Task.Delay(300, cancellationToken);
            }
            catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
            {
                return;
            }
            catch (Exception error)
            {
                await _ipc.DisposeAsync();
                _reconnectAttempt = Math.Min(_reconnectAttempt + 1, 6);
                SetConnectionStatus($"IPC отключён, переподключение: {error.Message}");
                await Task.Delay(
                    TimeSpan.FromMilliseconds(250 * Math.Pow(2, _reconnectAttempt)),
                    cancellationToken);
            }
        }
    }

    private void SetConnectionStatus(string text) =>
        _ = DispatcherQueue.TryEnqueue(() => ConnectionStatus.Text = text);
}
