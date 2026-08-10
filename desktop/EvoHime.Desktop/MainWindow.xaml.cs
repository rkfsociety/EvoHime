using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using EvoHime.Desktop.Services;
using Windows.Storage.Pickers;
using WinRT.Interop;
using System.Text.Json;

namespace EvoHime.Desktop;

public partial class MainWindow : Window
{
    private static Brush ThemeBrush(string key, byte r, byte g, byte b) =>
        Application.Current.Resources.TryGetValue(key, out var value) && value is Brush brush
            ? brush
            : new SolidColorBrush(Windows.UI.Color.FromArgb(255, r, g, b));

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
        BuildUi();
        _state.SelectWorkspace(Environment.CurrentDirectory);
        WorkspacePathText.Text = $"Workspace: {_state.WorkspacePath}";
        _ = RestoreWorkspaceAsync();
        _ = CheckForUpdatesAsync();
    }

    private void BuildUi()
    {
        var text = ThemeBrush("TextBrush", 244, 242, 250);
        var muted = ThemeBrush("MutedTextBrush", 146, 152, 173);
        var surface = ThemeBrush("SurfaceBrush", 25, 28, 39);
        var raised = ThemeBrush("SurfaceRaisedBrush", 34, 38, 53);
        var root = new Grid { Background = ThemeBrush("NightBackgroundBrush", 17, 19, 27) };
        root.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(248) });
        root.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });

        var sidebar = new Grid { Background = surface, Padding = new Thickness(18, 24, 14, 18) };
        sidebar.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        sidebar.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        sidebar.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        sidebar.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        var brand = new StackPanel { Spacing = 2, Margin = new Thickness(4, 0, 0, 24) };
        brand.Children.Add(new TextBlock { Text = "ЕВА", FontSize = 25, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Foreground = text });
        brand.Children.Add(new TextBlock { Text = "локальный AI-агент", FontSize = 12, Foreground = muted });
        sidebar.Children.Add(brand);

        var newChat = new Button
        {
            Content = "+   Новый чат",
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
            Background = ThemeBrush("PurpleBrush", 167, 139, 250),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.White),
            Margin = new Thickness(0, 0, 0, 18),
        };
        newChat.Click += (_, _) =>
        {
            PromptBox.Text = string.Empty;
            EventLog.Text = string.Empty;
            ConnectionStatus.Text = "●  Готова";
        };
        Grid.SetRow(newChat, 1);
        sidebar.Children.Add(newChat);

        var navItems = new StackPanel { Spacing = 4 };
        foreach (var item in ShellNavigationCatalog.Items.Where(item => item.Title != "Новый чат"))
        {
            var button = new Button
            {
                Content = $"{item.Glyph}   {item.Title}",
                Tag = item.Description,
                HorizontalAlignment = HorizontalAlignment.Stretch,
                HorizontalContentAlignment = HorizontalAlignment.Left,
                Background = item.Title == "Пульс" ? raised : new SolidColorBrush(Microsoft.UI.Colors.Transparent),
                Foreground = text,
            };
            navItems.Children.Add(button);
        }
        Grid.SetRow(navItems, 2);
        sidebar.Children.Add(navItems);
        var workspaceInfo = new StackPanel { Spacing = 5 };
        workspaceInfo.Children.Add(new TextBlock { Text = "РАБОЧИЕ ПРОСТРАНСТВА", FontSize = 11, Foreground = muted });
        workspaceInfo.Children.Add(new TextBlock { Text = "⌂  Текущий workspace", Foreground = muted, Padding = new Thickness(4, 8, 4, 8) });
        Grid.SetRow(workspaceInfo, 3);
        sidebar.Children.Add(workspaceInfo);
        Grid.SetColumn(sidebar, 0);
        root.Children.Add(sidebar);

        var content = new Grid { Margin = new Thickness(30, 24, 30, 22) };
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        var title = new StackPanel { Spacing = 5 };
        title.Children.Add(new TextBlock { Text = "Добрый вечер, хозяин", FontSize = 28, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Foreground = text });
        title.Children.Add(new TextBlock { Text = "Опишите задачу — Ева разберётся и покажет, что делает.", FontSize = 14, Foreground = muted });
        var status = new Border { Background = raised, CornerRadius = new CornerRadius(12), Padding = new Thickness(10, 5, 10, 5), VerticalAlignment = VerticalAlignment.Top };
        ConnectionStatus = new TextBlock { Text = "●  Готова", Foreground = ThemeBrush("TealBrush", 89, 216, 200), FontSize = 12 };
        status.Child = ConnectionStatus;
        var header = new Grid { ColumnSpacing = 20, Margin = new Thickness(0, 0, 0, 22) };
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        header.Children.Add(title);
        Grid.SetColumn(status, 1);
        header.Children.Add(status);
        content.Children.Add(header);

        var work = new Grid { RowSpacing = 14 };
        work.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        work.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        work.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        var workspaceBar = new Grid { ColumnSpacing = 12, Margin = new Thickness(0, 0, 0, 4) };
        workspaceBar.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        workspaceBar.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        WorkspacePathText = new TextBlock { Text = "Workspace: не выбран", Foreground = muted, VerticalAlignment = VerticalAlignment.Center, TextTrimming = TextTrimming.CharacterEllipsis };
        workspaceBar.Children.Add(WorkspacePathText);
        ChooseWorkspaceButton = new Button { Content = "Выбрать workspace" };
        ChooseWorkspaceButton.Click += ChooseWorkspaceButton_Click;
        Grid.SetColumn(ChooseWorkspaceButton, 1);
        workspaceBar.Children.Add(ChooseWorkspaceButton);
        work.Children.Add(workspaceBar);

        var card = new Border { Background = surface, BorderBrush = ThemeBrush("BorderBrush", 48, 53, 72), BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(14), Padding = new Thickness(22) };
        var cardGrid = new Grid { RowSpacing = 14 };
        cardGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        cardGrid.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        cardGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        var intro = new StackPanel { Spacing = 8 };
        intro.Children.Add(new TextBlock { Text = "Чем займёмся?", FontSize = 23, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Foreground = text });
        intro.Children.Add(new TextBlock { Text = "Изучу проект, найду проблему, изменю файлы или объясню код.", FontSize = 14, Foreground = muted });
        cardGrid.Children.Add(intro);
        EventLog = new TextBlock
        {
            Text = "Здесь появится план и журнал выполнения задачи.",
            TextWrapping = TextWrapping.Wrap,
            Foreground = muted,
            FontSize = 13,
        };
        var log = new ScrollViewer { Content = EventLog, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        Grid.SetRow(log, 1);
        cardGrid.Children.Add(log);
        PromptBox = new TextBox
        {
            PlaceholderText = "Поручите что угодно",
            AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap,
            MinHeight = 44,
            MaxHeight = 110,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            Foreground = text,
        };
        var transparent = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        foreach (var resourceKey in new[]
        {
            "TextControlBackground",
            "TextControlBackgroundFocused",
            "TextControlBackgroundPointerOver",
            "TextControlBackgroundDisabled",
            "TextControlBorderBrush",
            "TextControlBorderBrushFocused",
            "TextControlBorderBrushPointerOver",
        })
        {
            PromptBox.Resources[resourceKey] = transparent;
        }
        foreach (var resourceKey in new[]
        {
            "TextControlForeground",
            "TextControlForegroundFocused",
            "TextControlForegroundPointerOver",
            "TextControlForegroundDisabled",
        })
        {
            PromptBox.Resources[resourceKey] = text;
        }
        PromptBox.Resources["TextControlPlaceholderForeground"] = muted;
        StartButton = new Button
        {
            Content = "↑",
            Width = 32,
            Height = 32,
            Padding = new Thickness(0),
            CornerRadius = new CornerRadius(16),
            Background = ThemeBrush("TealBrush", 255, 59, 95),
            Foreground = ThemeBrush("TextBrush", 247, 244, 245),
            HorizontalContentAlignment = HorizontalAlignment.Center,
            VerticalContentAlignment = VerticalAlignment.Center,
        };
        StartButton.Click += StartButton_Click;
        var composer = new Border
        {
            Background = ThemeBrush("SurfaceRaisedBrush", 31, 34, 43),
            BorderBrush = ThemeBrush("BorderBrush", 48, 53, 72),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(16),
            Padding = new Thickness(14, 10, 10, 10),
        };
        var composerGrid = new Grid { RowSpacing = 8 };
        composerGrid.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        composerGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        composerGrid.Children.Add(PromptBox);
        var composerActions = new Grid { ColumnSpacing = 8 };
        composerActions.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        composerActions.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        composerActions.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        composerActions.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        composerActions.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        composerActions.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        composerActions.Children.Add(new TextBlock { Text = "+", FontSize = 22, Foreground = muted, VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(0, 0, 3, 0) });
        var accessButton = new Button
        {
            Content = "◉  Полный доступ",
            Foreground = new SolidColorBrush(Windows.UI.Color.FromArgb(255, 239, 133, 80)),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(4, 5, 6, 5),
        };
        Grid.SetColumn(accessButton, 1);
        composerActions.Children.Add(accessButton);
        var modelButton = new Button
        {
            Content = "5.6 Luna  Среднее⌄",
            Foreground = text,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(6, 5, 6, 5),
        };
        Grid.SetColumn(modelButton, 3);
        composerActions.Children.Add(modelButton);
        var microphoneButton = new Button
        {
            Content = "♩",
            Foreground = text,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(7, 5, 7, 5),
        };
        Grid.SetColumn(microphoneButton, 4);
        composerActions.Children.Add(microphoneButton);
        Grid.SetColumn(StartButton, 5);
        composerActions.Children.Add(StartButton);
        Grid.SetRow(composerActions, 1);
        composerGrid.Children.Add(composerActions);
        composer.Child = composerGrid;
        Grid.SetRow(composer, 2);
        cardGrid.Children.Add(composer);
        card.Child = cardGrid;
        Grid.SetRow(card, 1);
        work.Children.Add(card);

        ApprovalPanel = new StackPanel { Spacing = 8, Visibility = Visibility.Collapsed, Background = raised, Padding = new Thickness(14) };
        ApprovalText = new TextBlock { TextWrapping = TextWrapping.Wrap, Foreground = text };
        ApprovalPanel.Children.Add(ApprovalText);
        var approvalActions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var approve = new Button { Content = "Разрешить" };
        approve.Click += ApproveButton_Click;
        var deny = new Button { Content = "Отклонить" };
        deny.Click += DenyButton_Click;
        approvalActions.Children.Add(approve);
        approvalActions.Children.Add(deny);
        ApprovalPanel.Children.Add(approvalActions);
        Grid.SetRow(ApprovalPanel, 2);
        work.Children.Add(ApprovalPanel);
        Grid.SetRow(work, 1);
        content.Children.Add(work);

        var footer = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10, HorizontalAlignment = HorizontalAlignment.Right, Margin = new Thickness(0, 14, 0, 0) };
        UpdateStatusText = new TextBlock { VerticalAlignment = VerticalAlignment.Center, Foreground = muted, FontSize = 12 };
        UpdateButton = new Button { Content = "Обновить", Visibility = Visibility.Collapsed };
        UpdateButton.Click += UpdateButton_Click;
        StopButton = new Button { Content = "Остановить", IsEnabled = false };
        StopButton.Click += StopButton_Click;
        footer.Children.Add(UpdateStatusText);
        footer.Children.Add(UpdateButton);
        footer.Children.Add(StopButton);
        Grid.SetRow(footer, 2);
        content.Children.Add(footer);
        Grid.SetColumn(content, 1);
        root.Children.Add(content);
        Content = root;
    }

    private void BuildUiLegacy()
    {
        var root = new Grid
        {
            Background = ThemeBrush("NightBackgroundBrush", 17, 19, 27),
        };
        root.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(248) });
        root.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        var navigation = new StackPanel
        {
            Background = ThemeBrush("SurfaceBrush", 25, 28, 39),
            Padding = new Thickness(18, 24, 14, 18),
            Spacing = 8,
        };
        navigation.Children.Add(new TextBlock
        {
            Text = "ЕВА",
            FontSize = 24,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            Foreground = ThemeBrush("TextBrush", 244, 242, 250),
            Margin = new Thickness(4, 0, 0, 18),
        });
        foreach (var item in ShellNavigationCatalog.Items)
        {
            var button = new Button
            {
                HorizontalAlignment = HorizontalAlignment.Stretch,
                HorizontalContentAlignment = HorizontalAlignment.Left,
                Background = item.Title == "Новый чат"
                    ? ThemeBrush("SurfaceRaisedBrush", 34, 38, 53)
                    : new SolidColorBrush(Microsoft.UI.Colors.Transparent),
                Foreground = ThemeBrush("TextBrush", 244, 242, 250),
                Content = $"{item.Glyph}   {item.Title}",
                Tag = item.Description,
            };
            navigation.Children.Add(button);
        }
        var projectLabel = new TextBlock
        {
            Text = "РАБОЧИЕ ПРОСТРАНСТВА",
            FontSize = 11,
            Foreground = ThemeBrush("MutedTextBrush", 146, 152, 173),
            Margin = new Thickness(5, 22, 0, 2),
        };
        navigation.Children.Add(projectLabel);
        navigation.Children.Add(new TextBlock
        {
            Text = "⌂  Текущий workspace",
            Foreground = ThemeBrush("MutedTextBrush", 146, 152, 173),
            Padding = new Thickness(4, 8, 4, 8),
        });
        root.Children.Add(navigation);
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        var title = new StackPanel { Spacing = 4, Margin = new Thickness(28, 22, 0, 12) };
        title.Children.Add(new TextBlock { Text = "Добрый вечер, Роман", FontSize = 27, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Foreground = ThemeBrush("TextBrush", 244, 242, 250) });
        title.Children.Add(new TextBlock { Text = "Ева готова помочь с текущим workspace", FontSize = 13, Foreground = ThemeBrush("MutedTextBrush", 146, 152, 173) });
        ConnectionStatus = new TextBlock { Text = "Не подключено" };
        title.Children.Add(ConnectionStatus);

        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, Margin = new Thickness(0, 22, 24, 12) };
        actions.Children.Add(new TextBox { PlaceholderText = "Поиск по workspace", Width = 190, Height = 36, VerticalAlignment = VerticalAlignment.Top, VerticalContentAlignment = VerticalAlignment.Center });
        actions.Children.Add(new Button { Content = "◌" });
        UpdateStatusText = new TextBlock { VerticalAlignment = VerticalAlignment.Center };
        UpdateButton = new Button { Content = "Обновить", Visibility = Visibility.Collapsed };
        UpdateButton.Click += UpdateButton_Click;
        ChooseWorkspaceButton = new Button { Content = "Выбрать workspace" };
        ChooseWorkspaceButton.Click += ChooseWorkspaceButton_Click;
        actions.Children.Add(UpdateStatusText);
        actions.Children.Add(UpdateButton);
        actions.Children.Add(ChooseWorkspaceButton);

        var header = new Grid { ColumnSpacing = 16 };
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        header.Children.Add(title);
        Grid.SetColumn(actions, 1);
        header.Children.Add(actions);
        Grid.SetColumn(header, 1);
        root.Children.Add(header);

        WorkspacePathText = new TextBlock { Text = "Workspace: не выбран" };
        Grid.SetRow(WorkspacePathText, 1);
        Grid.SetColumn(WorkspacePathText, 1);
        root.Children.Add(WorkspacePathText);

        var taskArea = new Grid { RowSpacing = 12, Margin = new Thickness(28, 8, 28, 20) };
        taskArea.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        taskArea.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        taskArea.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        PromptBox = new TextBox
        {
            Header = "Задача",
            PlaceholderText = "Что нужно сделать?",
            AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap,
        };
        taskArea.Children.Add(PromptBox);
        var welcome = new StackPanel { Spacing = 12, Margin = new Thickness(0, 42, 0, 22) };
        welcome.Children.Add(new TextBlock { Text = "Чем займёмся?", FontSize = 32, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Foreground = ThemeBrush("TextBrush", 244, 242, 250) });
        welcome.Children.Add(new TextBlock { Text = "Опиши задачу — Ева покажет план, запросит разрешения и будет вести журнал выполнения.", Foreground = ThemeBrush("MutedTextBrush", 146, 152, 173), FontSize = 15 });
        var suggestions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        foreach (var suggestion in new[] { "Проверить проект", "Найти проблему", "Объяснить код" })
        {
            suggestions.Children.Add(new Button { Content = suggestion, Foreground = ThemeBrush("TealBrush", 89, 216, 200), Background = ThemeBrush("SurfaceRaisedBrush", 34, 38, 53) });
        }
        welcome.Children.Add(suggestions);
        Grid.SetRow(welcome, 1);
        taskArea.Children.Add(welcome);
        EventLog = new TextBlock { TextWrapping = TextWrapping.Wrap };
        var log = new ScrollViewer { MaxHeight = 190, Content = EventLog };
        Grid.SetRow(log, 1);
        log.VerticalAlignment = VerticalAlignment.Bottom;
        taskArea.Children.Add(log);

        ApprovalPanel = new StackPanel { Spacing = 8, Visibility = Visibility.Collapsed };
        ApprovalText = new TextBlock { TextWrapping = TextWrapping.Wrap };
        ApprovalPanel.Children.Add(ApprovalText);
        var approvalActions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var approve = new Button { Content = "Разрешить" };
        approve.Click += ApproveButton_Click;
        var deny = new Button { Content = "Отклонить" };
        deny.Click += DenyButton_Click;
        approvalActions.Children.Add(approve);
        approvalActions.Children.Add(deny);
        ApprovalPanel.Children.Add(approvalActions);
        Grid.SetRow(ApprovalPanel, 2);
        taskArea.Children.Add(ApprovalPanel);
        Grid.SetRow(taskArea, 2);
        Grid.SetColumn(taskArea, 1);
        root.Children.Add(taskArea);

        var controls = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12, Margin = new Thickness(28, 0, 28, 14) };
        StartButton = new Button { Content = "Запустить" };
        StartButton.Click += StartButton_Click;
        StopButton = new Button { Content = "Stop", IsEnabled = false };
        StopButton.Click += StopButton_Click;
        controls.Children.Add(StartButton);
        controls.Children.Add(StopButton);
        Grid.SetRow(controls, 3);
        Grid.SetColumn(controls, 1);
        root.Children.Add(controls);

        Content = root;
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
