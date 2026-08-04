using Microsoft.UI.Xaml;
using EvoHime.Desktop.Services;
using Windows.Storage.Pickers;
using WinRT.Interop;
using System.Text.Json;

namespace EvoHime.Desktop;

public partial class MainWindow : Window
{
    public event Action<string, string>? NotificationRequested;
    public event Action? UpdateReadyToInstall;

    private readonly CoreIpcClient _ipc = new("evohime-core-v1");
    private readonly NativeShellState _state = new();
    private readonly WorkspaceSettings _settings = new();
    private readonly UpdateService _updates = new();
    private CancellationTokenSource? _eventCts;
    private string? _activeTaskId;
    private int _reconnectAttempt;
    private string? _pendingApprovalId;

    public MainWindow()
    {
        InitializeComponent();
        _state.SelectWorkspace(Environment.CurrentDirectory);
        WorkspacePathText.Text = $"Workspace: {_state.WorkspacePath}";
        _ = RestoreWorkspaceAsync();
        _ = CheckForUpdatesAsync();
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
            await _ipc.StartTaskAsync(
                _activeTaskId,
                prompt,
                _state.WorkspacePath ?? Environment.CurrentDirectory,
                CancellationToken.None);
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
                            var text = NativeEventFormatter.Format(envelope);
                            _ = DispatcherQueue.TryEnqueue(() => EventLog.Text += text + Environment.NewLine);
                            if (envelope.EventType == "approval.required")
                            {
                                ShowApproval(envelope);
                            }
                            if (envelope.TaskId == _activeTaskId)
                            {
                                var notification = envelope.EventType switch
                                {
                                    "task.completed" => ("Задача завершена", "EvoHime завершила задачу."),
                                    "task.failed" => ("Задача завершилась с ошибкой", "Проверьте журнал событий EvoHime."),
                                    "task.stopped" => ("Задача остановлена", "Выполнение остановлено пользователем."),
                                    _ => ((string Title, string Message)?)null,
                                };
                                if (notification is not null)
                                {
                                    _ = DispatcherQueue.TryEnqueue(() =>
                                        NotificationRequested?.Invoke(notification.Value.Title, notification.Value.Message));
                                }
                            }
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

    private async Task CheckForUpdatesAsync()
    {
        try
        {
            var update = await _updates.CheckLatestAsync(UpdateService.CurrentVersion, CancellationToken.None);
            if (update is null)
            {
                return;
            }

            _availableUpdate = update;
            _ = DispatcherQueue.TryEnqueue(() =>
            {
                UpdateStatusText.Text = $"Доступна Ева {update.Version}";
                UpdateButton.Visibility = Microsoft.UI.Xaml.Visibility.Visible;
            });
        }
        catch (Exception error) when (error is HttpRequestException or TaskCanceledException or JsonException)
        {
            SetConnectionStatus("Проверка обновлений недоступна.");
        }
    }

    private UpdateInfo? _availableUpdate;

    private async void UpdateButton_Click(object sender, RoutedEventArgs e)
    {
        if (_availableUpdate is null)
        {
            return;
        }

        try
        {
            UpdateButton.IsEnabled = false;
            UpdateStatusText.Text = "Загрузка обновления...";
            var installer = await _updates.DownloadInstallerAsync(_availableUpdate, CancellationToken.None);
            UpdateReadyToInstall?.Invoke();
            await Task.Delay(250);
            UpdateService.LaunchUpdater(installer, AppContext.BaseDirectory);
        }
        catch (Exception error)
        {
            UpdateButton.IsEnabled = true;
            UpdateStatusText.Text = "Обновление не установлено.";
            SetConnectionStatus($"Ошибка обновления: {error.Message}");
        }
    }

    private void ShowApproval(CoreEventEnvelope envelope)
    {
        try
        {
            using var json = JsonDocument.Parse(envelope.Payload);
            var root = json.RootElement;
            _pendingApprovalId = root.GetProperty("approval_id").GetString();
            var tool = root.GetProperty("tool_name").GetString() ?? "tool";
            var permission = root.GetProperty("permission").GetString() ?? "permission";
            var scope = root.GetProperty("scope").GetString() ?? "workspace";
            _ = DispatcherQueue.TryEnqueue(() =>
            {
                ApprovalText.Text = $"Требуется разрешение: {tool} · {permission} · {scope}";
                ApprovalPanel.Visibility = Microsoft.UI.Xaml.Visibility.Visible;
            });
        }
        catch (JsonException)
        {
            SetConnectionStatus("Получен повреждённый approval-запрос.");
        }
    }

    private async void ApproveButton_Click(object sender, RoutedEventArgs e) => await ResolveApprovalAsync(true);

    private async void DenyButton_Click(object sender, RoutedEventArgs e) => await ResolveApprovalAsync(false);

    private async Task ResolveApprovalAsync(bool granted)
    {
        var approvalId = _pendingApprovalId;
        if (string.IsNullOrWhiteSpace(approvalId))
        {
            return;
        }

        try
        {
            await _ipc.ResolveApprovalAsync(approvalId, granted, CancellationToken.None);
            _pendingApprovalId = null;
            ApprovalPanel.Visibility = Microsoft.UI.Xaml.Visibility.Collapsed;
        }
        catch (Exception error)
        {
            SetConnectionStatus($"Ошибка approval: {error.Message}");
        }
    }
}
