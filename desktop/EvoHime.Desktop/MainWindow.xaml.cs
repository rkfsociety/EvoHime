using System.Diagnostics;
using System.Security.Cryptography;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Input;
using EvoHime.Desktop.Services;
using Windows.Storage.Pickers;
using Windows.Storage;
using Windows.System;
using Windows.UI.Core;
using Microsoft.UI.Input;
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
    private readonly SemaphoreSlim _ipcRequestGate = new(1, 1);
    private readonly NativeShellState _state = new();
    private readonly WorkspaceSettings _settings = new();
    private readonly UpdateService _updates = new();
    private readonly ProjectCatalogService _projectCatalogService = new();
    private ProjectCatalog _projectCatalog = new();
    private StackPanel? _projectListPanel;
    private string? _activeProjectId;
    private string? _activeChatId;
    private CancellationTokenSource? _eventCts;
    private string? _activeTaskId;
    private int _reconnectAttempt;
    private string? _pendingApprovalId;
    private Button? _modelButton;
    private Grid? _homeContent;
    private Grid? _settingsView;
    private TextBlock? _settingsWorkspaceText;
    private readonly ProviderSettingsService _providerSettings = new();
    private TextBox? _providerBox;
    private TextBox? _baseUrlBox;
    private PasswordBox? _apiKeyBox;
    private TextBlock? _settingsSaveStatus;
    private ComboBox? _modelModeBox;
    private ComboBox? _modelSelector;
    private string _configuredModel = string.Empty;
    private readonly List<StorageFile> _attachments = [];
    private TextBlock? _attachmentsText;
    private string _permissionMode = "ask";

    public MainWindow()
    {
        BuildUi();
        _state.SelectWorkspace(Environment.CurrentDirectory);
        _projectCatalog = _projectCatalogService.Load();
        EnsureActiveProject(_state.WorkspacePath);
        WorkspacePathText.Text = $"Workspace: {_state.WorkspacePath}";
        _ = LoadModelConfigAsync();
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
            _activeChatId = null;
            ConnectionStatus.Text = "●  Готова";
        };
        Grid.SetRow(newChat, 1);
        sidebar.Children.Add(newChat);

        var navItems = new StackPanel { Spacing = 4 };
        _projectListPanel = new StackPanel { Spacing = 2 };
        foreach (var item in ShellNavigationCatalog.Items.Where(item => item.Title is not ("Новый чат" or "Проекты" or "Настройки")))
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
            if (item.Title == "Настройки")
            {
                button.Click += (_, _) => ShowSettingsView();
            }
            navItems.Children.Add(button);
        }
        navItems.Children.Add(new TextBlock
        {
            Text = "ПРОЕКТЫ",
            FontSize = 11,
            Foreground = muted,
            Margin = new Thickness(4, 18, 0, 2),
        });
        navItems.Children.Add(new ScrollViewer
        {
            Content = _projectListPanel,
            MaxHeight = 260,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
        });
        var settingsItem = ShellNavigationCatalog.Items.First(item => item.Title == "Настройки");
        var settingsButton = new Button
        {
            Content = $"{settingsItem.Glyph}   {settingsItem.Title}",
            Tag = settingsItem.Description,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Foreground = text,
        };
        settingsButton.Click += (_, _) => ShowSettingsView();
        navItems.Children.Add(settingsButton);
        Grid.SetRow(navItems, 2);
        sidebar.Children.Add(navItems);
        var workspaceInfo = new StackPanel { Spacing = 5 };
        workspaceInfo.Children.Add(new TextBlock { Text = "РАБОЧЕЕ ПРОСТРАНСТВО", FontSize = 11, Foreground = muted });
        workspaceInfo.Children.Add(new TextBlock { Text = "⌂  Текущий проект", Foreground = muted, Padding = new Thickness(4, 8, 4, 8) });
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
        PromptBox.KeyDown += PromptBox_KeyDown;
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
        var composerBody = new StackPanel { Spacing = 4 };
        composerBody.Children.Add(PromptBox);
        _attachmentsText = new TextBlock
        {
            Visibility = Visibility.Collapsed,
            Foreground = muted,
            FontSize = 11,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        composerBody.Children.Add(_attachmentsText);
        composerGrid.Children.Add(composerBody);
        var composerActions = new Grid { ColumnSpacing = 8 };
        composerActions.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        composerActions.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        composerActions.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        composerActions.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        composerActions.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var attachButton = new Button
        {
            Content = "+",
            FontSize = 22,
            Foreground = muted,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(3, 0, 3, 0),
        };
        ToolTipService.SetToolTip(attachButton, "Добавить файлы");
        attachButton.Click += AttachFiles_Click;
        composerActions.Children.Add(attachButton);
        var accessButton = new Button
        {
            Content = "◉  С подтверждением",
            Foreground = new SolidColorBrush(Windows.UI.Color.FromArgb(255, 239, 133, 80)),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(4, 5, 6, 5),
        };
        Grid.SetColumn(accessButton, 1);
        composerActions.Children.Add(accessButton);
        _modelButton = new Button
        {
            Content = "Модель не выбрана",
            Foreground = text,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(6, 5, 6, 5),
        };
        accessButton.Click += AccessButton_Click;
        _modelButton.Click += ModelButton_Click;
        Grid.SetColumn(_modelButton, 3);
        composerActions.Children.Add(_modelButton);
        Grid.SetColumn(StartButton, 4);
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
        _homeContent = content;
        _settingsView = BuildSettingsView();
        Grid.SetColumn(_settingsView, 1);
        _settingsView.Visibility = Visibility.Collapsed;
        root.Children.Add(_settingsView);
        var providerSettings = _providerSettings.Load();
        if (!string.IsNullOrWhiteSpace(providerSettings.Model))
        {
            _modelButton.Content = $"{providerSettings.Provider}: {providerSettings.Model} ⌄";
        }
        Content = root;
    }

    private Grid BuildSettingsView()
    {
        var text = ThemeBrush("TextBrush", 247, 244, 245);
        var muted = ThemeBrush("MutedTextBrush", 143, 146, 157);
        var surface = ThemeBrush("SurfaceBrush", 16, 20, 27);
        var raised = ThemeBrush("SurfaceRaisedBrush", 23, 28, 37);
        var view = new Grid { Margin = new Thickness(30, 24, 30, 22) };
        view.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        view.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        view.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        var header = new Grid { Margin = new Thickness(0, 0, 0, 22) };
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var title = new StackPanel { Spacing = 5 };
        title.Children.Add(new TextBlock { Text = "Настройки", FontSize = 28, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Foreground = text });
        title.Children.Add(new TextBlock { Text = "Конфигурация локального агента Евы", FontSize = 14, Foreground = muted });
        header.Children.Add(title);
        var back = new Button { Content = "←  Вернуться к чату" };
        back.Click += (_, _) => ShowHomeView();
        Grid.SetColumn(back, 1);
        header.Children.Add(back);
        view.Children.Add(header);

        var sections = new StackPanel { Spacing = 14 };
        var providerSettings = _providerSettings.Load();
        _providerBox = new TextBox { Header = "Провайдер", Text = providerSettings.Provider, IsReadOnly = true };
        _baseUrlBox = new TextBox { Header = "Base URL", Text = providerSettings.BaseUrl };
        _configuredModel = providerSettings.Model;
        _apiKeyBox = new PasswordBox { Header = "API-ключ", PlaceholderText = "Введите ключ провайдера" };
        _apiKeyBox.Password = providerSettings.ApiKey;
        _modelModeBox = new ComboBox { Header = "Режим каталога", ItemsSource = new[] { "Бесплатные", "Платные" }, SelectedIndex = 0 };
        _modelSelector = new ComboBox { Header = "Модель", PlaceholderText = "Загрузка списка моделей..." };
        _modelModeBox.SelectionChanged += (_, _) =>
        {
            if (_modelModeBox.SelectedIndex >= 0)
            {
                _ = LoadModelCatalogAsync(_modelModeBox.SelectedIndex == 0 ? "free" : "paid");
            }
        };
        _modelSelector.SelectionChanged += (_, _) =>
        {
            if (_modelSelector.SelectedItem is string model)
            {
                _configuredModel = model;
            }
        };
        var saveProvider = new Button { Content = "Сохранить настройки провайдера", Margin = new Thickness(0, 8, 0, 0) };
        saveProvider.Click += SaveProviderSettings_Click;
        _settingsSaveStatus = new TextBlock { FontSize = 12, Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157), TextWrapping = TextWrapping.Wrap };
        var providerForm = new StackPanel { Spacing = 10 };
        providerForm.Children.Add(_providerBox);
        providerForm.Children.Add(_baseUrlBox);
        providerForm.Children.Add(_modelModeBox);
        providerForm.Children.Add(_modelSelector);
        providerForm.Children.Add(_apiKeyBox);
        providerForm.Children.Add(new TextBlock { Text = "Ключ хранится в профиле Windows через DPAPI и не записывается в репозиторий.", FontSize = 11, Foreground = muted, TextWrapping = TextWrapping.Wrap });
        providerForm.Children.Add(saveProvider);
        providerForm.Children.Add(_settingsSaveStatus);
        sections.Children.Add(CreateSettingsSection(
            "Модель и провайдер",
            "Параметры LiteRouter или другого OpenAI-compatible провайдера.",
            providerForm,
            raised));

        _settingsWorkspaceText = new TextBlock
        {
            Text = _state.WorkspacePath ?? "Workspace не выбран",
            TextWrapping = TextWrapping.Wrap,
            Foreground = text,
            FontSize = 14,
        };
        var chooseWorkspace = new Button { Content = "Изменить workspace", Margin = new Thickness(0, 12, 0, 0) };
        chooseWorkspace.Click += ChooseSettingsWorkspace_Click;
        sections.Children.Add(CreateSettingsSection(
            "Рабочее пространство",
            "Папка, в которой Ева выполняет задачи.",
            new StackPanel { Children = { _settingsWorkspaceText, chooseWorkspace } },
            surface));

        var dataDirectory = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "EvoHime");
        var runtime = new StackPanel { Spacing = 7 };
        runtime.Children.Add(CreateSettingsStatusLine("Core", "Внутренний процесс агента", "Запускается вместе с клиентом", text, muted));
        runtime.Children.Add(CreateSettingsStatusLine("IPC", "Связь с Core", "Versioned named pipe", text, muted));
        runtime.Children.Add(CreateSettingsStatusLine("Данные", "Локальное хранилище", dataDirectory, text, muted));
        var diagnostics = new Button { Content = "Открыть папку диагностики", Margin = new Thickness(0, 8, 0, 0) };
        diagnostics.Click += (_, _) =>
        {
            var logs = Path.Combine(dataDirectory, "logs");
            Directory.CreateDirectory(logs);
            Process.Start(new ProcessStartInfo { FileName = "explorer.exe", Arguments = $"\"{logs}\"", UseShellExecute = true });
        };
        runtime.Children.Add(diagnostics);
        sections.Children.Add(CreateSettingsSection("Состояние и диагностика", "Служебная информация приложения.", runtime, raised));

        var scroll = new ScrollViewer { Content = sections, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        Grid.SetRow(scroll, 1);
        view.Children.Add(scroll);
        Grid.SetRow(view, 0);
        return view;
    }

    private static Border CreateSettingsSection(string title, string description, UIElement content, Brush background) =>
        new()
        {
            Background = background,
            BorderBrush = ThemeBrush("BorderBrush", 68, 32, 43),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(12),
            Padding = new Thickness(18),
            Child = new StackPanel
            {
                Spacing = 7,
                Children =
                {
                    new TextBlock { Text = title, FontSize = 16, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Foreground = ThemeBrush("TextBrush", 247, 244, 245) },
                    new TextBlock { Text = description, FontSize = 12, Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157) },
                    content,
                },
            },
        };

    private static StackPanel CreateSettingsStatusLine(string title, string description, string value, Brush text, Brush muted)
    {
        var line = new Grid { ColumnSpacing = 12 };
        line.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        line.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        line.Children.Add(new StackPanel
        {
            Children =
            {
                new TextBlock { Text = title, Foreground = text, FontSize = 13 },
                new TextBlock { Text = description, Foreground = muted, FontSize = 11 },
            },
        });
        var valueText = new TextBlock { Text = value, Foreground = muted, FontSize = 11, TextTrimming = TextTrimming.CharacterEllipsis, MaxWidth = 360, VerticalAlignment = VerticalAlignment.Center };
        Grid.SetColumn(valueText, 1);
        line.Children.Add(valueText);
        return new StackPanel { Children = { line } };
    }

    private void ShowSettingsView()
    {
        if (_homeContent is not null && _settingsView is not null)
        {
            _homeContent.Visibility = Visibility.Collapsed;
            _settingsView.Visibility = Visibility.Visible;
            _ = LoadModelCatalogAsync(_modelModeBox?.SelectedIndex == 1 ? "paid" : "free");
        }
    }

    private void ShowHomeView()
    {
        if (_homeContent is not null && _settingsView is not null)
        {
            _settingsView.Visibility = Visibility.Collapsed;
            _homeContent.Visibility = Visibility.Visible;
        }
    }

    private async void ChooseSettingsWorkspace_Click(object sender, RoutedEventArgs e)
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
        EnsureActiveProject(folder.Path);
        WorkspacePathText.Text = $"Workspace: {folder.Path}";
        if (_settingsWorkspaceText is not null)
        {
            _settingsWorkspaceText.Text = folder.Path;
        }
    }

    private async void SaveProviderSettings_Click(object sender, RoutedEventArgs e)
    {
        if (_providerBox is null || _baseUrlBox is null || _modelSelector is null || _apiKeyBox is null)
        {
            return;
        }

        try
        {
            var selectedModel = _modelSelector.SelectedItem as string;
            if (string.IsNullOrWhiteSpace(selectedModel))
            {
                throw new InvalidOperationException("Сначала выберите модель из списка.");
            }

            _providerSettings.Save(new ProviderSettings(
                _providerBox.Text.Trim(),
                _baseUrlBox.Text.Trim(),
                selectedModel,
                _apiKeyBox.Password));
            if (_settingsSaveStatus is not null)
            {
                _settingsSaveStatus.Text = "Сохранено. Загружаю модели...";
                _settingsSaveStatus.Foreground = ThemeBrush("TealBrush", 255, 59, 95);
            }

            if (Application.Current is not App app || !app.RestartCore())
            {
                throw new InvalidOperationException("Не удалось перезапустить Core для применения ключа.");
            }

            await _ipc.DisposeAsync();
            var modelsLoaded = await LoadModelCatalogAsync(_modelModeBox?.SelectedIndex == 1 ? "paid" : "free");
            await LoadModelConfigAsync();
            if (_settingsSaveStatus is not null)
            {
                _settingsSaveStatus.Text = modelsLoaded
                    ? "Сохранено. Модели загружены."
                    : "Ключ сохранён, но провайдер не вернул каталог моделей.";
            }
        }
        catch (Exception error) when (error is IOException or CryptographicException or InvalidOperationException)
        {
            if (_settingsSaveStatus is not null)
            {
                _settingsSaveStatus.Text = $"Не удалось сохранить настройки: {error.Message}";
            }
        }
    }

    private async Task<bool> LoadModelCatalogAsync(string mode)
    {
        await _ipcRequestGate.WaitAsync();
        try
        {
            return await LoadModelCatalogCoreAsync(mode);
        }
        finally
        {
            _ipcRequestGate.Release();
        }
    }

    private void EnsureActiveProject(string? path)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return;
        }

        var project = _projectCatalogService.EnsureProject(_projectCatalog, path);
        _activeProjectId = project.Id;
        _projectCatalogService.Save(_projectCatalog);
        RefreshProjectSidebar();
    }

    private void RefreshProjectSidebar()
    {
        if (_projectListPanel is null)
        {
            return;
        }

        _projectListPanel.Children.Clear();
        foreach (var project in _projectCatalog.Projects)
        {
            var projectButton = new Button
            {
                Content = $"{(project.Id == _activeProjectId ? "●" : "⌂")}  {project.Name}",
                Tag = project,
                HorizontalAlignment = HorizontalAlignment.Stretch,
                HorizontalContentAlignment = HorizontalAlignment.Left,
                Background = project.Id == _activeProjectId
                    ? ThemeBrush("SurfaceRaisedBrush", 34, 38, 53)
                    : new SolidColorBrush(Microsoft.UI.Colors.Transparent),
                Padding = new Thickness(4, 6, 4, 6),
            };
            projectButton.Click += (_, _) => SelectProject(project);
            _projectListPanel.Children.Add(projectButton);

            foreach (var chat in project.Chats.Take(8))
            {
                var chatButton = new Button
                {
                    Content = $"   ·  {chat.Title}",
                    Tag = chat,
                    HorizontalAlignment = HorizontalAlignment.Stretch,
                    HorizontalContentAlignment = HorizontalAlignment.Left,
                    Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
                    Foreground = ThemeBrush("MutedTextBrush", 146, 152, 173),
                    Padding = new Thickness(12, 4, 4, 4),
                };
                chatButton.Click += (_, _) => SelectChat(project, chat);
                _projectListPanel.Children.Add(chatButton);
            }
        }
    }

    private async void SelectProject(ProjectEntry project)
    {
        _activeProjectId = project.Id;
        _activeChatId = null;
        _state.SelectWorkspace(project.Path);
        await _settings.SaveWorkspaceAsync(project.Path);
        WorkspacePathText.Text = $"Workspace: {_state.WorkspacePath}";
        RefreshProjectSidebar();
        ShowHomeView();
        ConnectionStatus.Text = $"Проект: {project.Name}";
    }

    private void SelectChat(ProjectEntry project, ChatEntry chat)
    {
        _activeProjectId = project.Id;
        _activeChatId = chat.Id;
        _state.SelectWorkspace(project.Path);
        WorkspacePathText.Text = $"Workspace: {_state.WorkspacePath}";
        RefreshProjectSidebar();
        ShowHomeView();
        EventLog.Text = $"Чат «{chat.Title}» выбран. Новые события появятся здесь.";
        ConnectionStatus.Text = $"Проект: {project.Name} · чат: {chat.Title}";
    }

    private ProjectEntry? ActiveProject() =>
        _projectCatalog.Projects.FirstOrDefault(project => project.Id == _activeProjectId);

    private async void ModelButton_Click(object sender, RoutedEventArgs e)
    {
        var selector = new ComboBox
        {
            Width = 440,
            PlaceholderText = "Загрузка моделей...",
        };
        var dialog = new ContentDialog
        {
            Title = "Выберите модель",
            Content = selector,
            PrimaryButtonText = "Выбрать",
            CloseButtonText = "Отмена",
            IsPrimaryButtonEnabled = false,
            XamlRoot = ((FrameworkElement)Content).XamlRoot,
        };

        var loadTask = FetchComposerModelsAsync();
        var dialogTask = dialog.ShowAsync().AsTask();
        try
        {
            var models = await loadTask;
            foreach (var availableModel in models)
            {
                selector.Items.Add(availableModel);
            }
            var currentModel = _providerSettings.Load().Model;
            selector.SelectedItem = models.Contains(currentModel) ? currentModel : models.FirstOrDefault();
            selector.PlaceholderText = models.Count == 0 ? "Модели не найдены" : "Выберите модель";
            dialog.IsPrimaryButtonEnabled = models.Count > 0;
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            selector.PlaceholderText = $"Не удалось загрузить модели: {error.Message}";
        }

        if (await dialogTask == ContentDialogResult.Primary && selector.SelectedItem is string model)
        {
            await ApplyComposerModelAsync(model);
        }
    }

    private async Task<List<string>> FetchComposerModelsAsync()
    {
        await _ipcRequestGate.WaitAsync();
        try
        {
            if (!_ipc.IsConnected)
            {
                await ConnectToCoreWithRetryAsync(CancellationToken.None);
            }

            var models = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            foreach (var mode in new[] { "free", "paid" })
            {
                using var requestTimeout = new CancellationTokenSource(TimeSpan.FromSeconds(30));
                await _ipc.RequestModelCatalogAsync(mode, requestTimeout.Token);
                var response = await _ipc.ReadEventAsync(requestTimeout.Token);
                if (response.EventType != "model.catalog")
                {
                    throw new InvalidOperationException("Core не вернул каталог моделей.");
                }

                using var json = JsonDocument.Parse(response.Payload);
                if (json.RootElement.TryGetProperty("models", out var modelsValue))
                {
                    foreach (var model in modelsValue.EnumerateArray()
                                 .Select(item => item.GetString())
                                 .Where(item => !string.IsNullOrWhiteSpace(item)))
                    {
                        models.Add(model!);
                    }
                }
            }
            return models.OrderBy(model => model, StringComparer.OrdinalIgnoreCase).ToList();
        }
        finally
        {
            _ipcRequestGate.Release();
        }
    }

    private async Task ApplyComposerModelAsync(string model)
    {
        var settings = _providerSettings.Load();
        _providerSettings.Save(settings with { Model = model });
        if (Application.Current is not App app || !app.RestartCore())
        {
            throw new InvalidOperationException("Не удалось перезапустить Core для применения модели.");
        }

        await _ipc.DisposeAsync();
        if (_modelButton is not null)
        {
            _modelButton.Content = $"{settings.Provider}: {model} ⌄";
        }
        ConnectionStatus.Text = "Модель обновляется...";
        await LoadModelConfigAsync();
        ConnectionStatus.Text = $"Модель: {model}";
    }

    private void PromptBox_KeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != VirtualKey.Enter)
        {
            return;
        }

        var shiftState = InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Shift);
        if ((shiftState & CoreVirtualKeyStates.Down) != 0)
        {
            return;
        }

        e.Handled = true;
        StartButton_Click(sender, e);
    }

    private async void AttachFiles_Click(object sender, RoutedEventArgs e)
    {
        var picker = new FileOpenPicker();
        picker.FileTypeFilter.Add("*");
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var files = await picker.PickMultipleFilesAsync();
        if (files is null || files.Count == 0)
        {
            return;
        }

        foreach (var file in files)
        {
            if (_attachments.All(item => !string.Equals(item.Path, file.Path, StringComparison.OrdinalIgnoreCase)))
            {
                _attachments.Add(file);
            }
        }
        UpdateAttachmentsText();
    }

    private void UpdateAttachmentsText()
    {
        if (_attachmentsText is null)
        {
            return;
        }

        _attachmentsText.Text = _attachments.Count == 0
            ? string.Empty
            : $"Прикреплено файлов: {_attachments.Count} · {string.Join(", ", _attachments.Select(file => file.Name))}";
        _attachmentsText.Visibility = _attachments.Count == 0 ? Visibility.Collapsed : Visibility.Visible;
    }

    private void AccessButton_Click(object sender, RoutedEventArgs e)
    {
        var menu = new MenuFlyout();
        var button = sender as Button;
        AddPermissionMenuItem(menu, button, "ask", "◉  С подтверждением");
        AddPermissionMenuItem(menu, button, "read_only", "◌  Только чтение");
        AddPermissionMenuItem(menu, button, "full", "⚡  Полный доступ");
        menu.ShowAt((FrameworkElement)sender);
    }

    private void AddPermissionMenuItem(MenuFlyout menu, Button? button, string mode, string label)
    {
        var item = new MenuFlyoutItem { Text = label };
        item.Click += async (_, _) =>
        {
            _permissionMode = mode;
            if (button is not null)
            {
                button.Content = label;
            }
            try
            {
                if (!_ipc.IsConnected)
                {
                    await _ipc.ConnectAndHandshakeAsync(CancellationToken.None);
                    _ = await _ipc.ReadEventAsync(CancellationToken.None);
                }
                await _ipc.SetPermissionModeAsync(mode, CancellationToken.None);
                ConnectionStatus.Text = $"Режим: {label.Replace("◉  ", "").Replace("◌  ", "").Replace("⚡  ", "")}";
            }
            catch (Exception error)
            {
                ConnectionStatus.Text = $"Не удалось применить режим: {error.Message}";
            }
        };
        menu.Items.Add(item);
    }


    private async Task<bool> LoadModelCatalogCoreAsync(string mode)
    {
        if (_modelSelector is null)
        {
            return false;
        }

        StartupDiagnostics.Write($"Model catalog: start mode={mode}");
        try
        {
            if (!_ipc.IsConnected)
            {
                StartupDiagnostics.Write("Model catalog: connecting to Core");
                await ConnectToCoreWithRetryAsync(CancellationToken.None);
            }

            StartupDiagnostics.Write("Model catalog: request sent");
            using var requestTimeout = new CancellationTokenSource(TimeSpan.FromSeconds(30));
            await _ipc.RequestModelCatalogAsync(mode, requestTimeout.Token);
            var response = await _ipc.ReadEventAsync(requestTimeout.Token);
            StartupDiagnostics.Write($"Model catalog: response event={response.EventType}, bytes={response.Payload.Length}");
            if (response.EventType != "model.catalog")
            {
                throw new InvalidOperationException("Core не вернул каталог моделей.");
            }

            using var json = JsonDocument.Parse(response.Payload);
            var models = json.RootElement.TryGetProperty("models", out var modelsValue)
                ? modelsValue.EnumerateArray().Select(item => item.GetString()).Where(item => !string.IsNullOrWhiteSpace(item)).Cast<string>().ToList()
                : [];
            _ = DispatcherQueue.TryEnqueue(() =>
            {
                _modelSelector.Items.Clear();
                foreach (var model in models)
                {
                    _modelSelector.Items.Add(model);
                }
                if (!string.IsNullOrWhiteSpace(_configuredModel) && models.Contains(_configuredModel))
                {
                    _modelSelector.SelectedItem = _configuredModel;
                }
                else if (models.Count > 0)
                {
                    _modelSelector.SelectedIndex = 0;
                }
                if (models.Count == 0 && _settingsSaveStatus is not null)
                {
                    _settingsSaveStatus.Text = "Провайдер не вернул модели для выбранного режима.";
                }
            });
            StartupDiagnostics.Write($"Model catalog: parsed count={models.Count}");
            return models.Count > 0;
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            StartupDiagnostics.Write($"Model catalog failed: {error.GetType().Name}: {error.Message}");
            _ = DispatcherQueue.TryEnqueue(() =>
            {
                if (_settingsSaveStatus is not null)
                {
                    _settingsSaveStatus.Text = $"Не удалось получить список моделей: {error.Message}";
                }
            });
            return false;
        }
    }

    private async Task ConnectToCoreWithRetryAsync(CancellationToken cancellationToken)
    {
        Exception? lastError = null;
        for (var attempt = 0; attempt < 20; attempt++)
        {
            try
            {
                using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
                timeout.CancelAfter(TimeSpan.FromSeconds(1));
                await _ipc.ConnectAndHandshakeAsync(timeout.Token);
                var ready = await _ipc.ReadEventAsync(timeout.Token);
                if (ready.EventType != "core.ready")
                {
                    throw new InvalidOperationException("Core не подтвердил готовность.");
                }

                return;
            }
            catch (Exception error) when (error is IOException or InvalidOperationException or OperationCanceledException)
            {
                lastError = error;
                await _ipc.DisposeAsync();
                if (attempt < 19)
                {
                    await Task.Delay(250, cancellationToken);
                }
            }
        }

        throw new IOException("Core не стал доступен после перезапуска.", lastError);
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
        EnsureActiveProject(folder.Path);
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
        EnsureActiveProject(savedPath);
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
            if (!_ipc.IsConnected)
            {
                await _ipc.ConnectAndHandshakeAsync(CancellationToken.None);
            }
            _reconnectAttempt = 0;
            _activeTaskId = Guid.NewGuid().ToString("N");
            var workspacePath = _state.WorkspacePath ?? Environment.CurrentDirectory;
            EnsureActiveProject(workspacePath);
            var activeProject = ActiveProject();
            if (activeProject is not null && _activeChatId is null)
            {
                var title = prompt.Replace("\r", " ").Replace("\n", " ").Trim();
                var chat = _projectCatalogService.AddChat(activeProject, title.Length > 56 ? $"{title[..56]}…" : title);
                _activeChatId = chat.Id;
                _projectCatalogService.Save(_projectCatalog);
                RefreshProjectSidebar();
            }
            var attachmentPaths = await CopyAttachmentsToWorkspaceAsync(workspacePath, _activeTaskId);
            var taskPrompt = prompt;
            if (attachmentPaths.Count > 0)
            {
                taskPrompt += $"\n\nПрикреплённые файлы находятся в workspace по путям:\n- {string.Join("\n- ", attachmentPaths)}\nИзучи их как часть задачи.";
            }
            await _ipc.StartTaskAsync(
                _activeTaskId,
                taskPrompt,
                workspacePath,
                CancellationToken.None);
            _attachments.Clear();
            UpdateAttachmentsText();
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


    private async Task LoadModelConfigAsync()
    {
        await _ipcRequestGate.WaitAsync();
        try
        {
            await LoadModelConfigCoreAsync();
        }
        finally
        {
            _ipcRequestGate.Release();
        }
    }

    private async Task<List<string>> CopyAttachmentsToWorkspaceAsync(string workspacePath, string taskId)
    {
        var copiedPaths = new List<string>();
        if (_attachments.Count == 0)
        {
            return copiedPaths;
        }

        var attachmentDirectory = Path.Combine(workspacePath, ".evohime", "attachments", taskId);
        Directory.CreateDirectory(attachmentDirectory);
        foreach (var file in _attachments)
        {
            var destination = Path.Combine(attachmentDirectory, Path.GetFileName(file.Name));
            var suffix = 1;
            while (File.Exists(destination))
            {
                destination = Path.Combine(
                    attachmentDirectory,
                    $"{Path.GetFileNameWithoutExtension(file.Name)}-{suffix++}{Path.GetExtension(file.Name)}");
            }

            await using var source = File.OpenRead(file.Path);
            await using var target = File.Create(destination);
            await source.CopyToAsync(target);
            copiedPaths.Add(Path.GetRelativePath(workspacePath, destination));
        }
        return copiedPaths;
    }

    private async Task LoadModelConfigCoreAsync()
    {
        StartupDiagnostics.Write("Model config: start");
        try
        {
            using var requestTimeout = new CancellationTokenSource(TimeSpan.FromSeconds(15));
            if (!_ipc.IsConnected)
            {
                StartupDiagnostics.Write("Model config: connecting to Core");
                await ConnectToCoreWithRetryAsync(requestTimeout.Token);
            }

            StartupDiagnostics.Write("Model config: request sent");
            await _ipc.RequestModelConfigAsync(requestTimeout.Token);
            var response = await _ipc.ReadEventAsync(requestTimeout.Token);
            StartupDiagnostics.Write($"Model config: response event={response.EventType}, bytes={response.Payload.Length}");
            if (response.EventType != "model.config")
            {
                throw new InvalidOperationException("Core не вернул конфигурацию модели.");
            }

            using var json = JsonDocument.Parse(response.Payload);
            var root = json.RootElement;
            var provider = root.TryGetProperty("provider", out var providerValue)
                ? providerValue.GetString()
                : null;
            var model = root.TryGetProperty("model", out var modelValue)
                ? modelValue.GetString()
                : null;
            var configured = root.TryGetProperty("configured", out var configuredValue)
                && configuredValue.GetBoolean();
            var label = configured && !string.IsNullOrWhiteSpace(model)
                ? $"{provider}: {model} ⌄"
                : "Провайдер не настроен";
            _ = DispatcherQueue.TryEnqueue(() =>
            {
                if (_modelButton is not null)
                {
                    _modelButton.Content = label;
                }
            });
            StartupDiagnostics.Write("Model config: completed");
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            StartupDiagnostics.Write($"Model config failed: {error.GetType().Name}: {error.Message}");
            _ = DispatcherQueue.TryEnqueue(() =>
            {
                if (_modelButton is not null)
                {
                    _modelButton.Content = "Модель недоступна";
                }
            });
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
