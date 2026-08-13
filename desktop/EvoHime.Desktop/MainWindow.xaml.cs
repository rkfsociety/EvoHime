using System.Diagnostics;
using System.Security.Cryptography;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Windowing;
using Microsoft.UI;
using XamlArcSegment = Microsoft.UI.Xaml.Media.ArcSegment;
using XamlEllipse = Microsoft.UI.Xaml.Shapes.Ellipse;
using XamlPathFigure = Microsoft.UI.Xaml.Media.PathFigure;
using XamlPathGeometry = Microsoft.UI.Xaml.Media.PathGeometry;
using XamlPath = Microsoft.UI.Xaml.Shapes.Path;
using Microsoft.UI.Xaml.Input;
using EvoHime.Desktop.Services;
using Windows.Storage.Pickers;
using Windows.Storage;
using Windows.ApplicationModel.DataTransfer;
using Windows.System;
using Windows.UI.Core;
using Microsoft.UI.Input;
using WinRT.Interop;
using System.Text.Json;
using System.Text.Json.Serialization;

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
    private readonly GitHubAuthService _githubAuth = new();
    private readonly ProjectCatalogService _projectCatalogService = new();
    private ProjectCatalog _projectCatalog = new();
    private StackPanel? _projectListPanel;
    private string? _activeProjectId;
    private string? _activeChatId;
    private bool _newChatRequested;
    private CancellationTokenSource? _eventCts;
    private string? _activeTaskId;
    private int _reconnectAttempt;
    private string? _recoveryOutcomeStatus;
    private string? _pendingApprovalId;
    private Button? _modelButton;
    private Button? _contextButton;
    private XamlPath? _contextProgressArc;
    private TextBlock? _contextPercentText;
    private string _modelContextDetails = "Контекст модели ещё не получен.";
    private Grid? _homeContent;
    private Grid? _settingsView;
    private Grid? _scheduledView;
    private Grid? _pluginsView;
    private Grid? _tasksView;
    private Grid? _filesView;
    private Grid? _gitView;
    private Grid? _terminalView;
    private StackPanel? _filesList;
    private TextBox? _filePreview;
    private TextBlock? _filesPathText;
    private string _filesRelativePath = ".";
    private string? _filesSelectedPath;
    private TextBox? _gitPathBox;
    private TextBox? _gitStatusPreview;
    private TextBox? _gitDiffPreview;
    private TextBlock? _gitStatusText;
    private StackPanel? _taskList;
    private TextBlock? _taskWorkspaceStatus;
    private TaskGraphDto? _lastTaskGraph;
    private Microsoft.UI.Xaml.Controls.Canvas? _taskGraphCanvas;
    private readonly HashSet<string> _coreProjects = new(StringComparer.Ordinal);
    private StackPanel? _pluginsList;
    private TextBox? _pluginSearch;
    private TextBlock? _pluginStatus;
    private readonly PluginCatalogService _pluginCatalogService = new();
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
    private TextBlock? _githubProfileText;
    private TextBlock? _githubProfileStatus;
    private Button? _githubProfileButton;
    private StackPanel? _conversationPanel;
    private ScrollViewer? _conversationScroll;
    private TextBlock? _streamingAssistantText;
    private StackPanel? _tracePanel;
    private ScrollViewer? _traceScroll;

    public MainWindow()
    {
        Title = "ЕВА — локальный AI-агент";
        ConfigureAgentTitleBar();
        BuildUi();
        _state.SelectWorkspace(Environment.CurrentDirectory);
        _projectCatalog = _projectCatalogService.Load();
        EnsureActiveProject(_state.WorkspacePath);
        WorkspacePathText.Text = $"Workspace: {_state.WorkspacePath}";
        _ = LoadModelConfigAsync();
        _ = RestoreWorkspaceAsync();
        _ = CheckForUpdatesAsync();
        _ = RefreshGitHubProfileAsync();
    }

    private void ConfigureAgentTitleBar()
    {
        var hwnd = WindowNative.GetWindowHandle(this);
        var windowId = Win32Interop.GetWindowIdFromWindow(hwnd);
        var appWindow = AppWindow.GetFromWindowId(windowId);
        appWindow.Title = Title;
        // Базовая композиция native-оболочки рассчитана на рабочую область 16:9.
        // Пользователь по-прежнему может свободно изменить размер окна после запуска.
        appWindow.Resize(new Windows.Graphics.SizeInt32(1440, 810));

        if (!AppWindowTitleBar.IsCustomizationSupported())
        {
            return;
        }

        var titleBar = appWindow.TitleBar;
        titleBar.BackgroundColor = Windows.UI.Color.FromArgb(255, 9, 11, 16);
        titleBar.ForegroundColor = Windows.UI.Color.FromArgb(255, 247, 244, 245);
        titleBar.InactiveBackgroundColor = Windows.UI.Color.FromArgb(255, 16, 20, 27);
        titleBar.InactiveForegroundColor = Windows.UI.Color.FromArgb(255, 143, 146, 157);
        titleBar.ButtonBackgroundColor = Windows.UI.Color.FromArgb(255, 9, 11, 16);
        titleBar.ButtonForegroundColor = Windows.UI.Color.FromArgb(255, 247, 244, 245);
        titleBar.ButtonHoverBackgroundColor = Windows.UI.Color.FromArgb(255, 227, 38, 79);
        titleBar.ButtonHoverForegroundColor = Windows.UI.Color.FromArgb(255, 255, 255, 255);
        titleBar.ButtonPressedBackgroundColor = Windows.UI.Color.FromArgb(255, 170, 24, 56);
        titleBar.ButtonPressedForegroundColor = Windows.UI.Color.FromArgb(255, 255, 255, 255);
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
        root.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(310) });

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
            ClearConversation();
            _activeChatId = null;
            _newChatRequested = true;
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
                Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
                Foreground = text,
            };
            button.Click += (_, _) => NavigateShellItem(item.Title);
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
        Grid.SetRow(navItems, 2);
        sidebar.Children.Add(navItems);
        var workspaceInfo = new StackPanel { Spacing = 5, Margin = new Thickness(0, 14, 0, 0) };
        workspaceInfo.Children.Add(new TextBlock { Text = "РАБОЧЕЕ ПРОСТРАНСТВО", FontSize = 11, Foreground = muted });
        workspaceInfo.Children.Add(new TextBlock { Text = "⌂  Текущий проект", Foreground = muted, Padding = new Thickness(4, 8, 4, 8) });
        var accountGrid = new Grid
        {
            Padding = new Thickness(8, 7, 6, 7),
        };
        accountGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        accountGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var accountBar = new Border
        {
            Background = raised,
            CornerRadius = new CornerRadius(10),
            Margin = new Thickness(0, 10, 0, 0),
            Child = accountGrid,
        };
        _githubProfileButton = new Button
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(0),
            BorderThickness = new Thickness(0),
        };
        var profileContent = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        profileContent.Children.Add(new Border
        {
            Width = 28,
            Height = 28,
            CornerRadius = new CornerRadius(14),
            Background = ThemeBrush("PurpleBrush", 167, 139, 250),
            Child = new TextBlock { Text = "⌁", FontSize = 18, Foreground = new SolidColorBrush(Microsoft.UI.Colors.White), HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center },
        });
        var profileText = new StackPanel { Spacing = 1, VerticalAlignment = VerticalAlignment.Center };
        _githubProfileText = new TextBlock { Text = "GitHub", FontSize = 12, Foreground = text, TextTrimming = TextTrimming.CharacterEllipsis, MaxWidth = 132 };
        _githubProfileStatus = new TextBlock { Text = "Проверяю вход…", FontSize = 10, Foreground = muted, TextTrimming = TextTrimming.CharacterEllipsis, MaxWidth = 132 };
        profileText.Children.Add(_githubProfileText);
        profileText.Children.Add(_githubProfileStatus);
        profileContent.Children.Add(profileText);
        _githubProfileButton.Content = profileContent;
        _githubProfileButton.Click += (_, _) => _ = HandleGitHubProfileClickAsync();
        accountGrid.Children.Add(_githubProfileButton);
        var settingsGear = new Button
        {
            Content = "⚙",
            FontSize = 18,
            Width = 32,
            Height = 32,
            Padding = new Thickness(0),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Foreground = muted,
            HorizontalContentAlignment = HorizontalAlignment.Center,
            VerticalContentAlignment = VerticalAlignment.Center,
        };
        ToolTipService.SetToolTip(settingsGear, "Настройки");
        settingsGear.Click += (_, _) => ShowSettingsView();
        Grid.SetColumn(settingsGear, 1);
        accountGrid.Children.Add(settingsGear);
        workspaceInfo.Children.Add(accountBar);
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
        _conversationPanel = new StackPanel { Spacing = 14 };
        _conversationScroll = new ScrollViewer
        {
            Content = _conversationPanel,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled,
            Padding = new Thickness(2, 4, 8, 8),
        };
        Grid.SetRow(_conversationScroll, 1);
        cardGrid.Children.Add(_conversationScroll);
        AddConversationMessage("Ева", "Привет, хозяин. Опиши задачу — я разберусь и покажу, что делаю.", false);
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
        // WinUI uses separate placeholder resources for the normal, focused,
        // hover and disabled states. Setting only the base key leaves the
        // hint almost black in the native TextBox template.
        var placeholder = ThemeBrush("TextBrush", 193, 196, 209);
        foreach (var resourceKey in new[]
        {
            "TextControlPlaceholderForeground",
            "TextControlPlaceholderForegroundFocused",
            "TextControlPlaceholderForegroundPointerOver",
            "TextControlPlaceholderForegroundDisabled",
        })
        {
            PromptBox.Resources[resourceKey] = placeholder;
        }
        // В многострочном TextBox обычный KeyDown может быть поглощён
        // обработчиком AcceptsReturn. PreviewKeyDown гарантирует отправку по Enter.
        PromptBox.PreviewKeyDown += PromptBox_PreviewKeyDown;
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
            Content = "Доступ: спрашивать",
            Foreground = new SolidColorBrush(Windows.UI.Color.FromArgb(255, 239, 133, 80)),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(4, 5, 6, 5),
        };
        ToolTipService.SetToolTip(accessButton, "Режим доступа инструментов агента");
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
        var contextIndicator = new Grid { Width = 42, Height = 42 };
        contextIndicator.Children.Add(new XamlEllipse
        {
            Width = 34,
            Height = 34,
            Stroke = ThemeBrush("BorderBrush", 48, 53, 72),
            StrokeThickness = 3,
        });
        _contextProgressArc = new XamlPath
        {
            Width = 42,
            Height = 42,
            Stroke = ThemeBrush("PurpleBrush", 167, 139, 250),
            StrokeThickness = 3,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        contextIndicator.Children.Add(_contextProgressArc);
        _contextPercentText = new TextBlock
        {
            Text = "—",
            FontSize = 10,
            Foreground = text,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        contextIndicator.Children.Add(_contextPercentText);
        _contextButton = new Button
        {
            Content = contextIndicator,
            Foreground = muted,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(0),
            IsEnabled = false,
        };
        ToolTipService.SetToolTip(_contextButton, "Заполнение контекста модели в токенах");
        _contextButton.Click += ContextButton_Click;
        Grid.SetColumn(_contextButton, 3);
        composerActions.Children.Add(_contextButton);
        Grid.SetColumn(_modelButton, 4);
        composerActions.Children.Add(_modelButton);
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
        var copyAllButton = new Button { Content = "Скопировать весь чат" };
        copyAllButton.Click += (_, _) => CopyWholeConversation();
        var traceButton = new Button { Content = "Открыть trace" };
        traceButton.Click += (_, _) => OpenTraceFolder();
        StopButton = new Button { Content = "Остановить", IsEnabled = false };
        StopButton.Click += StopButton_Click;
        footer.Children.Add(UpdateStatusText);
        footer.Children.Add(UpdateButton);
        footer.Children.Add(copyAllButton);
        footer.Children.Add(traceButton);
        footer.Children.Add(StopButton);
        Grid.SetRow(footer, 2);
        content.Children.Add(footer);
        Grid.SetColumn(content, 1);
        root.Children.Add(content);
        var traceBackground = ThemeBrush("SurfaceBrush", 25, 28, 39);
        var tracePanelBorder = new Border
        {
            Background = traceBackground,
            BorderBrush = ThemeBrush("BorderBrush", 48, 53, 72),
            BorderThickness = new Thickness(1, 0, 0, 0),
            Padding = new Thickness(16, 24, 14, 18),
        };
        var traceLayout = new Grid { RowSpacing = 10 };
        traceLayout.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        traceLayout.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        var traceHeader = new Grid { ColumnSpacing = 8 };
        traceHeader.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        traceHeader.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        traceHeader.Children.Add(new TextBlock
        {
            Text = "Трейс выполнения",
            FontSize = 18,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            Foreground = text,
        });
        var copyTraceButton = new Button
        {
            Content = "Копировать",
            FontSize = 11,
            Padding = new Thickness(6, 4, 6, 4),
        };
        ToolTipService.SetToolTip(copyTraceButton, "Скопировать trace");
        copyTraceButton.Click += (_, _) => CopyTrace();
        Grid.SetColumn(copyTraceButton, 1);
        traceHeader.Children.Add(copyTraceButton);
        traceLayout.Children.Add(traceHeader);
        _tracePanel = new StackPanel { Spacing = 8 };
        _traceScroll = new ScrollViewer
        {
            Content = _tracePanel,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled,
        };
        Grid.SetRow(_traceScroll, 1);
        traceLayout.Children.Add(_traceScroll);
        tracePanelBorder.Child = traceLayout;
        Grid.SetColumn(tracePanelBorder, 2);
        root.Children.Add(tracePanelBorder);
        _homeContent = content;
        _settingsView = BuildSettingsView();
        Grid.SetColumn(_settingsView, 1);
        _settingsView.Visibility = Visibility.Collapsed;
        root.Children.Add(_settingsView);
        _scheduledView = BuildScheduledView();
        Grid.SetColumn(_scheduledView, 1);
        _scheduledView.Visibility = Visibility.Collapsed;
        root.Children.Add(_scheduledView);
        _tasksView = BuildTasksView();
        Grid.SetColumn(_tasksView, 1);
        _tasksView.Visibility = Visibility.Collapsed;
        root.Children.Add(_tasksView);
        _filesView = BuildFilesView();
        Grid.SetColumn(_filesView, 1);
        _filesView.Visibility = Visibility.Collapsed;
        root.Children.Add(_filesView);
        _gitView = BuildGitView();
        Grid.SetColumn(_gitView, 1);
        _gitView.Visibility = Visibility.Collapsed;
        root.Children.Add(_gitView);
        _terminalView = BuildTerminalView();
        Grid.SetColumn(_terminalView, 1);
        _terminalView.Visibility = Visibility.Collapsed;
        root.Children.Add(_terminalView);
        _pluginsView = BuildPluginsView();
        Grid.SetColumn(_pluginsView, 1);
        _pluginsView.Visibility = Visibility.Collapsed;
        root.Children.Add(_pluginsView);
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
        _modelModeBox = new ComboBox
        {
            Header = "Режим каталога",
            ItemsSource = new[] { "Бесплатные", "Платные" },
            SelectedIndex = string.Equals(providerSettings.CatalogMode, "paid", StringComparison.OrdinalIgnoreCase) ? 1 : 0,
        };
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
        providerForm.Children.Add(new TextBlock { Text = "Ключ хранится в Credential Manager текущего пользователя Windows; в настройках остаётся только ссылка.", FontSize = 11, Foreground = muted, TextWrapping = TextWrapping.Wrap });
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
        var doctor = new Button { Content = "Core Doctor", Margin = new Thickness(0, 8, 0, 0) };
        doctor.Click += async (_, _) => await ShowDoctorReportAsync();
        runtime.Children.Add(doctor);
        var capabilitySelection = new Button { Content = "Capability Selection", Margin = new Thickness(0, 8, 0, 0) };
        capabilitySelection.Click += async (_, _) => await ShowCapabilitySelectionAsync();
        runtime.Children.Add(capabilitySelection);
        var backupPath = new TextBox
        {
            Header = "Файл backup/restore",
            Text = Path.Combine(dataDirectory, "evohime-backup.evohime"),
            TextWrapping = TextWrapping.NoWrap,
        };
        var backupStatus = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Foreground = muted,
        };
        var backupActions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var createBackup = new Button { Content = "Создать backup" };
        createBackup.Click += async (_, _) => await CreateDatabaseBackupAsync(backupPath.Text, backupStatus);
        var restoreBackup = new Button { Content = "Восстановить…" };
        restoreBackup.Click += async (_, _) => await RestoreDatabaseBackupAsync(backupPath.Text, backupStatus);
        backupActions.Children.Add(createBackup);
        backupActions.Children.Add(restoreBackup);
        runtime.Children.Add(backupPath);
        runtime.Children.Add(backupActions);
        runtime.Children.Add(backupStatus);
        sections.Children.Add(CreateSettingsSection("Состояние и диагностика", "Служебная информация приложения.", runtime, raised));

        var scroll = new ScrollViewer { Content = sections, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        Grid.SetRow(scroll, 1);
        view.Children.Add(scroll);
        Grid.SetRow(view, 0);
        return view;
    }

    private async Task CreateDatabaseBackupAsync(string path, TextBlock status)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            status.Text = "Укажите путь к backup-файлу.";
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await ConnectToCoreWithRetryAsync(CancellationToken.None);
                await _ipc.RequestDatabaseBackupAsync(path.Trim(), CancellationToken.None);
                while (true)
                {
                    var response = await _ipc.ReadEventAsync(CancellationToken.None);
                    if (response.EventType == "storage.progress")
                    {
                        status.Text = FormatStorageProgress(response.Payload);
                        continue;
                    }

                    if (response.EventType == "storage.backup.created")
                    {
                        status.Text = "Backup создан. В manifest не входят provider secrets.";
                        break;
                    }
                }
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            status.Text = $"Backup не создан: {error.Message}";
        }
    }

    private async Task RestoreDatabaseBackupAsync(string path, TextBlock status)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            status.Text = "Укажите путь к backup-файлу.";
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await ConnectToCoreWithRetryAsync(CancellationToken.None);
                await _ipc.PrepareDatabaseRestoreAsync(path.Trim(), CancellationToken.None);
                var previewResponse = await _ipc.ReadEventAsync(CancellationToken.None);
                if (previewResponse.EventType != "storage.restore.preview")
                {
                    throw new InvalidOperationException("Core не вернул preview restore.");
                }

                using var previewJson = JsonDocument.Parse(previewResponse.Payload);
                var root = previewJson.RootElement;
                var approvalId = root.GetProperty("approval_id").GetString();
                var preview = root.GetProperty("preview");
                var summary = $"Файл: {preview.GetProperty("source_name").GetString()}\n" +
                              $"Формат: {preview.GetProperty("format_version").GetUInt32()}\n" +
                              $"Schema: {preview.GetProperty("schema_version").GetUInt32()}\n" +
                              $"Размер: {preview.GetProperty("database_size_bytes").GetUInt64()} байт\n" +
                              $"Объекты: {preview.GetProperty("objects").GetArrayLength()}\n\n" +
                              "Будет создан safety backup текущей базы. Provider secrets не восстанавливаются.";
                var dialog = new ContentDialog
                {
                    Title = "Подтвердить восстановление базы",
                    Content = new TextBlock { Text = summary, TextWrapping = TextWrapping.Wrap },
                    PrimaryButtonText = "Одобрить restore",
                    CloseButtonText = "Отмена",
                    XamlRoot = ((FrameworkElement)Content).XamlRoot,
                };
                if (await dialog.ShowAsync() != ContentDialogResult.Primary)
                {
                    status.Text = "Restore отменён до atomic swap.";
                    return;
                }

                await _ipc.RestoreDatabaseAsync(path.Trim(), approvalId ?? string.Empty, CancellationToken.None);
                while (true)
                {
                    var response = await _ipc.ReadEventAsync(CancellationToken.None);
                    if (response.EventType == "storage.progress")
                    {
                        status.Text = FormatStorageProgress(response.Payload);
                        continue;
                    }

                    if (response.EventType == "storage.restore.completed")
                    {
                        status.Text = "Restore завершён; рабочая база переоткрыта и audit записан.";
                        break;
                    }
                }
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            status.Text = $"Restore не выполнен: {error.Message}";
        }
    }

    private static string FormatStorageProgress(byte[] payload)
    {
        try
        {
            using var json = JsonDocument.Parse(payload);
            var root = json.RootElement;
            var phase = root.GetProperty("phase").GetString() ?? "storage";
            var message = root.GetProperty("message").GetString() ?? string.Empty;
            var completed = root.GetProperty("completed").GetUInt64();
            var total = root.TryGetProperty("total", out var totalValue) && totalValue.ValueKind != JsonValueKind.Null
                ? $"{completed}/{totalValue.GetUInt64()}"
                : completed.ToString();
            return $"{phase}: {total} — {message}";
        }
        catch (JsonException)
        {
            return "storage: выполняется";
        }
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
        HideShellViews();
        if (_settingsView is not null)
        {
            _settingsView.Visibility = Visibility.Visible;
            _ = LoadModelCatalogAsync(_modelModeBox?.SelectedIndex == 1 ? "paid" : "free");
        }
    }

    private void ShowHomeView()
    {
        HideShellViews();
        if (_homeContent is not null)
        {
            _homeContent.Visibility = Visibility.Visible;
        }
    }

    private void NavigateShellItem(string title)
    {
        switch (title)
        {
            case "Задачи":
                ShowTasksView();
                break;
            case "Файлы":
                ShowFilesView();
                break;
            case "Git":
                ShowGitView();
                break;
            case "Терминал":
                ShowTerminalView();
                break;
            case "Запланировано":
                ShowScheduledView();
                break;
            case "Плагины":
                ShowPluginsView();
                break;
            case "Настройки":
                ShowSettingsView();
                break;
            case "Новый чат":
                ShowHomeView();
                break;
        }
    }

    private void HideShellViews()
    {
        if (_homeContent is not null) _homeContent.Visibility = Visibility.Collapsed;
        if (_settingsView is not null) _settingsView.Visibility = Visibility.Collapsed;
        if (_scheduledView is not null) _scheduledView.Visibility = Visibility.Collapsed;
        if (_tasksView is not null) _tasksView.Visibility = Visibility.Collapsed;
        if (_filesView is not null) _filesView.Visibility = Visibility.Collapsed;
        if (_gitView is not null) _gitView.Visibility = Visibility.Collapsed;
        if (_terminalView is not null) _terminalView.Visibility = Visibility.Collapsed;
        if (_pluginsView is not null) _pluginsView.Visibility = Visibility.Collapsed;
    }

    private void ShowScheduledView()
    {
        HideShellViews();
        if (_scheduledView is not null) _scheduledView.Visibility = Visibility.Visible;
    }

    private void ShowTasksView()
    {
        HideShellViews();
        if (_tasksView is not null)
        {
            _tasksView.Visibility = Visibility.Visible;
            _ = LoadTaskWorkspaceAsync();
        }
    }

    private void ShowFilesView()
    {
        HideShellViews();
        if (_filesView is not null)
        {
            _filesView.Visibility = Visibility.Visible;
            _ = LoadFilesAsync(_filesRelativePath);
        }
    }

    private void ShowGitView()
    {
        HideShellViews();
        if (_gitView is not null)
        {
            _gitView.Visibility = Visibility.Visible;
            _ = LoadGitAsync();
        }
    }

    private void ShowTerminalView()
    {
        HideShellViews();
        if (_terminalView is not null)
        {
            _terminalView.Visibility = Visibility.Visible;
        }
    }

    private Grid BuildTerminalView()
    {
        var view = BuildShellPage("Терминал", "Команды выполняются Core в sandbox workspace и ограничены timeout/output.");
        var content = new Grid { RowSpacing = 10 };
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });

        var form = new StackPanel { Spacing = 8 };
        var program = new TextBox { Header = "Программа", PlaceholderText = "например: git" };
        var args = new TextBox { Header = "Аргументы", PlaceholderText = "например: status --short" };
        var cwd = new TextBox { Header = "Относительный cwd", PlaceholderText = "." };
        var timeout = new NumberBox { Header = "Timeout (мс)", Value = 30000, Minimum = 100, Maximum = 30000, SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Compact };
        var run = new Button { Content = "Выполнить через Core", HorizontalAlignment = HorizontalAlignment.Left };
        var status = new TextBlock { Text = "Команда ещё не запускалась.", TextWrapping = TextWrapping.Wrap, Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157) };
        var output = CreateReadOnlyCodeBox("Вывод команды появится здесь");
        run.Click += async (_, _) => await ExecuteTerminalAsync(program, args, cwd, timeout, status, output);
        form.Children.Add(program);
        form.Children.Add(args);
        form.Children.Add(cwd);
        form.Children.Add(timeout);
        form.Children.Add(run);
        Grid.SetRow(form, 0);
        content.Children.Add(form);
        Grid.SetRow(status, 1);
        content.Children.Add(status);
        Grid.SetRow(output, 2);
        content.Children.Add(new Border { Child = output, Background = ThemeBrush("SurfaceRaisedBrush", 23, 28, 37), CornerRadius = new CornerRadius(10) });
        Grid.SetRow(content, 1);
        view.Children.Add(content);
        return view;
    }

    private async Task ExecuteTerminalAsync(TextBox program, TextBox args, TextBox cwd, NumberBox timeout, TextBlock status, TextBox output)
    {
        var executable = program.Text.Trim();
        if (string.IsNullOrWhiteSpace(executable))
        {
            status.Text = "Укажите программу.";
            return;
        }
        var taskId = Guid.NewGuid().ToString();
        var workspacePath = _state.WorkspacePath ?? Environment.CurrentDirectory;
        var argumentList = args.Text.Split(' ', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        var workingDirectory = cwd.Text.Trim();
        var timeoutMs = (uint)Math.Clamp(timeout.Value is double value ? value : 30000, 100, 30000);
        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                using var requestTimeout = new CancellationTokenSource(TimeSpan.FromSeconds(45));
                if (!_ipc.IsConnected)
                {
                    await ConnectToCoreWithRetryAsync(requestTimeout.Token);
                }
                await _ipc.ExecuteTerminalAsync(taskId, workspacePath, executable, argumentList, workingDirectory, timeoutMs, string.Empty, requestTimeout.Token);
                var response = await _ipc.ReadEventAsync(requestTimeout.Token);
                if (response.EventType == "approval.required")
                {
                    using var approvalJson = JsonDocument.Parse(response.Payload);
                    var approvalId = approvalJson.RootElement.GetProperty("approval_id").GetString() ?? string.Empty;
                    var scope = approvalJson.RootElement.GetProperty("scope").GetString() ?? executable;
                    var dialog = new ContentDialog
                    {
                        Title = "Разрешить команду?",
                        Content = $"Core запросил ShellExecute для {scope}.\nКоманда: {executable} {string.Join(' ', argumentList)}",
                        PrimaryButtonText = "Разрешить",
                        CloseButtonText = "Отмена",
                        XamlRoot = Content.XamlRoot,
                    };
                    var decision = await dialog.ShowAsync();
                    await _ipc.ResolveApprovalAsync(approvalId, decision == ContentDialogResult.Primary, requestTimeout.Token);
                    if (decision != ContentDialogResult.Primary)
                    {
                        status.Text = "Команда отменена до запуска.";
                        output.Text = string.Empty;
                        return;
                    }
                    await _ipc.ExecuteTerminalAsync(taskId, workspacePath, executable, argumentList, workingDirectory, timeoutMs, approvalId, requestTimeout.Token);
                    response = await _ipc.ReadEventAsync(requestTimeout.Token);
                }
                if (response.EventType != "terminal.result")
                {
                    throw new InvalidOperationException($"Core вернул {response.EventType} вместо terminal.result.");
                }
                using var resultJson = JsonDocument.Parse(response.Payload);
                var root = resultJson.RootElement;
                var ok = root.GetProperty("ok").GetBoolean();
                output.Text = root.TryGetProperty("output", out var outputValue) ? outputValue.GetString() ?? string.Empty : root.GetProperty("error").GetString() ?? string.Empty;
                status.Text = ok
                    ? (root.TryGetProperty("truncated", out var truncated) && truncated.GetBoolean() ? "Готово; вывод усечён bounded-лимитом Core." : "Готово через Core.")
                    : "Команда не выполнена: Core отклонил или не смог запустить её.";
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error)
        {
            status.Text = $"Ошибка Terminal: {error.Message}";
            output.Text = string.Empty;
        }
    }

    private Grid BuildGitView()
    {
        var view = BuildShellPage("Git", "Read-only статус и diff через Core IPC.");
        var content = new Grid { RowSpacing = 12 };
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });

        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        _gitPathBox = new TextBox { PlaceholderText = "Файл для diff (необязательно)", Width = 260 };
        actions.Children.Add(_gitPathBox);
        var refresh = new Button { Content = "Обновить" };
        refresh.Click += (_, _) => _ = LoadGitAsync();
        actions.Children.Add(refresh);
        Grid.SetRow(actions, 0);
        content.Children.Add(actions);

        _gitStatusText = new TextBlock
        {
            Text = "Загрузка Git…",
            Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157),
            TextWrapping = TextWrapping.Wrap,
        };
        Grid.SetRow(_gitStatusText, 1);
        content.Children.Add(_gitStatusText);

        var panes = new Grid { ColumnSpacing = 12 };
        panes.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(0.8, GridUnitType.Star) });
        panes.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1.2, GridUnitType.Star) });
        _gitStatusPreview = CreateReadOnlyCodeBox("Статус репозитория");
        _gitDiffPreview = CreateReadOnlyCodeBox("Выберите файл или оставьте поле пустым");
        panes.Children.Add(new Border { Child = _gitStatusPreview, Background = ThemeBrush("SurfaceRaisedBrush", 23, 28, 37), CornerRadius = new CornerRadius(10) });
        var diffScroll = new ScrollViewer { Content = _gitDiffPreview, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, HorizontalScrollBarVisibility = ScrollBarVisibility.Auto };
        Grid.SetColumn(diffScroll, 1);
        panes.Children.Add(new Border { Child = diffScroll, Background = ThemeBrush("SurfaceRaisedBrush", 23, 28, 37), CornerRadius = new CornerRadius(10) });
        Grid.SetRow(panes, 2);
        content.Children.Add(panes);
        Grid.SetRow(content, 1);
        view.Children.Add(content);
        return view;
    }

    private TextBox CreateReadOnlyCodeBox(string placeholder) => new()
    {
        IsReadOnly = true,
        TextWrapping = TextWrapping.Wrap,
        AcceptsReturn = true,
        FontFamily = new Microsoft.UI.Xaml.Media.FontFamily("Cascadia Mono"),
        Foreground = ThemeBrush("TextBrush", 247, 244, 245),
        Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
        Padding = new Thickness(14),
        PlaceholderText = placeholder,
    };

    private async Task LoadGitAsync()
    {
        if (_gitStatusPreview is null || _gitDiffPreview is null || _gitStatusText is null)
        {
            return;
        }
        var workspacePath = _state.WorkspacePath ?? Environment.CurrentDirectory;
        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(15));
                if (!_ipc.IsConnected)
                {
                    await ConnectToCoreWithRetryAsync(timeout.Token);
                }
                await _ipc.RequestGitStatusAsync(workspacePath, timeout.Token);
                var status = await _ipc.ReadEventAsync(timeout.Token);
                if (status.EventType != "git.status")
                {
                    throw new InvalidOperationException($"Core вернул {status.EventType} вместо git.status.");
                }
                await _ipc.RequestGitDiffAsync(workspacePath, _gitPathBox?.Text.Trim() ?? string.Empty, timeout.Token);
                var diff = await _ipc.ReadEventAsync(timeout.Token);
                if (diff.EventType != "git.diff")
                {
                    throw new InvalidOperationException($"Core вернул {diff.EventType} вместо git.diff.");
                }
                using var statusJson = JsonDocument.Parse(status.Payload);
                using var diffJson = JsonDocument.Parse(diff.Payload);
                _gitStatusPreview.Text = statusJson.RootElement.GetProperty("output").GetString() ?? string.Empty;
                _gitDiffPreview.Text = diffJson.RootElement.GetProperty("output").GetString() ?? string.Empty;
                var truncated = diffJson.RootElement.TryGetProperty("truncated", out var truncatedValue) && truncatedValue.GetBoolean();
                _gitStatusText.Text = truncated
                    ? "Git загружен; diff усечён bounded-лимитом Core."
                    : "Git загружен через Core; операции записи и push здесь недоступны.";
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error)
        {
            _gitStatusText.Text = $"Не удалось получить Git через Core: {error.Message}";
            _gitStatusPreview.Text = string.Empty;
            _gitDiffPreview.Text = string.Empty;
        }
    }

    private Grid BuildFilesView()
    {
        var view = BuildShellPage("Файлы", "Безопасный просмотр текущего workspace через Core.");
        var content = new Grid { ColumnSpacing = 14 };
        content.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(300) });
        content.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });

        var browser = new Grid { RowSpacing = 8 };
        browser.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        browser.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        var toolbar = new Grid { ColumnSpacing = 8 };
        toolbar.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        toolbar.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        toolbar.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        _filesPathText = new TextBlock { Text = ".", Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157), TextTrimming = TextTrimming.CharacterEllipsis };
        toolbar.Children.Add(_filesPathText);
        var refresh = new Button { Content = "Обновить" };
        refresh.Click += (_, _) => _ = LoadFilesAsync(_filesRelativePath);
        Grid.SetColumn(refresh, 1);
        toolbar.Children.Add(refresh);
        var edit = new Button { Content = "Редактировать через Build" };
        edit.Click += (_, _) => _ = OpenSelectedFileBuildAsync();
        Grid.SetColumn(edit, 2);
        toolbar.Children.Add(edit);
        browser.Children.Add(toolbar);
        _filesList = new StackPanel { Spacing = 4 };
        var filesScroll = new ScrollViewer { Content = _filesList, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        Grid.SetRow(filesScroll, 1);
        browser.Children.Add(filesScroll);
        Grid.SetColumn(browser, 0);
        content.Children.Add(browser);

        _filePreview = new TextBox
        {
            IsReadOnly = true,
            TextWrapping = TextWrapping.Wrap,
            AcceptsReturn = true,
            FontFamily = new Microsoft.UI.Xaml.Media.FontFamily("Cascadia Mono"),
            Foreground = ThemeBrush("TextBrush", 247, 244, 245),
            Background = ThemeBrush("SurfaceRaisedBrush", 23, 28, 37),
            Padding = new Thickness(14),
            PlaceholderText = "Выберите текстовый файл",
        };
        var previewScroll = new ScrollViewer
        {
            Content = _filePreview,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
        };
        Grid.SetColumn(previewScroll, 1);
        content.Children.Add(previewScroll);
        Grid.SetRow(content, 1);
        view.Children.Add(content);
        return view;
    }

    private async Task LoadFilesAsync(string relativePath)
    {
        if (_filesList is null || _filesPathText is null)
        {
            return;
        }

        var workspacePath = _state.WorkspacePath ?? Environment.CurrentDirectory;
        _filesRelativePath = string.IsNullOrWhiteSpace(relativePath) ? "." : relativePath.Replace('\\', '/');
        _filesPathText.Text = _filesRelativePath;
        _filesList.Children.Clear();
        _filesList.Children.Add(new TextBlock { Text = "Загружаю список…", Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157) });
        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(15));
                if (!_ipc.IsConnected)
                {
                    await ConnectToCoreWithRetryAsync(timeout.Token);
                }
                await _ipc.RequestWorkspaceListAsync(workspacePath, _filesRelativePath, timeout.Token);
                var response = await _ipc.ReadEventAsync(timeout.Token);
                if (response.EventType != "workspace.list")
                {
                    throw new InvalidOperationException($"Core вернул {response.EventType} вместо workspace.list.");
                }

                var listing = JsonSerializer.Deserialize<WorkspaceListingDto>(response.Payload)
                    ?? throw new InvalidOperationException("Core вернул пустой список workspace.");
                _filesList.Children.Clear();
                if (_filesRelativePath != ".")
                {
                    var back = new Button { Content = "←  Вверх", HorizontalAlignment = HorizontalAlignment.Stretch, HorizontalContentAlignment = HorizontalAlignment.Left };
                    back.Click += (_, _) => _ = LoadFilesAsync(ParentWorkspacePath(_filesRelativePath));
                    _filesList.Children.Add(back);
                }
                foreach (var entry in listing.Entries)
                {
                    var button = new Button
                    {
                        Content = entry.Directory ? $"▸  {entry.Name}" : $"▱  {entry.Name}",
                        Tag = entry,
                        HorizontalAlignment = HorizontalAlignment.Stretch,
                        HorizontalContentAlignment = HorizontalAlignment.Left,
                    };
                    button.Click += (_, _) =>
                    {
                        if (button.Tag is WorkspaceEntryDto selected)
                        {
                            if (selected.Directory)
                            {
                                _ = LoadFilesAsync(selected.RelativePath);
                            }
                            else
                            {
                                _ = LoadFileAsync(selected.RelativePath);
                            }
                        }
                    };
                    _filesList.Children.Add(button);
                }
                if (listing.Entries.Count == 0)
                {
                    _filesList.Children.Add(new TextBlock { Text = "Каталог пуст.", Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157) });
                }
                if (listing.Truncated)
                {
                    _filesList.Children.Add(new TextBlock { Text = "Список ограничен лимитом Core.", Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157), TextWrapping = TextWrapping.Wrap });
                }
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error)
        {
            _filesList.Children.Clear();
            _filesList.Children.Add(new TextBlock { Text = $"Не удалось получить список: {error.Message}", Foreground = ThemeBrush("ErrorBrush", 255, 120, 130), TextWrapping = TextWrapping.Wrap });
        }
    }

    private async Task LoadFileAsync(string relativePath)
    {
        if (_filePreview is null)
        {
            return;
        }

        try
        {
            _filesSelectedPath = relativePath.Replace('\\', '/');
            await _ipcRequestGate.WaitAsync();
            try
            {
                using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(15));
                if (!_ipc.IsConnected)
                {
                    await ConnectToCoreWithRetryAsync(timeout.Token);
                }
                await _ipc.RequestWorkspaceFileAsync(_state.WorkspacePath ?? Environment.CurrentDirectory, relativePath, timeout.Token);
                var response = await _ipc.ReadEventAsync(timeout.Token);
                if (response.EventType != "workspace.file")
                {
                    throw new InvalidOperationException($"Core вернул {response.EventType} вместо workspace.file.");
                }
                using var json = JsonDocument.Parse(response.Payload);
                _filePreview.Text = json.RootElement.GetProperty("content").GetString() ?? string.Empty;
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error)
        {
            _filePreview.Text = $"Не удалось прочитать файл: {error.Message}";
        }
    }

    private async Task OpenSelectedFileBuildAsync()
    {
        if (string.IsNullOrWhiteSpace(_filesSelectedPath) || _filePreview is null)
        {
            if (_filePreview is not null)
            {
                _filePreview.Text = "Сначала выберите файл для редактирования";
            }
            return;
        }

        var task = _lastTaskGraph?.Tasks
            .FirstOrDefault(item => string.Equals(item.Status, "ready", StringComparison.OrdinalIgnoreCase))
            ?? _lastTaskGraph?.Tasks.FirstOrDefault();
        if (task is null)
        {
            _filePreview.Text = "Для bounded Build сначала создайте или загрузите задачу на странице «Задачи».";
            return;
        }

        await PrepareBuildDialogAsync(task, _filesSelectedPath, _filePreview.Text);
    }

    private static string ParentWorkspacePath(string path)
    {
        var normalized = path.Replace('\\', '/').TrimEnd('/');
        var separator = normalized.LastIndexOf('/');
        return separator <= 0 ? "." : normalized[..separator];
    }

    private sealed record WorkspaceListingDto(
        [property: JsonPropertyName("path")] string Path,
        [property: JsonPropertyName("entries")] List<WorkspaceEntryDto> Entries,
        [property: JsonPropertyName("truncated")] bool Truncated);

    private sealed record WorkspaceEntryDto(
        [property: JsonPropertyName("name")] string Name,
        [property: JsonPropertyName("relative_path")] string RelativePath,
        [property: JsonPropertyName("directory")] bool Directory,
        [property: JsonPropertyName("bytes")] int? Bytes);

    private void ShowPluginsView()
    {
        HideShellViews();
        if (_pluginsView is not null) _pluginsView.Visibility = Visibility.Visible;
    }

    private Grid BuildScheduledView()
    {
        var view = BuildShellPage(
            "Запланировано",
            "Управление задачами, которые нужно выполнить позже.");
        var content = new StackPanel { Spacing = 14 };
        content.Children.Add(CreateSettingsSection(
            "Планировщик",
            "Здесь будут отображаться будущие задачи Евы.",
            new StackPanel
            {
                Spacing = 10,
                Children =
                {
                    new TextBlock { Text = "Запланированных задач пока нет.", Foreground = ThemeBrush("TextBrush", 247, 244, 245), FontSize = 15 },
                    new TextBlock { Text = "Создайте задачу в чате, когда понадобится выполнить её в выбранное время.", Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157), TextWrapping = TextWrapping.Wrap },
                    CreateNavigationButton("Создать задачу в чате", ShowHomeView),
                },
            },
            ThemeBrush("SurfaceRaisedBrush", 23, 28, 37)));
        Grid.SetRow(content, 1);
        view.Children.Add(content);
        return view;
    }

    private Grid BuildTasksView()
    {
        var view = BuildShellPage("Задачи", "Граф задач проекта, готовые и заблокированные шаги.");
        var content = new Grid { RowSpacing = 12 };
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });

        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var refresh = new Button { Content = "Обновить" };
        refresh.Click += (_, _) => _ = LoadTaskWorkspaceAsync();
        actions.Children.Add(refresh);
        var addTask = new Button { Content = "Добавить задачу" };
        addTask.Click += async (_, _) => await CreateTaskDialogAsync(null);
        actions.Children.Add(addTask);
        var addEdge = new Button { Content = "Добавить зависимость" };
        addEdge.Click += async (_, _) => await AddTaskEdgeDialogAsync();
        actions.Children.Add(addEdge);
        var nextReady = new Button { Content = "Следующая задача" };
        nextReady.Click += (_, _) => _ = RequestNextReadyTaskAsync();
        actions.Children.Add(nextReady);
        var import = new Button { Content = "Импортировать PRD" };
        import.Click += async (_, _) => await ImportPrdFromFileAsync();
        actions.Children.Add(import);
        Grid.SetRow(actions, 0);
        content.Children.Add(actions);

        _taskWorkspaceStatus = new TextBlock
        {
            Text = "Граф ещё не загружен.",
            Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157),
            TextWrapping = TextWrapping.Wrap,
        };
        Grid.SetRow(_taskWorkspaceStatus, 1);
        content.Children.Add(_taskWorkspaceStatus);

        _taskGraphCanvas = new Microsoft.UI.Xaml.Controls.Canvas
        {
            Height = 190,
            Background = ThemeBrush("SurfaceRaisedBrush", 23, 28, 37),
        };
        Grid.SetRow(_taskGraphCanvas, 2);
        content.Children.Add(_taskGraphCanvas);

        _taskList = new StackPanel { Spacing = 10 };
        var scroll = new ScrollViewer
        {
            Content = _taskList,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
        };
        Grid.SetRow(scroll, 3);
        content.Children.Add(scroll);
        Grid.SetRow(content, 1);
        view.Children.Add(content);
        return view;
    }

    private async Task LoadTaskWorkspaceAsync()
    {
        if (_taskList is null || _taskWorkspaceStatus is null)
        {
            return;
        }

        var project = ActiveProject();
        if (project is null)
        {
            _taskWorkspaceStatus.Text = "Сначала выберите проект.";
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await EnsureCoreProjectAsync(project);
                await _ipc.RequestTaskGraphAsync(project.Id, CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "task.graph")
                {
                    throw new InvalidOperationException("Core не вернул граф задач.");
                }

                var graph = JsonSerializer.Deserialize<TaskGraphDto>(response.Payload);
                if (graph is null)
                {
                    throw new JsonException("Пустой граф задач.");
                }

                _taskList.Children.Clear();
                _lastTaskGraph = graph;
                RenderTaskGraph(graph);
                foreach (var task in graph.Tasks.OrderByDescending(task => task.Priority).ThenBy(task => task.Id, StringComparer.Ordinal))
                {
                    _taskList.Children.Add(BuildTaskCard(task, graph.Edges));
                }
                var graphText = string.Join(", ", graph.Edges.Select(edge => $"{edge.FromTaskId} → {edge.ToTaskId}"));
                _taskWorkspaceStatus.Text = graph.Tasks.Count == 0
                    ? "В проекте пока нет задач. Импортируйте PRD или создайте задачу через Core."
                    : graph.Edges.Count == 0
                        ? $"Задач: {graph.Tasks.Count} · Связей нет"
                        : $"Задач: {graph.Tasks.Count} · Связей: {graph.Edges.Count} · Граф: {graphText}";
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Не удалось загрузить task workspace: {error.Message}";
        }
    }

    private Border BuildTaskCard(TaskDto task, IReadOnlyList<TaskEdgeDto> edges)
    {
        var text = ThemeBrush("TextBrush", 247, 244, 245);
        var muted = ThemeBrush("MutedTextBrush", 143, 146, 157);
        var dependencies = edges.Count(edge => edge.FromTaskId == task.Id);
        var details = new StackPanel { Spacing = 5 };
        details.Children.Add(new TextBlock
        {
            Text = task.Title,
            FontSize = 16,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            Foreground = text,
        });
        details.Children.Add(new TextBlock
        {
            Text = $"{task.Status} · сложность {task.Complexity ?? "не указана"} · приоритет {task.Priority} · зависимостей: {dependencies}",
            Foreground = muted,
            FontSize = 12,
        });
        if (!string.IsNullOrWhiteSpace(task.ParentId))
        {
            details.Children.Add(new TextBlock
            {
                Text = $"Подзадача родителя: {task.ParentId}",
                Foreground = muted,
                FontSize = 11,
            });
        }
        if (!string.IsNullOrWhiteSpace(task.Description))
        {
            details.Children.Add(new TextBlock { Text = task.Description, Foreground = muted, TextWrapping = TextWrapping.Wrap });
        }
        if (!string.IsNullOrWhiteSpace(task.AcceptanceCriteria))
        {
            details.Children.Add(new TextBlock { Text = $"Критерии: {task.AcceptanceCriteria}", Foreground = muted, TextWrapping = TextWrapping.Wrap, FontSize = 12 });
        }
        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6, Margin = new Thickness(0, 5, 0, 0) };
        AddTaskStatusButton(actions, "Готова", task, "ready");
        AddTaskStatusButton(actions, "В работу", task, "in_progress");
        AddTaskControlButton(actions, "Запустить", task, StartTaskControlAsync);
        if (task.Status == "in_progress")
        {
            AddTaskControlButton(actions, "Остановить", task, StopTaskControlAsync);
        }
        AddTaskStatusButton(actions, "Выполнена", task, "done");
        AddTaskStatusButton(actions, "Отложить", task, "backlog");
        if (task.Status == "done")
        {
            AddTaskStatusButton(actions, "Повторить", task, "ready");
        }
        var history = new Button { Content = "История", Padding = new Thickness(8, 3, 8, 3), FontSize = 11 };
        history.Click += async (_, _) => await RequestTaskHistoryAsync(task);
        actions.Children.Add(history);
        var rollback = new Button { Content = "Rollback", Padding = new Thickness(8, 3, 8, 3), FontSize = 11 };
        rollback.Click += async (_, _) => await RestoreLatestSnapshotAsync(task);
        actions.Children.Add(rollback);
        var context = new Button { Content = "Контекст", Padding = new Thickness(8, 3, 8, 3), FontSize = 11 };
        context.Click += async (_, _) => await RequestTaskContextAsync(task);
        actions.Children.Add(context);
        var planSpec = new Button { Content = "Plan/Spec", Padding = new Thickness(8, 3, 8, 3), FontSize = 11 };
        planSpec.Click += async (_, _) => await RequestTaskPlanSpecAsync(task);
        actions.Children.Add(planSpec);
        var build = new Button { Content = "Build preview", Padding = new Thickness(8, 3, 8, 3), FontSize = 11 };
        build.Click += async (_, _) => await PrepareBuildDialogAsync(task);
        actions.Children.Add(build);
        var policy = new Button { Content = "Policy", Padding = new Thickness(8, 3, 8, 3), FontSize = 11 };
        policy.Click += async (_, _) => await EditBuildPolicyAsync();
        actions.Children.Add(policy);
        var subtask = new Button { Content = "Подзадача", Padding = new Thickness(8, 3, 8, 3), FontSize = 11 };
        subtask.Click += async (_, _) => await CreateTaskDialogAsync(task.Id);
        actions.Children.Add(subtask);
        details.Children.Add(actions);
        return new Border
        {
            Background = ThemeBrush("SurfaceRaisedBrush", 23, 28, 37),
            BorderBrush = ThemeBrush("BorderBrush", 68, 32, 43),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(10),
            Padding = new Thickness(14),
            Child = details,
        };
    }

    private void RenderTaskGraph(TaskGraphDto graph)
    {
        if (_taskGraphCanvas is null)
        {
            return;
        }

        _taskGraphCanvas.Children.Clear();
        var positions = new Dictionary<string, (double X, double Y)>();
        const double nodeWidth = 170;
        const double nodeHeight = 52;
        const double horizontalGap = 28;
        const double verticalGap = 18;
        const int columns = 3;
        for (var index = 0; index < graph.Tasks.Count; index++)
        {
            var task = graph.Tasks[index];
            var column = index % columns;
            var row = index / columns;
            positions[task.Id] = (column * (nodeWidth + horizontalGap) + 8, row * (nodeHeight + verticalGap) + 8);
        }

        _taskGraphCanvas.Height = Math.Max(190, Math.Ceiling(graph.Tasks.Count / (double)columns) * (nodeHeight + verticalGap) + 16);
        foreach (var edge in graph.Edges)
        {
            if (!positions.TryGetValue(edge.FromTaskId, out var from) || !positions.TryGetValue(edge.ToTaskId, out var to))
            {
                continue;
            }
            var line = new Microsoft.UI.Xaml.Shapes.Line
            {
                X1 = from.X + nodeWidth,
                Y1 = from.Y + nodeHeight / 2,
                X2 = to.X,
                Y2 = to.Y + nodeHeight / 2,
                Stroke = ThemeBrush("PurpleBrush", 167, 139, 250),
                StrokeThickness = 2,
            };
            _taskGraphCanvas.Children.Add(line);
        }

        foreach (var task in graph.Tasks)
        {
            var position = positions[task.Id];
            var node = new Border
            {
                Width = nodeWidth,
                Height = nodeHeight,
                CornerRadius = new CornerRadius(8),
                Padding = new Thickness(8),
                Background = task.Status switch
                {
                    "done" => ThemeBrush("TealBrush", 40, 95, 91),
                    "in_progress" => ThemeBrush("PurpleBrush", 80, 62, 128),
                    "ready" => ThemeBrush("SurfaceBrush", 34, 72, 76),
                    _ => ThemeBrush("SurfaceBrush", 48, 48, 58),
                },
                Child = new TextBlock
                {
                    Text = task.Title,
                    Foreground = ThemeBrush("TextBrush", 247, 244, 245),
                    TextWrapping = TextWrapping.Wrap,
                    TextTrimming = TextTrimming.CharacterEllipsis,
                    MaxLines = 2,
                },
            };
            ToolTipService.SetToolTip(node, $"{task.Id} · {task.Status}");
            Microsoft.UI.Xaml.Controls.Canvas.SetLeft(node, position.X);
            Microsoft.UI.Xaml.Controls.Canvas.SetTop(node, position.Y);
            _taskGraphCanvas.Children.Add(node);
        }
    }

    private void AddTaskStatusButton(Panel panel, string title, TaskDto task, string status)
    {
        var button = new Button { Content = title, Padding = new Thickness(8, 3, 8, 3), FontSize = 11 };
        button.Click += async (_, _) =>
        {
            button.IsEnabled = false;
            await SetTaskStatusAsync(task, status);
        };
        panel.Children.Add(button);
    }

    private void AddTaskControlButton(
        Panel panel,
        string title,
        TaskDto task,
        Func<TaskDto, Task> action)
    {
        var button = new Button { Content = title, Padding = new Thickness(8, 3, 8, 3), FontSize = 11 };
        button.Click += async (_, _) =>
        {
            button.IsEnabled = false;
            await action(task);
        };
        panel.Children.Add(button);
    }

    private async Task StartTaskControlAsync(TaskDto task)
    {
        if (_taskWorkspaceStatus is null)
        {
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await _ipc.UpdateTaskStatusAsync(task.Id, task.Version, "in_progress", CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "task.status_updated")
                {
                    throw new InvalidOperationException("Core не подтвердил запуск задачи.");
                }
                var prompt = string.IsNullOrWhiteSpace(task.Description)
                    ? task.Title
                    : $"{task.Title}\n\n{task.Description}\n\nКритерии приемки: {task.AcceptanceCriteria}";
                await _ipc.StartTaskAsync(task.Id, prompt, _state.WorkspacePath ?? Environment.CurrentDirectory, CancellationToken.None);
            }
            finally
            {
                _ipcRequestGate.Release();
            }
            _taskWorkspaceStatus.Text = $"Задача «{task.Title}» запущена.";
            await LoadTaskWorkspaceAsync();
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Запуск задачи не выполнен: {error.Message}";
        }
    }

    private async Task StopTaskControlAsync(TaskDto task)
    {
        if (_taskWorkspaceStatus is null)
        {
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await _ipc.StopTaskAsync(task.Id, CancellationToken.None);
                await _ipc.UpdateTaskStatusAsync(task.Id, task.Version, "ready", CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "task.status_updated")
                {
                    throw new InvalidOperationException("Core не подтвердил остановку задачи.");
                }
            }
            finally
            {
                _ipcRequestGate.Release();
            }
            _taskWorkspaceStatus.Text = $"Задача «{task.Title}» остановлена и возвращена в ready.";
            await LoadTaskWorkspaceAsync();
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Остановка задачи не выполнена: {error.Message}";
        }
    }

    private async Task SetTaskStatusAsync(TaskDto task, string status)
    {
        if (_taskWorkspaceStatus is null)
        {
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await _ipc.UpdateTaskStatusAsync(task.Id, task.Version, status, CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "task.status_updated")
                {
                    throw new InvalidOperationException("Core не подтвердил переход статуса.");
                }
            }
            finally
            {
                _ipcRequestGate.Release();
            }
            await LoadTaskWorkspaceAsync();
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Переход задачи не выполнен: {error.Message}";
        }
    }

    private async Task RequestTaskHistoryAsync(TaskDto task)
    {
        if (_taskWorkspaceStatus is null)
        {
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await _ipc.RequestTaskHistoryAsync(task.Id, 20, CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "task.history")
                {
                    throw new InvalidOperationException("Core не вернул историю задачи.");
                }
                using var json = JsonDocument.Parse(response.Payload);
                var events = json.RootElement.GetProperty("events");
                var labels = events.EnumerateArray()
                    .Select(FormatHistoryEvent)
                    .ToArray();
                _taskWorkspaceStatus.Text = labels.Length == 0
                    ? $"История задачи «{task.Title}» пуста."
                    : $"История «{task.Title}»: {string.Join(" → ", labels)}";
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Не удалось загрузить историю: {error.Message}";
        }
    }

    private static string FormatHistoryEvent(JsonElement item)
    {
        var eventType = item.GetProperty("event_type").GetString() ?? "event";
        var createdAt = item.GetProperty("created_at").GetString() ?? string.Empty;
        if (eventType == "build.applied")
        {
            try
            {
                var bytes = item.GetProperty("payload").EnumerateArray().Select(value => value.GetByte()).ToArray();
                using var payload = JsonDocument.Parse(bytes);
                var root = payload.RootElement;
                var diff = root.TryGetProperty("diff", out var diffValue)
                    ? string.Join(", ", diffValue.EnumerateArray().Select(item =>
                        $"{item.GetProperty("operation").GetString()}:{item.GetProperty("relative_path").GetString()}"))
                    : "нет подробностей";
                return $"Build applied: run {root.GetProperty("run_id").GetString()}, snapshot {root.GetProperty("snapshot_id").GetString()}, diff {root.GetProperty("diff_count").GetInt32()} [{diff}] ({createdAt})";
            }
            catch (Exception error) when (error is JsonException or InvalidOperationException or KeyNotFoundException)
            {
                return $"{eventType} ({createdAt})";
            }
        }

        if (eventType is "run.reconciliation.completed" or "run.recovery.blocked")
        {
            try
            {
                var bytes = item.GetProperty("payload").EnumerateArray().Select(value => value.GetByte()).ToArray();
                using var payload = JsonDocument.Parse(bytes);
                var root = payload.RootElement;
                var runId = root.TryGetProperty("run_id", out var run)
                    ? run.GetString()
                    : "неизвестен";
                var effectId = root.TryGetProperty("effect_id", out var effect)
                    ? effect.GetString()
                    : "неизвестен";

                if (eventType == "run.reconciliation.completed")
                {
                    var snapshotId = root.TryGetProperty("snapshot_id", out var snapshot) && snapshot.ValueKind != JsonValueKind.Null
                        ? snapshot.GetString()
                        : "неизвестен";
                    var decision = root.TryGetProperty("decision", out var decisionValue)
                        ? decisionValue.GetString()
                        : "applied";
                    return $"RESUMABLE: результат подтверждён (run {runId}, effect {effectId}, snapshot {snapshotId}, решение {decision}) ({createdAt})";
                }

                var reason = root.TryGetProperty("reason", out var reasonValue)
                    ? reasonValue.GetString()
                    : "требуется проверка";
                return $"BLOCKED: неизвестный effect остановлен (run {runId}, effect {effectId}, причина {reason}) ({createdAt})";
            }
            catch (Exception error) when (error is JsonException or InvalidOperationException or KeyNotFoundException)
            {
                return eventType == "run.reconciliation.completed"
                    ? $"RESUMABLE: результат подтверждён ({createdAt})"
                    : $"BLOCKED: recovery требует проверки ({createdAt})";
            }
        }

        return $"{eventType} ({createdAt})";
    }

    private async Task RestoreLatestSnapshotAsync(TaskDto task)
    {
        var project = ActiveProject();
        if (project is null || _taskWorkspaceStatus is null)
        {
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await EnsureCoreProjectAsync(project);
                await _ipc.RequestTaskSnapshotAsync(project.Id, task.Id, CancellationToken.None);
                var snapshotResponse = await _ipc.ReadEventAsync(CancellationToken.None);
                if (snapshotResponse.EventType != "task.snapshot")
                {
                    throw new InvalidOperationException("Core не вернул snapshot задачи.");
                }
                using var snapshotJson = JsonDocument.Parse(snapshotResponse.Payload);
                var snapshotRoot = snapshotJson.RootElement;
                var snapshotId = snapshotRoot.GetProperty("id").GetString();
                var snapshot = snapshotRoot.GetProperty("snapshot");
                var diff = snapshot.TryGetProperty("diff", out var diffValue)
                    ? string.Join("\n", diffValue.EnumerateArray().Select(item =>
                        $"{item.GetProperty("operation").GetString()}: {item.GetProperty("relative_path").GetString()}"))
                    : "diff отсутствует";
                var rollbackScope = snapshot.TryGetProperty("rollback_scope", out var scopeValue)
                    ? scopeValue.GetString()
                    : "workspace_files_only";
                var dialog = new ContentDialog
                {
                    Title = "Подтвердить rollback snapshot",
                    Content = new TextBlock
                    {
                        Text = $"Snapshot: {snapshotId}\nRun: {snapshotRoot.GetProperty("run_id").GetString()}\nWorkspace hash: {snapshotRoot.GetProperty("workspace_hash").GetString()}\n\n" +
                               $"ГРАНИЦА ROLLBACK: {rollbackScope}\nВосстанавливаются ТОЛЬКО файлы workspace из snapshot. " +
                               $"SQLite/durable состояние НЕ откатывается. Внешние эффекты (сеть, процессы) НЕ отменяются.\n\n{diff}",
                        TextWrapping = TextWrapping.Wrap,
                    },
                    PrimaryButtonText = "Одобрить rollback",
                    CloseButtonText = "Отмена",
                    XamlRoot = ((FrameworkElement)Content).XamlRoot,
                };
                if (await dialog.ShowAsync() != ContentDialogResult.Primary)
                {
                    _taskWorkspaceStatus.Text = "Rollback отменён до изменения workspace.";
                    return;
                }

                await _ipc.RestoreTaskSnapshotAsync(project.Id, task.Id, snapshotId ?? string.Empty, CancellationToken.None);
                var restored = await _ipc.ReadEventAsync(CancellationToken.None);
                if (restored.EventType != "snapshot.restored")
                {
                    throw new InvalidOperationException("Core не подтвердил rollback snapshot.");
                }
                _taskWorkspaceStatus.Text = $"Snapshot {snapshotId} восстановлен с approval и записан в audit.";
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Rollback не выполнен: {error.Message}";
        }
    }

    private async Task RequestTaskContextAsync(TaskDto task)
    {
        var project = ActiveProject();
        if (project is null || _taskWorkspaceStatus is null)
        {
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await _ipc.RequestTaskContextAsync(project.Id, task.Id, 4096, CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "task.context")
                {
                    throw new InvalidOperationException("Core не вернул task context.");
                }
                using var json = JsonDocument.Parse(response.Payload);
                var hash = json.RootElement.GetProperty("workspace_hash").GetString();
                var context = json.RootElement.GetProperty("context").GetString() ?? string.Empty;
                _taskWorkspaceStatus.Text = $"Context hash: {hash}\n{context}";
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Не удалось собрать context: {error.Message}";
        }
    }

    private async Task RequestTaskPlanSpecAsync(TaskDto task)
    {
        var project = ActiveProject();
        if (project is null || _taskWorkspaceStatus is null)
        {
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await _ipc.RequestTaskPlanSpecAsync(project.Id, task.Id, 4096, CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "task.plan_spec")
                {
                    throw new InvalidOperationException("Core не вернул Plan/Spec.");
                }
                using var json = JsonDocument.Parse(response.Payload);
                _taskWorkspaceStatus.Text = $"READ-ONLY PLAN\n{json.RootElement.GetProperty("plan").GetString()}\n\nSPEC\n{json.RootElement.GetProperty("spec").GetString()}";
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Не удалось получить Plan/Spec: {error.Message}";
        }
    }

    private async Task PrepareBuildDialogAsync(TaskDto task, string? initialPath = null, string? initialContent = null)
    {
        var project = ActiveProject();
        if (project is null || _taskWorkspaceStatus is null)
        {
            return;
        }

        var relativePath = new TextBox { Header = "Разрешённый относительный путь", Text = initialPath ?? "src/README.md" };
        var newContent = new TextBox { Header = "Новое содержимое", Text = initialContent ?? string.Empty, AcceptsReturn = true, TextWrapping = TextWrapping.Wrap, MinHeight = 150 };
        var proposalDialog = new ContentDialog
        {
            Title = $"Build preview: {task.Title}",
            Content = new StackPanel { Spacing = 10, Children = { relativePath, newContent } },
            PrimaryButtonText = "Подготовить diff",
            CloseButtonText = "Отмена",
            XamlRoot = ((FrameworkElement)Content).XamlRoot,
        };
        if (await proposalDialog.ShowAsync() != ContentDialogResult.Primary || string.IsNullOrWhiteSpace(relativePath.Text))
        {
            return;
        }

        var normalizedPath = relativePath.Text.Trim().Replace('\\', '/');
        var directory = Path.GetDirectoryName(normalizedPath)?.Replace('\\', '/') ?? string.Empty;
        var extension = Path.GetExtension(normalizedPath).TrimStart('.');
        var proposal = new BuildProposalDto(
            new BuildScopeDto(
                string.IsNullOrWhiteSpace(directory) ? [] : [directory],
                ["write"],
                ["updated source"],
                [],
                [string.IsNullOrWhiteSpace(extension) ? "txt" : extension],
                1,
                Math.Max(1, newContent.Text.Length),
                false,
                false,
                false,
                null,
                task.AcceptanceCriteria,
                "medium",
                30000),
            [new BuildChangeDto(normalizedPath, newContent.Text, null, false)]);
        var proposalJson = JsonSerializer.SerializeToUtf8Bytes(proposal);

        try
        {
            byte[] approvedJson;
            await _ipcRequestGate.WaitAsync();
            try
            {
                await EnsureCoreProjectAsync(project);
                await _ipc.PrepareBuildAsync(project.Id, proposalJson, CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "build.prepared")
                {
                    throw new InvalidOperationException("Core не подготовил Build proposal.");
                }
                approvedJson = response.Payload;
            }
            finally
            {
                _ipcRequestGate.Release();
            }

            using var approved = JsonDocument.Parse(approvedJson);
            var intentHash = approved.RootElement.GetProperty("intent_hash").GetString();
            var effectivePermissionsHash = approved.RootElement.GetProperty("effective_permissions_hash").GetString();
            var baseline = approved.RootElement.GetProperty("expected_workspace_hash").GetString();
            var changes = approved.RootElement.GetProperty("changes");
            var changedFiles = changes.GetArrayLength() == 0
                ? "нет изменений"
                : string.Join(", ", changes.EnumerateArray().Select(item => item.GetProperty("relative_path").GetString()));
            var diffSummary = "нет diff";
            if (approved.RootElement.TryGetProperty("preview_diff", out var previewDiff) && previewDiff.GetArrayLength() > 0)
            {
                diffSummary = string.Join("\n", previewDiff.EnumerateArray().Select(item =>
                {
                    var path = item.GetProperty("relative_path").GetString();
                    var operation = item.GetProperty("operation").GetString();
                    var bytes = item.GetProperty("bytes_changed").GetInt64();
                    return $"  {operation}: {path} ({bytes} bytes)";
                }));
            }
            var scope = approved.RootElement.GetProperty("scope");
            var allowedPaths = string.Join(", ", scope.GetProperty("allowed_paths").EnumerateArray().Select(item => item.GetString()));
            var allowedOperations = string.Join(", ", scope.GetProperty("allowed_operations").EnumerateArray().Select(item => item.GetString()));
            var expectedOutputs = string.Join(", ", scope.GetProperty("expected_outputs").EnumerateArray().Select(item => item.GetString()));
            var allowedTypes = string.Join(", ", scope.GetProperty("allowed_file_types").EnumerateArray().Select(item => item.GetString()));
            var maxFiles = scope.GetProperty("max_files_changed").GetInt64();
            var maxBytes = scope.GetProperty("max_bytes_changed").GetInt64();
            var allowCreate = scope.GetProperty("allow_create").GetBoolean();
            var allowDelete = scope.GetProperty("allow_delete").GetBoolean();
            var allowRename = scope.GetProperty("allow_rename").GetBoolean();
            var acceptanceCriteria = scope.GetProperty("acceptance_criteria").GetString();
            var riskClass = scope.GetProperty("risk_class").GetString();
            var timeoutMs = scope.GetProperty("timeout_ms").GetInt64();
            var approvalDialog = new ContentDialog
            {
                Title = "Подтвердить bounded Build",
                Content = new TextBlock
                {
                    Text = $"Файлы: {changedFiles}\nDiff:\n{diffSummary}\nОперации: {allowedOperations}\nОжидаемый output: {expectedOutputs}\nAllowed paths: {allowedPaths}\nТипы файлов: {allowedTypes}\nЛимит файлов: {maxFiles}\nЛимит байт: {maxBytes}\nСоздание: {(allowCreate ? "разрешено" : "запрещено")} · удаление: {(allowDelete ? "разрешено" : "запрещено")} · rename: {(allowRename ? "разрешено" : "запрещено")}\nRisk: {riskClass} · timeout: {timeoutMs} ms\nAcceptance criteria: {acceptanceCriteria}\nBaseline: {baseline}\nEffective permissions hash: {effectivePermissionsHash}\nIntent hash: {intentHash}\n\nЗапись ещё не выполнялась. Rollback после apply восстановит только файлы workspace — не SQLite и не внешние эффекты.",
                    TextWrapping = TextWrapping.Wrap,
                },
                PrimaryButtonText = "Одобрить и применить",
                CloseButtonText = "Отмена",
                XamlRoot = ((FrameworkElement)Content).XamlRoot,
            };
            if (await approvalDialog.ShowAsync() != ContentDialogResult.Primary)
            {
                _taskWorkspaceStatus.Text = "Build отменён до записи файлов.";
                return;
            }

            await _ipcRequestGate.WaitAsync();
            try
            {
                var runId = $"build-{task.Id}-{DateTimeOffset.UtcNow:yyyyMMddHHmmssfff}";
                await _ipc.ApplyApprovedBuildAsync(project.Id, runId, task.Id, approvedJson, CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "build.applied")
                {
                    throw new InvalidOperationException("Core не подтвердил Build.");
                }
            }
            finally
            {
                _ipcRequestGate.Release();
            }
            _taskWorkspaceStatus.Text = $"Build применён с approval intent {intentHash}. Run-linked snapshot сохранён Core. Rollback scope: только файлы workspace (не SQLite, не внешние эффекты).";
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Build не применён: {error.Message}";
        }
    }

    private async Task EditBuildPolicyAsync()
    {
        var project = ActiveProject();
        if (project is null || _taskWorkspaceStatus is null)
        {
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await EnsureCoreProjectAsync(project);
                await _ipc.RequestBuildPolicyAsync(project.Id, CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "build.policy")
                {
                    throw new InvalidOperationException("Core не вернул build policy.");
                }
                var current = JsonSerializer.Deserialize<BuildPolicyEnvelope>(response.Payload)
                    ?? throw new InvalidOperationException("Core вернул пустую build policy.");
                var paths = new TextBox { Header = "Разрешённые пути (через запятую)", Text = string.Join(", ", current.Policy.AllowedPaths) };
                var operations = new TextBox { Header = "Разрешённые операции (через запятую)", Text = string.Join(", ", current.Policy.AllowedOperations) };
                var maxFiles = new NumberBox { Header = "Максимум изменённых файлов", Value = current.Policy.MaxFilesChanged, Minimum = 1, Maximum = 1000, SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Compact };
                var maxBytes = new NumberBox { Header = "Максимум изменённых байт", Value = current.Policy.MaxBytesChanged, Minimum = 1, Maximum = 16 * 1024 * 1024, SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Compact };
                var timeout = new NumberBox { Header = "Таймаут Build (мс)", Value = current.Policy.TimeoutMs, Minimum = 1, Maximum = 300000, SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Compact };
                var risk = new ComboBox { Header = "Класс риска", ItemsSource = new[] { "low", "medium", "high" }, SelectedItem = current.Policy.RiskClass };
                var allowCreate = new CheckBox { Content = "Разрешить создание файлов", IsChecked = current.Policy.AllowCreate };
                var allowDelete = new CheckBox { Content = "Разрешить удаление файлов", IsChecked = current.Policy.AllowDelete };
                var allowRename = new CheckBox { Content = "Разрешить переименование", IsChecked = current.Policy.AllowRename };
                var dialog = new ContentDialog
                {
                    Title = $"Build policy · версия {current.Version}",
                    Content = new ScrollViewer { Content = new StackPanel { Spacing = 8, Children = { paths, operations, maxFiles, maxBytes, timeout, risk, allowCreate, allowDelete, allowRename } } },
                    PrimaryButtonText = "Сохранить в Core",
                    CloseButtonText = "Отмена",
                    XamlRoot = ((FrameworkElement)Content).XamlRoot,
                };
                if (await dialog.ShowAsync() != ContentDialogResult.Primary)
                {
                    return;
                }

                var policy = current.Policy with
                {
                    AllowedPaths = SplitPolicyList(paths.Text),
                    AllowedOperations = SplitPolicyList(operations.Text),
                    MaxFilesChanged = (int)maxFiles.Value,
                    MaxBytesChanged = (int)maxBytes.Value,
                    TimeoutMs = (int)timeout.Value,
                    RiskClass = risk.SelectedItem as string ?? current.Policy.RiskClass,
                    AllowCreate = allowCreate.IsChecked == true,
                    AllowDelete = allowDelete.IsChecked == true,
                    AllowRename = allowRename.IsChecked == true,
                };
                var policyJson = JsonSerializer.SerializeToUtf8Bytes(policy);
                await _ipc.SaveBuildPolicyAsync(project.Id, policyJson, current.Version, CancellationToken.None);
                var saved = await _ipc.ReadEventAsync(CancellationToken.None);
                if (saved.EventType != "build.policy.saved")
                {
                    throw new InvalidOperationException("Core не подтвердил сохранение policy.");
                }
                _taskWorkspaceStatus.Text = "Build policy сохранена в Core.";
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Policy не сохранена: {error.Message}";
        }
    }

    private async Task ShowDoctorReportAsync()
    {
        var projectId = ActiveProject()?.Id ?? string.Empty;
        var detailLevel = 0;
        var statusText = new TextBlock { Text = string.Empty, TextWrapping = TextWrapping.Wrap, FontSize = 11 };
        var checksList = new StackPanel { Spacing = 6 };
        var detailToggle = new ComboBox
        {
            Header = "Уровень детализации",
            ItemsSource = new[] { "Summary", "Detailed" },
            SelectedIndex = 0,
        };
        var refresh = new Button { Content = "Обновить" };
        var export = new Button { Content = "Экспорт логов и метрик…" };

        async Task RefreshAsync()
        {
            checksList.Children.Clear();
            statusText.Text = "Загрузка…";
            try
            {
                await _ipcRequestGate.WaitAsync();
                try
                {
                    if (!string.IsNullOrEmpty(projectId))
                    {
                        var project = ActiveProject();
                        if (project is not null)
                        {
                            await EnsureCoreProjectAsync(project);
                        }
                    }
                    await _ipc.RequestDoctorReportAsync(projectId, detailLevel, CancellationToken.None);
                    var response = await _ipc.ReadEventAsync(CancellationToken.None);
                    if (response.EventType != "doctor.report")
                    {
                        throw new InvalidOperationException("Core не вернул doctor.report.");
                    }
                    var report = JsonSerializer.Deserialize<DoctorReportDto>(response.Payload)
                        ?? throw new InvalidOperationException("Core вернул пустой doctor report.");
                    statusText.Text = report.IsActionable() ? "Есть пункты, требующие внимания." : "Всё в порядке.";
                    foreach (var check in report.Checks)
                    {
                        checksList.Children.Add(BuildDoctorCheckRow(check));
                    }
                }
                finally
                {
                    _ipcRequestGate.Release();
                }
            }
            catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
            {
                statusText.Text = $"Doctor недоступен: {error.Message}";
            }
        }

        detailToggle.SelectionChanged += async (_, _) =>
        {
            detailLevel = detailToggle.SelectedIndex == 1 ? 1 : 0;
            await RefreshAsync();
        };
        refresh.Click += async (_, _) => await RefreshAsync();
        export.Click += async (_, _) =>
        {
            var picker = new FileSavePicker();
            picker.FileTypeChoices.Add("JSON Lines", new List<string> { ".jsonl" });
            picker.SuggestedFileName = "evohime-doctor-export";
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            var file = await picker.PickSaveFileAsync();
            if (file is null)
            {
                return;
            }
            try
            {
                await _ipcRequestGate.WaitAsync();
                try
                {
                    await _ipc.RequestDoctorExportAsync(file.Path, CancellationToken.None);
                    var response = await _ipc.ReadEventAsync(CancellationToken.None);
                    if (response.EventType != "doctor.export.completed")
                    {
                        throw new InvalidOperationException("Core не подтвердил экспорт.");
                    }
                    using var json = JsonDocument.Parse(response.Payload);
                    var lines = json.RootElement.GetProperty("lines_exported").GetInt64();
                    statusText.Text = $"Экспортировано строк: {lines}. Файл: {file.Path}";
                }
                finally
                {
                    _ipcRequestGate.Release();
                }
            }
            catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
            {
                statusText.Text = $"Экспорт не выполнен: {error.Message}";
            }
        };

        var dialog = new ContentDialog
        {
            Title = "Core Doctor",
            Content = new ScrollViewer
            {
                MaxHeight = 480,
                Content = new StackPanel
                {
                    Spacing = 10,
                    Children = { detailToggle, refresh, statusText, checksList, export },
                },
            },
            CloseButtonText = "Закрыть",
            XamlRoot = ((FrameworkElement)Content).XamlRoot,
        };
        await RefreshAsync();
        await dialog.ShowAsync();
    }

    private async Task ShowCapabilitySelectionAsync()
    {
        var taskId = ActiveProject()?.Id ?? "default-task";
        var statusText = new TextBlock { Text = string.Empty, TextWrapping = TextWrapping.Wrap, FontSize = 11 };
        var selectionPanel = new StackPanel { Spacing = 4 };
        var taskIdBox = new TextBox { Header = "Task ID", Text = taskId };
        var intentBox = new TextBox { Header = "Intent", Text = string.Empty };
        var riskBox = new ComboBox
        {
            Header = "Requested risk",
            ItemsSource = new[] { "low", "medium", "high" },
            SelectedIndex = 0,
        };
        var replacePicker = new ComboBox { Header = "Заменить на", ItemsSource = Array.Empty<string>() };
        var getButton = new Button { Content = "Получить выбор" };
        var pinButton = new Button { Content = "Закрепить" };
        var replaceButton = new Button { Content = "Заменить" };

        async Task RefreshManifestsAsync()
        {
            try
            {
                await _ipcRequestGate.WaitAsync();
                try
                {
                    await _ipc.RequestCapabilityListAsync(50, CancellationToken.None);
                    var response = await _ipc.ReadEventAsync(CancellationToken.None);
                    if (response.EventType != "capability.list")
                    {
                        return;
                    }
                    var listed = JsonSerializer.Deserialize<CapabilityListDto>(response.Payload);
                    replacePicker.ItemsSource = listed?.Manifests.Select(manifest => manifest.Name).ToArray()
                        ?? Array.Empty<string>();
                }
                finally
                {
                    _ipcRequestGate.Release();
                }
            }
            catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
            {
                statusText.Text = $"Список манифестов недоступен: {error.Message}";
            }
        }

        async Task RunGetAsync()
        {
            selectionPanel.Children.Clear();
            statusText.Text = "Загрузка…";
            try
            {
                await _ipcRequestGate.WaitAsync();
                try
                {
                    await _ipc.RequestCapabilitySelectionAsync(
                        taskIdBox.Text,
                        intentBox.Text,
                        Array.Empty<string>(),
                        Array.Empty<string>(),
                        riskBox.SelectedItem as string ?? "low",
                        CancellationToken.None);
                    var response = await _ipc.ReadEventAsync(CancellationToken.None);
                    if (response.EventType != "capability.selection")
                    {
                        throw new InvalidOperationException("Core не вернул capability.selection.");
                    }
                    var state = JsonSerializer.Deserialize<CapabilitySelectionStateDto>(response.Payload)
                        ?? throw new InvalidOperationException("Core вернул пустой capability selection.");
                    RenderCapabilitySelection(selectionPanel, state);
                    statusText.Text = $"Origin: {state.Origin}";
                }
                finally
                {
                    _ipcRequestGate.Release();
                }
            }
            catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
            {
                statusText.Text = $"Selection недоступен: {error.Message}";
            }
        }

        getButton.Click += async (_, _) => await RunGetAsync();

        pinButton.Click += async (_, _) =>
        {
            try
            {
                await _ipcRequestGate.WaitAsync();
                try
                {
                    await _ipc.PinCapabilitySelectionAsync(taskIdBox.Text, CancellationToken.None);
                    var response = await _ipc.ReadEventAsync(CancellationToken.None);
                    if (response.EventType != "capability.selection.pinned")
                    {
                        throw new InvalidOperationException("Core не подтвердил pin.");
                    }
                    var state = JsonSerializer.Deserialize<CapabilitySelectionStateDto>(response.Payload)
                        ?? throw new InvalidOperationException("Core вернул пустой capability selection.");
                    RenderCapabilitySelection(selectionPanel, state);
                    statusText.Text = $"Закреплено. Origin: {state.Origin}";
                }
                finally
                {
                    _ipcRequestGate.Release();
                }
            }
            catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
            {
                statusText.Text = $"Pin не выполнен: {error.Message}";
            }
        };

        replaceButton.Click += async (_, _) =>
        {
            var manifestName = replacePicker.SelectedItem as string;
            if (string.IsNullOrWhiteSpace(manifestName))
            {
                statusText.Text = "Укажите имя манифеста для замены.";
                return;
            }
            try
            {
                await _ipcRequestGate.WaitAsync();
                try
                {
                    await _ipc.ReplaceCapabilitySelectionAsync(
                        taskIdBox.Text,
                        manifestName,
                        intentBox.Text,
                        Array.Empty<string>(),
                        Array.Empty<string>(),
                        riskBox.SelectedItem as string ?? "low",
                        CancellationToken.None);
                    var response = await _ipc.ReadEventAsync(CancellationToken.None);
                    if (response.EventType != "capability.selection.replaced")
                    {
                        throw new InvalidOperationException("Core не подтвердил замену.");
                    }
                    var state = JsonSerializer.Deserialize<CapabilitySelectionStateDto>(response.Payload)
                        ?? throw new InvalidOperationException("Core вернул пустой capability selection.");
                    RenderCapabilitySelection(selectionPanel, state);
                    statusText.Text = $"Заменено. Origin: {state.Origin}";
                }
                finally
                {
                    _ipcRequestGate.Release();
                }
            }
            catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
            {
                statusText.Text = $"Замена не выполнена: {error.Message}";
            }
        };

        var dialog = new ContentDialog
        {
            Title = "Capability Selection",
            Content = new ScrollViewer
            {
                MaxHeight = 480,
                Content = new StackPanel
                {
                    Spacing = 10,
                    Children =
                    {
                        taskIdBox,
                        intentBox,
                        riskBox,
                        getButton,
                        statusText,
                        selectionPanel,
                        replacePicker,
                        new StackPanel
                        {
                            Orientation = Orientation.Horizontal,
                            Spacing = 8,
                            Children = { pinButton, replaceButton },
                        },
                    },
                },
            },
            CloseButtonText = "Закрыть",
            XamlRoot = ((FrameworkElement)Content).XamlRoot,
        };
        await RefreshManifestsAsync();
        await dialog.ShowAsync();
    }

    private static void RenderCapabilitySelection(StackPanel panel, CapabilitySelectionStateDto state)
    {
        panel.Children.Clear();
        var selection = state.Selection;
        panel.Children.Add(new TextBlock
        {
            Text = $"{selection.ManifestName} v{selection.Version} ({state.Origin}{(selection.Pinned ? ", pinned" : string.Empty)})",
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            TextWrapping = TextWrapping.Wrap,
        });
        if (selection.Reasons.Length > 0)
        {
            panel.Children.Add(new TextBlock { Text = "Причины: " + string.Join("; ", selection.Reasons), TextWrapping = TextWrapping.Wrap, FontSize = 12 });
        }
        panel.Children.Add(new TextBlock
        {
            Text = $"Risk: {selection.Permissions.RiskClass}; Tools: {string.Join(", ", selection.Permissions.AllowedTools)}",
            TextWrapping = TextWrapping.Wrap,
            FontSize = 12,
        });
        if (selection.AcceptanceCriteria.Length > 0)
        {
            panel.Children.Add(new TextBlock { Text = "Acceptance criteria: " + string.Join("; ", selection.AcceptanceCriteria), TextWrapping = TextWrapping.Wrap, FontSize = 11, Opacity = 0.8 });
        }
    }

    private sealed record CapabilitySelectionStateDto(
        [property: JsonPropertyName("selection")] CapabilitySelectionViewDto Selection,
        [property: JsonPropertyName("origin")] string Origin);

    private sealed record CapabilitySelectionViewDto(
        [property: JsonPropertyName("manifest_name")] string ManifestName,
        [property: JsonPropertyName("version")] string Version,
        [property: JsonPropertyName("reasons")] string[] Reasons,
        [property: JsonPropertyName("permissions")] CapabilityPermissionsDto Permissions,
        [property: JsonPropertyName("acceptance_criteria")] string[] AcceptanceCriteria,
        [property: JsonPropertyName("pinned")] bool Pinned);

    private sealed record CapabilityPermissionsDto(
        [property: JsonPropertyName("allowed_tools")] string[] AllowedTools,
        [property: JsonPropertyName("allowed_domains")] string[] AllowedDomains,
        [property: JsonPropertyName("risk_class")] string RiskClass);

    private sealed record CapabilityListDto(
        [property: JsonPropertyName("manifests")] CapabilityManifestNameDto[] Manifests);

    private sealed record CapabilityManifestNameDto(
        [property: JsonPropertyName("name")] string Name);

    private static Border BuildDoctorCheckRow(DoctorCheckDto check)
    {
        var (background, statusLabel) = check.Status switch
        {
            "OK" => (new SolidColorBrush(Windows.UI.Color.FromArgb(40, 60, 200, 100)), "OK"),
            "WARN" => (new SolidColorBrush(Windows.UI.Color.FromArgb(40, 220, 180, 40)), "WARN"),
            "FAIL" => (new SolidColorBrush(Windows.UI.Color.FromArgb(40, 220, 70, 70)), "FAIL"),
            _ => (new SolidColorBrush(Windows.UI.Color.FromArgb(40, 220, 70, 70)), "BLOCKED"),
        };
        var stack = new StackPanel { Spacing = 2, Margin = new Thickness(10, 6, 10, 6) };
        stack.Children.Add(new TextBlock { Text = $"[{statusLabel}] {check.Id}: {check.Summary}", FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, TextWrapping = TextWrapping.Wrap });
        stack.Children.Add(new TextBlock { Text = check.Action, TextWrapping = TextWrapping.Wrap, FontSize = 12 });
        if (!string.IsNullOrEmpty(check.Details))
        {
            stack.Children.Add(new TextBlock { Text = check.Details, TextWrapping = TextWrapping.Wrap, FontSize = 11, Opacity = 0.75 });
        }
        return new Border { Background = background, CornerRadius = new CornerRadius(8), Child = stack };
    }

    private static string[] SplitPolicyList(string value) => value
        .Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
        .Where(item => !string.IsNullOrWhiteSpace(item))
        .ToArray();

    private async Task RequestNextReadyTaskAsync()
    {
        var project = ActiveProject();
        if (project is null || _taskWorkspaceStatus is null)
        {
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await EnsureCoreProjectAsync(project);
                await _ipc.RequestNextReadyTaskAsync(project.Id, CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "task.next_ready")
                {
                    throw new InvalidOperationException("Core не вернул next_ready.");
                }
                using var json = JsonDocument.Parse(response.Payload);
                var task = json.RootElement.GetProperty("task");
                if (task.ValueKind == JsonValueKind.Null)
                {
                    _taskWorkspaceStatus.Text = "Готовых задач сейчас нет. Заблокированные задачи не предлагаются.";
                    return;
                }

                var nextTask = JsonSerializer.Deserialize<TaskDto>(task.GetRawText());
                if (nextTask is null)
                {
                    throw new InvalidOperationException("Core вернул пустую next_ready задачу.");
                }

                _taskWorkspaceStatus.Text = string.Join(
                    Environment.NewLine,
                    $"Следующая задача: {nextTask.Title} · {nextTask.Status}",
                    $"Описание: {BoundedStatusText(nextTask.Description)}",
                    $"Критерии приёмки: {BoundedStatusText(nextTask.AcceptanceCriteria)}",
                    $"Версия: {nextTask.Version} · Приоритет: {nextTask.Priority}");
            }
            finally
            {
                _ipcRequestGate.Release();
            }
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Не удалось выбрать следующую задачу: {error.Message}";
        }
    }

    private static string BoundedStatusText(string? value, int maxLength = 700)
    {
        var normalized = string.IsNullOrWhiteSpace(value)
            ? "не указано"
            : value.Trim();
        return normalized.Length <= maxLength
            ? normalized
            : normalized[..maxLength] + "…";
    }

    private async Task CreateTaskDialogAsync(string? parentId)
    {
        var project = ActiveProject();
        if (project is null || _taskWorkspaceStatus is null)
        {
            return;
        }

        var title = new TextBox { Header = "Название", PlaceholderText = "Что нужно сделать?" };
        var description = new TextBox { Header = "Описание", AcceptsReturn = true, TextWrapping = TextWrapping.Wrap, MinHeight = 70 };
        var acceptance = new TextBox { Header = "Критерии приемки", AcceptsReturn = true, TextWrapping = TextWrapping.Wrap, MinHeight = 55 };
        var priority = new NumberBox { Header = "Приоритет", Value = 0, SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Compact };
        var complexity = new ComboBox { Header = "Сложность", ItemsSource = new[] { "small", "medium", "large" }, SelectedIndex = 0 };
        var dialog = new ContentDialog
        {
            Title = parentId is null ? "Новая задача" : "Новая подзадача",
            Content = new StackPanel { Spacing = 10, Children = { title, description, acceptance, priority, complexity } },
            PrimaryButtonText = "Создать",
            CloseButtonText = "Отмена",
            XamlRoot = ((FrameworkElement)Content).XamlRoot,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary || string.IsNullOrWhiteSpace(title.Text))
        {
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await EnsureCoreProjectAsync(project);
                await _ipc.CreateTaskAsync(
                    Guid.NewGuid().ToString("N"),
                    project.Id,
                    parentId ?? string.Empty,
                    title.Text.Trim(),
                    description.Text,
                    acceptance.Text,
                    (long)priority.Value,
                    complexity.SelectedItem as string ?? "medium",
                    CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "task.created")
                {
                    throw new InvalidOperationException("Core не подтвердил создание задачи.");
                }
            }
            finally
            {
                _ipcRequestGate.Release();
            }
            await LoadTaskWorkspaceAsync();
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Задача не создана: {error.Message}";
        }
    }

    private async Task AddTaskEdgeDialogAsync()
    {
        var project = ActiveProject();
        var graph = _lastTaskGraph;
        if (project is null || graph is null || graph.Tasks.Count < 2 || _taskWorkspaceStatus is null)
        {
            if (_taskWorkspaceStatus is not null)
            {
                _taskWorkspaceStatus.Text = "Для зависимости нужны минимум две загруженные задачи.";
            }
            return;
        }

        var choices = graph.Tasks.Select(task => new TaskChoice(task.Id, task.Title)).ToList();
        var from = new ComboBox { Header = "Задача, которая зависит", ItemsSource = choices, DisplayMemberPath = nameof(TaskChoice.Label), SelectedIndex = 0 };
        var to = new ComboBox { Header = "Зависимость", ItemsSource = choices, DisplayMemberPath = nameof(TaskChoice.Label), SelectedIndex = 1 };
        var kind = new TextBox { Header = "Тип связи", Text = "blocks" };
        var dialog = new ContentDialog
        {
            Title = "Добавить зависимость",
            Content = new StackPanel { Spacing = 10, Children = { from, to, kind } },
            PrimaryButtonText = "Добавить",
            CloseButtonText = "Отмена",
            XamlRoot = ((FrameworkElement)Content).XamlRoot,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary || from.SelectedItem is not TaskChoice source || to.SelectedItem is not TaskChoice target || source.Id == target.Id)
        {
            return;
        }

        try
        {
            await _ipcRequestGate.WaitAsync();
            try
            {
                await _ipc.AddTaskEdgeAsync(source.Id, target.Id, string.IsNullOrWhiteSpace(kind.Text) ? "blocks" : kind.Text.Trim(), CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "task.edge_added")
                {
                    throw new InvalidOperationException("Core не подтвердил зависимость.");
                }
            }
            finally
            {
                _ipcRequestGate.Release();
            }
            await LoadTaskWorkspaceAsync();
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Зависимость не добавлена: {error.Message}";
        }
    }

    private async Task ImportPrdFromFileAsync()
    {
        var project = ActiveProject();
        if (project is null || _taskWorkspaceStatus is null)
        {
            return;
        }

        var picker = new FileOpenPicker();
        picker.FileTypeFilter.Add(".md");
        picker.FileTypeFilter.Add(".markdown");
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var file = await picker.PickSingleFileAsync();
        if (file is null)
        {
            return;
        }

        try
        {
            var source = await FileIO.ReadTextAsync(file);
            var importId = Convert.ToHexString(SHA256.HashData(System.Text.Encoding.UTF8.GetBytes($"{file.Path}\n{source}")));
            await _ipcRequestGate.WaitAsync();
            try
            {
                await EnsureCoreProjectAsync(project);
                await _ipc.ImportPrdAsync(importId, project.Id, file.Path, "v1", source, CancellationToken.None);
                var response = await _ipc.ReadEventAsync(CancellationToken.None);
                if (response.EventType != "prd.imported")
                {
                    throw new InvalidOperationException("Core не подтвердил импорт PRD.");
                }
            }
            finally
            {
                _ipcRequestGate.Release();
            }
            _taskWorkspaceStatus.Text = $"Импортирован PRD: {file.Name}";
            await LoadTaskWorkspaceAsync();
        }
        catch (Exception error) when (error is IOException or InvalidOperationException or JsonException or OperationCanceledException)
        {
            _taskWorkspaceStatus.Text = $"Импорт PRD не выполнен: {error.Message}";
        }
    }

    private async Task EnsureCoreProjectAsync(ProjectEntry project)
    {
        if (_coreProjects.Contains(project.Id))
        {
            return;
        }

        if (!_ipc.IsConnected)
        {
            await ConnectToCoreWithRetryAsync(CancellationToken.None);
        }
        await _ipc.CreateProjectAsync(project.Id, project.Name, project.Path, CancellationToken.None);
        var response = await _ipc.ReadEventAsync(CancellationToken.None);
        if (response.EventType != "project.created")
        {
            throw new InvalidOperationException("Core не подтвердил проект.");
        }
        _coreProjects.Add(project.Id);
    }

    private sealed record TaskGraphDto(
        [property: JsonPropertyName("project_id")] string ProjectId,
        [property: JsonPropertyName("tasks")] List<TaskDto> Tasks,
        [property: JsonPropertyName("edges")] List<TaskEdgeDto> Edges);

    private sealed record TaskChoice(string Id, string Label);

    private sealed record BuildProposalDto(
        [property: JsonPropertyName("scope")] BuildScopeDto Scope,
        [property: JsonPropertyName("changes")] BuildChangeDto[] Changes);

    private sealed record BuildPolicyEnvelope(
        [property: JsonPropertyName("project_id")] string ProjectId,
        [property: JsonPropertyName("version")] long Version,
        [property: JsonPropertyName("policy")] BuildScopeDto Policy);

    private sealed record BuildScopeDto(
        [property: JsonPropertyName("allowed_paths")] string[] AllowedPaths,
        [property: JsonPropertyName("allowed_operations")] string[] AllowedOperations,
        [property: JsonPropertyName("expected_outputs")] string[] ExpectedOutputs,
        [property: JsonPropertyName("protected_paths")] string[] ProtectedPaths,
        [property: JsonPropertyName("allowed_file_types")] string[] AllowedFileTypes,
        [property: JsonPropertyName("max_files_changed")] int MaxFilesChanged,
        [property: JsonPropertyName("max_bytes_changed")] int MaxBytesChanged,
        [property: JsonPropertyName("allow_create")] bool AllowCreate,
        [property: JsonPropertyName("allow_delete")] bool AllowDelete,
        [property: JsonPropertyName("allow_rename")] bool AllowRename,
        [property: JsonPropertyName("baseline_snapshot_id")] string? BaselineSnapshotId,
        [property: JsonPropertyName("acceptance_criteria")] string AcceptanceCriteria,
        [property: JsonPropertyName("risk_class")] string RiskClass,
        [property: JsonPropertyName("timeout_ms")] int TimeoutMs);

    private sealed record DoctorReportDto(
        [property: JsonPropertyName("contract_version")] int ContractVersion,
        [property: JsonPropertyName("bounded")] bool Bounded,
        [property: JsonPropertyName("checks")] DoctorCheckDto[] Checks)
    {
        public bool IsActionable() => Checks.Any(check => check.Status != "OK");
    }

    private sealed record DoctorCheckDto(
        [property: JsonPropertyName("id")] string Id,
        [property: JsonPropertyName("status")] string Status,
        [property: JsonPropertyName("summary")] string Summary,
        [property: JsonPropertyName("action")] string Action,
        [property: JsonPropertyName("details")] string? Details);

    private sealed record BuildChangeDto(
        [property: JsonPropertyName("relative_path")] string RelativePath,
        [property: JsonPropertyName("content")] string? Content,
        [property: JsonPropertyName("expected_content_hash")] string? ExpectedContentHash,
        [property: JsonPropertyName("delete")] bool Delete);

    private sealed record TaskDto(
        [property: JsonPropertyName("id")] string Id,
        [property: JsonPropertyName("parent_id")] string? ParentId,
        [property: JsonPropertyName("title")] string Title,
        [property: JsonPropertyName("description")] string Description,
        [property: JsonPropertyName("acceptance_criteria")] string AcceptanceCriteria,
        [property: JsonPropertyName("complexity")] string? Complexity,
        [property: JsonPropertyName("status")] string Status,
        [property: JsonPropertyName("priority")] long Priority,
        [property: JsonPropertyName("version")] long Version);

    private sealed record TaskEdgeDto(
        [property: JsonPropertyName("from_task_id")] string FromTaskId,
        [property: JsonPropertyName("to_task_id")] string ToTaskId,
        [property: JsonPropertyName("kind")] string Kind);

    private Grid BuildPluginsView()
    {
        var view = BuildShellPage(
            "Плагины",
            "Каталог плагинов GitHub для расширения Евы.");
        var content = new Grid { RowSpacing = 12 };
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        var searchBar = new Grid { ColumnSpacing = 8 };
        searchBar.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        searchBar.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        _pluginSearch = new TextBox { PlaceholderText = "Поиск Agent Plugins на GitHub", Text = "agent plugins" };
        _pluginSearch.KeyDown += (_, args) =>
        {
            if (args.Key == VirtualKey.Enter)
            {
                _ = RefreshPluginsAsync();
            }
        };
        searchBar.Children.Add(_pluginSearch);
        var refresh = new Button { Content = "Обновить" };
        refresh.Click += (_, _) => _ = RefreshPluginsAsync();
        Grid.SetColumn(refresh, 1);
        searchBar.Children.Add(refresh);
        content.Children.Add(searchBar);
        _pluginStatus = new TextBlock { Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157), TextWrapping = TextWrapping.Wrap };
        Grid.SetRow(_pluginStatus, 1);
        content.Children.Add(_pluginStatus);
        _pluginsList = new StackPanel { Spacing = 10 };
        var scroll = new ScrollViewer { Content = _pluginsList, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        Grid.SetRow(scroll, 2);
        content.Children.Add(scroll);
        Grid.SetRow(content, 1);
        view.Children.Add(content);
        _ = RefreshPluginsAsync();
        return view;
    }

    private async Task RefreshPluginsAsync()
    {
        if (_pluginsList is null || _pluginStatus is null)
        {
            return;
        }

        _pluginStatus.Text = "Загружаю каталог GitHub…";
        _pluginsList.Children.Clear();
        try
        {
            var plugins = await _pluginCatalogService.SearchAsync(_pluginSearch?.Text ?? "agent plugins");
            _pluginStatus.Text = $"Совместимых Agent Plugins: {plugins.Count}. Установка сохраняет пакеты в {_pluginCatalogService.PluginsDirectory}.";
            foreach (var plugin in plugins)
            {
                _pluginsList.Children.Add(BuildPluginCard(plugin));
            }
            if (plugins.Count == 0)
            {
                _pluginsList.Children.Add(new TextBlock { Text = "GitHub не вернул репозитории по этому запросу.", Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157) });
            }
        }
        catch (Exception error)
        {
            _pluginStatus.Text = $"Не удалось загрузить каталог GitHub: {error.Message}";
        }
    }

    private Border BuildPluginCard(GitHubPlugin plugin)
    {
        var content = new Grid { ColumnSpacing = 12 };
        content.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        content.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var details = new StackPanel { Spacing = 4 };
        var title = new TextBlock { Text = plugin.FullName, FontSize = 15, Foreground = ThemeBrush("TextBrush", 247, 244, 245) };
        title.PointerPressed += (_, _) => OpenExternalUrl(plugin.HtmlUrl);
        details.Children.Add(title);
        details.Children.Add(new TextBlock { Text = string.IsNullOrWhiteSpace(plugin.Manifest.Description) ? plugin.Description : plugin.Manifest.Description, Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157), TextWrapping = TextWrapping.Wrap });
        details.Children.Add(new TextBlock { Text = $"{plugin.Manifest.Name}  ·  {plugin.Manifest.Version}  ·  ★ {plugin.Stars:N0}  ·  {(plugin.Manifest.HasMcp ? "MCP" : "Skills")}", Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157), FontSize = 11 });
        content.Children.Add(details);
        var action = new Button { Content = plugin.Installed ? "Удалить" : "Установить" };
        action.Click += async (_, _) =>
        {
            action.IsEnabled = false;
            try
            {
                if (plugin.Installed)
                {
                    _pluginCatalogService.Uninstall(plugin);
                }
                else
                {
                    await _pluginCatalogService.InstallAsync(plugin);
                }
                await RefreshPluginsAsync();
            }
            catch (Exception error)
            {
                if (_pluginStatus is not null) _pluginStatus.Text = $"Операция с {plugin.FullName} не выполнена: {error.Message}";
                action.IsEnabled = true;
            }
        };
        Grid.SetColumn(action, 1);
        content.Children.Add(action);
        return new Border
        {
            Background = ThemeBrush("SurfaceRaisedBrush", 23, 28, 37),
            BorderBrush = ThemeBrush("BorderBrush", 68, 32, 43),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(10),
            Padding = new Thickness(14),
            Child = content,
        };
    }

    private static void OpenExternalUrl(string url)
    {
        Process.Start(new ProcessStartInfo { FileName = url, UseShellExecute = true });
    }

    private Grid BuildShellPage(string titleText, string subtitle)
    {
        var view = new Grid { Margin = new Thickness(30, 24, 30, 22) };
        view.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        view.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        var header = new Grid { Margin = new Thickness(0, 0, 0, 22) };
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        header.Children.Add(new StackPanel
        {
            Spacing = 5,
            Children =
            {
                new TextBlock { Text = titleText, FontSize = 28, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Foreground = ThemeBrush("TextBrush", 247, 244, 245) },
                new TextBlock { Text = subtitle, FontSize = 14, Foreground = ThemeBrush("MutedTextBrush", 143, 146, 157), TextWrapping = TextWrapping.Wrap },
            },
        });
        var back = new Button { Content = "←  Вернуться к чату" };
        back.Click += (_, _) => ShowHomeView();
        Grid.SetColumn(back, 1);
        header.Children.Add(back);
        view.Children.Add(header);
        return view;
    }

    private static Button CreateNavigationButton(string text, Action action)
    {
        var button = new Button { Content = text };
        button.Click += (_, _) => action();
        return button;
    }

    private async Task RefreshGitHubProfileAsync()
    {
        var profile = await _githubAuth.GetProfileAsync();
        if (_githubProfileText is null || _githubProfileStatus is null)
        {
            return;
        }

        _githubProfileText.Text = profile.IsAuthenticated ? profile.DisplayName : "GitHub";
        _githubProfileStatus.Text = profile.IsAuthenticated ? profile.Login : "Войти через gh / Git";
        _githubProfileStatus.Foreground = profile.IsAuthenticated
            ? ThemeBrush("TealBrush", 89, 216, 200)
            : ThemeBrush("MutedTextBrush", 146, 152, 173);
        if (_githubProfileButton is not null)
        {
            ToolTipService.SetToolTip(_githubProfileButton, profile.IsAuthenticated
                ? $"GitHub: {profile.Login} ({profile.Provider})"
                : profile.Error ?? "Авторизовать GitHub");
        }
    }

    private async Task HandleGitHubProfileClickAsync()
    {
        var profile = await _githubAuth.GetProfileAsync();
        if (profile.IsAuthenticated)
        {
            var dialog = new ContentDialog
            {
                Title = "Профиль GitHub",
                Content = $"{profile.DisplayName}\n{profile.Login}\nАвторизация: {profile.Provider}",
                CloseButtonText = "Закрыть",
                XamlRoot = Content.XamlRoot,
            };
            await dialog.ShowAsync();
            return;
        }

        var loginDialog = new ContentDialog
        {
            Title = "Войти в GitHub",
            Content = "Выберите способ авторизации. gh откроет официальный веб-вход, Git CLI использует Git Credential Manager.",
            PrimaryButtonText = "Войти через gh",
            SecondaryButtonText = "Войти через Git CLI",
            CloseButtonText = "Отмена",
            XamlRoot = Content.XamlRoot,
        };
        var result = await loginDialog.ShowAsync();
        if (result == ContentDialogResult.None)
        {
            return;
        }

        var command = result == ContentDialogResult.Primary
            ? await _githubAuth.StartGhLoginAsync()
            : await _githubAuth.StartGitLoginAsync();
        if (command.ExitCode != 0)
        {
            var error = new ContentDialog
            {
                Title = "Не удалось начать авторизацию",
                Content = string.IsNullOrWhiteSpace(command.Stderr) ? "Проверьте, что gh или Git Credential Manager установлены." : command.Stderr.Trim(),
                CloseButtonText = "Закрыть",
                XamlRoot = Content.XamlRoot,
            };
            await error.ShowAsync();
        }

        await RefreshGitHubProfileAsync();
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
            var selectedModel = (_modelSelector.SelectedItem as string)?.Trim();
            var model = string.IsNullOrWhiteSpace(selectedModel)
                ? _configuredModel?.Trim() ?? string.Empty
                : selectedModel;

            var catalogMode = SelectedCatalogMode();
            _providerSettings.Save(new ProviderSettings(
                _providerBox.Text.Trim(),
                _baseUrlBox.Text.Trim(),
                model,
                _apiKeyBox.Password)
            {
                CatalogMode = catalogMode,
            });
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
                    ? (string.IsNullOrWhiteSpace(model)
                        ? "Ключ сохранён. Выберите модель из загруженного списка."
                        : "Сохранено. Модели загружены.")
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
        if (project is null)
        {
            _activeProjectId = null;
            _activeChatId = null;
            RefreshProjectSidebar();
            return;
        }
        _activeProjectId = project.Id;
        if (!_newChatRequested && _activeChatId is null)
        {
            _activeChatId = project.Chats.FirstOrDefault(chat => !chat.Archived)?.Id;
        }
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
            var projectRow = new Grid { ColumnSpacing = 2 };
            projectRow.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            projectRow.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
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
            projectRow.Children.Add(projectButton);
            var deleteButton = new Button
            {
                Content = "×",
                Tag = project,
                Width = 28,
                Padding = new Thickness(0),
                Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
                Foreground = ThemeBrush("MutedTextBrush", 146, 152, 173),
            };
            ToolTipService.SetToolTip(deleteButton, "Удалить проект из списка");
            deleteButton.Click += async (_, _) => await DeleteProjectAsync(project);
            Grid.SetColumn(deleteButton, 1);
            projectRow.Children.Add(deleteButton);
            _projectListPanel.Children.Add(projectRow);

            foreach (var chat in project.Chats.Where(chat => !chat.Archived).Take(8))
            {
                var chatRow = new Grid { ColumnSpacing = 2 };
                chatRow.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                chatRow.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
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
                chatRow.Children.Add(chatButton);
                var archiveButton = new Button
                {
                    Content = "▣",
                    Width = 28,
                    Padding = new Thickness(0),
                    Visibility = Visibility.Collapsed,
                    Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
                    Foreground = ThemeBrush("MutedTextBrush", 146, 152, 173),
                };
                ToolTipService.SetToolTip(archiveButton, "Архивировать чат");
                archiveButton.Click += async (_, _) => await ArchiveChatAsync(project, chat);
                Grid.SetColumn(archiveButton, 1);
                chatRow.Children.Add(archiveButton);
                chatRow.PointerEntered += (_, _) => archiveButton.Visibility = Visibility.Visible;
                chatRow.PointerExited += (_, _) => archiveButton.Visibility = Visibility.Collapsed;
                _projectListPanel.Children.Add(chatRow);
            }
        }
    }

    private Task ArchiveChatAsync(ProjectEntry project, ChatEntry chat)
    {
        if (_projectCatalogService.ArchiveChat(project, chat))
        {
            _projectCatalogService.Save(_projectCatalog);
            if (_activeChatId == chat.Id)
            {
                _activeChatId = null;
                _newChatRequested = true;
                ClearConversation();
                ConnectionStatus.Text = $"Проект: {project.Name}";
            }
            RefreshProjectSidebar();
        }
        return Task.CompletedTask;
    }

    private async Task DeleteProjectAsync(ProjectEntry project)
    {
        var dialog = new ContentDialog
        {
            Title = $"Удалить проект «{project.Name}»?",
            Content = "Проект и его чаты будут убраны из списка EvoHime. Файлы workspace на диске не удаляются.",
            PrimaryButtonText = "Удалить",
            CloseButtonText = "Отмена",
            XamlRoot = ((FrameworkElement)Content).XamlRoot,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }

        _projectCatalogService.RemoveProject(_projectCatalog, project);
        _projectCatalogService.Save(_projectCatalog);
        if (project.Id == _activeProjectId)
        {
            _activeProjectId = null;
            _activeChatId = null;
            _newChatRequested = true;
            var nextProject = _projectCatalog.Projects.FirstOrDefault();
            if (nextProject is not null)
            {
                _activeProjectId = nextProject.Id;
                _state.SelectWorkspace(nextProject.Path);
                await _settings.SaveWorkspaceAsync(nextProject.Path);
                WorkspacePathText.Text = $"Workspace: {_state.WorkspacePath}";
                ConnectionStatus.Text = $"Проект: {nextProject.Name}";
            }
            else
            {
                ConnectionStatus.Text = "Проекты отсутствуют. Выберите workspace.";
            }
        }
        RefreshProjectSidebar();
    }

    private async void SelectProject(ProjectEntry project)
    {
        _activeProjectId = project.Id;
        _activeChatId = null;
        _newChatRequested = true;
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
        _newChatRequested = false;
        _state.SelectWorkspace(project.Path);
        WorkspacePathText.Text = $"Workspace: {_state.WorkspacePath}";
        RefreshProjectSidebar();
        ShowHomeView();
        ClearConversation();
        AddConversationActivity($"Чат «{chat.Title}» выбран · новые сообщения появятся здесь");
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

            var mode = SelectedCatalogMode();
            using var requestTimeout = new CancellationTokenSource(TimeSpan.FromSeconds(30));
            await _ipc.RequestModelCatalogAsync(mode, requestTimeout.Token);
            var response = await _ipc.ReadEventAsync(requestTimeout.Token);
            if (response.EventType != "model.catalog")
            {
                throw new InvalidOperationException("Core не вернул каталог моделей.");
            }

            using var json = JsonDocument.Parse(response.Payload);
            var models = json.RootElement.TryGetProperty("models", out var modelsValue)
                ? modelsValue.EnumerateArray().Select(item => item.GetString() ?? string.Empty)
                : [];
            return ModelCatalogFilter.Filter(models, mode).ToList();
        }
        finally
        {
            _ipcRequestGate.Release();
        }
    }

    private string SelectedCatalogMode() =>
        _modelModeBox?.SelectedIndex == 1
            ? "paid"
            : "free";

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

    private void PromptBox_PreviewKeyDown(object sender, KeyRoutedEventArgs e)
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

    private void ClearConversation()
    {
        if (_conversationPanel is null)
        {
            return;
        }

        _conversationPanel.Children.Clear();
        _streamingAssistantText = null;
        AddConversationMessage("Ева", "Привет, хозяин. Опиши задачу — я разберусь и покажу, что делаю.", false);
    }

    private void AddConversationMessage(string speaker, string message, bool isUser)
    {
        if (_conversationPanel is null)
        {
            return;
        }

        var text = ThemeBrush("TextBrush", 244, 242, 250);
        var bubble = new StackPanel { Spacing = 6 };
        bubble.Children.Add(new TextBlock
        {
            Text = speaker,
            FontSize = 12,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            Foreground = isUser ? ThemeBrush("TealBrush", 89, 216, 200) : ThemeBrush("PurpleBrush", 167, 139, 250),
        });
        var body = new TextBlock
        {
            Text = message,
            TextWrapping = TextWrapping.Wrap,
            FontSize = 14,
            Foreground = text,
        };
        bubble.Children.Add(body);
        var copyButton = new Button
        {
            Content = "Копировать",
            FontSize = 11,
            Padding = new Thickness(5, 2, 5, 2),
            Foreground = ThemeBrush("MutedTextBrush", 146, 152, 173),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            HorizontalAlignment = HorizontalAlignment.Left,
        };
        ToolTipService.SetToolTip(copyButton, "Скопировать сообщение");
        copyButton.Click += (_, _) => CopyConversationMessage(body.Text);
        bubble.Children.Add(copyButton);
        _conversationPanel.Children.Add(new Border
        {
            Child = bubble,
            MaxWidth = 820,
            Padding = new Thickness(16, 12, 16, 13),
            CornerRadius = isUser ? new CornerRadius(14, 14, 4, 14) : new CornerRadius(14),
            Background = isUser ? ThemeBrush("PurpleBrush", 62, 53, 91) : ThemeBrush("SurfaceRaisedBrush", 34, 38, 53),
            BorderBrush = ThemeBrush("BorderBrush", 48, 53, 72),
            BorderThickness = new Thickness(1),
            HorizontalAlignment = isUser ? HorizontalAlignment.Right : HorizontalAlignment.Left,
        });
        ScrollConversationToBottom();
        if (!isUser)
        {
            _streamingAssistantText = body;
        }
    }

    private void CopyConversationMessage(string message)
    {
        if (string.IsNullOrEmpty(message))
        {
            return;
        }

        try
        {
            var package = new DataPackage();
            package.SetText(message);
            Clipboard.SetContent(package);
            ConnectionStatus.Text = "Сообщение скопировано";
        }
        catch (Exception error)
        {
            ConnectionStatus.Text = $"Не удалось скопировать сообщение: {error.Message}";
        }
    }

    private void CopyWholeConversation()
    {
        if (_conversationPanel is null)
        {
            return;
        }

        var lines = new List<string>();
        foreach (var child in _conversationPanel.Children.OfType<Border>())
        {
            if (child.Child is StackPanel messagePanel)
            {
                var texts = messagePanel.Children.OfType<TextBlock>()
                    .Select(item => item.Text)
                    .Where(item => !string.IsNullOrWhiteSpace(item))
                    .ToList();
                if (texts.Count >= 2)
                {
                    lines.Add($"{texts[0]}: {texts[1]}");
                }
            }
            else if (child.Child is TextBlock activity && !string.IsNullOrWhiteSpace(activity.Text))
            {
                lines.Add(activity.Text.Trim());
            }
        }

        CopyConversationMessage(string.Join(Environment.NewLine + Environment.NewLine, lines));
        ConnectionStatus.Text = lines.Count == 0 ? "Чат пока пуст" : "Весь чат скопирован";
    }

    private void CopyTrace()
    {
        if (_tracePanel is null)
        {
            return;
        }

        var lines = _tracePanel.Children
            .OfType<Border>()
            .Select(item => (item.Child as TextBlock)?.Text)
            .Where(item => !string.IsNullOrWhiteSpace(item))
            .Cast<string>()
            .ToList();
        CopyConversationMessage(string.Join(Environment.NewLine + Environment.NewLine, lines));
        ConnectionStatus.Text = lines.Count == 0 ? "Trace пока пуст" : "Trace скопирован";
    }

    private void OpenTraceFolder()
    {
        var logs = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "EvoHime",
            "logs");
        Directory.CreateDirectory(logs);
        Process.Start(new ProcessStartInfo
        {
            FileName = "explorer.exe",
            Arguments = $"/select,\"{Path.Combine(logs, "model-trace.jsonl")}\"",
            UseShellExecute = true,
        });
        ConnectionStatus.Text = $"Trace: {Path.Combine(logs, "model-trace.jsonl")}";
    }

    private void AddConversationActivity(string message)
    {
        if (_conversationPanel is null)
        {
            return;
        }

        _streamingAssistantText = null;
        _conversationPanel.Children.Add(new Border
        {
            Background = ThemeBrush("SurfaceBrush", 25, 28, 39),
            BorderBrush = ThemeBrush("BorderBrush", 48, 53, 72),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(10),
            Padding = new Thickness(12, 8, 12, 8),
            HorizontalAlignment = HorizontalAlignment.Left,
            Child = new TextBlock
            {
                Text = $"·  {message}",
                FontSize = 12,
                Foreground = ThemeBrush("MutedTextBrush", 146, 152, 173),
                TextWrapping = TextWrapping.Wrap,
            },
        });
        ScrollConversationToBottom();
    }

    private void AddTraceLine(string message, bool important = false)
    {
        if (_tracePanel is null)
        {
            return;
        }

        _tracePanel.Children.Add(new Border
        {
            Background = important
                ? ThemeBrush("SurfaceRaisedBrush", 34, 38, 53)
                : ThemeBrush("SurfaceBrush", 21, 24, 33),
            BorderBrush = ThemeBrush("BorderBrush", 48, 53, 72),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(9, 7, 9, 7),
            Child = new TextBlock
            {
                Text = message,
                TextWrapping = TextWrapping.Wrap,
                FontSize = 11,
                Foreground = important
                    ? ThemeBrush("TextBrush", 244, 242, 250)
                    : ThemeBrush("MutedTextBrush", 146, 152, 173),
            },
        });
        _ = DispatcherQueue.TryEnqueue(() => _traceScroll?.ChangeView(null, double.MaxValue, null));
    }

    private void AppendAssistantDelta(string content)
    {
        if (string.IsNullOrEmpty(content))
        {
            return;
        }

        if (_streamingAssistantText is null)
        {
            AddConversationMessage("Ева", content, false);
        }
        else
        {
            _streamingAssistantText.Text += content;
            ScrollConversationToBottom();
        }
    }

    private void RenderConversationEvent(CoreEventEnvelope envelope)
    {
        try
        {
            using var document = JsonDocument.Parse(envelope.Payload);
            var root = document.RootElement;
            // Старые записи журнала сериализованы с именем варианта enum
            // (например, {"TaskCompleted": {...}}), новые идут плоским JSON.
            // Поддерживаем оба формата, чтобы итог задачи не терялся.
            if (root.ValueKind == JsonValueKind.Object && root.EnumerateObject().Count() == 1)
            {
                var wrapped = root.EnumerateObject().First().Value;
                if (wrapped.ValueKind == JsonValueKind.Object)
                {
                    root = wrapped;
                }
            }
            switch (envelope.EventType)
            {
                case "task.started":
                    AddConversationActivity("Ева отправила запрос модели, ожидаю ответ…");
                    break;
                case "model.context":
                    UpdateModelContext(root);
                    var toolCount = root.TryGetProperty("tools", out var contextTools) && contextTools.ValueKind == JsonValueKind.Array
                        ? contextTools.GetArrayLength()
                        : 0;
                    var userPrompt = PayloadString(root, "user_prompt");
                    AddTraceLine($"Контекст подготовлен\nМодель: {PayloadString(root, "model")}\nWorkspace: {PayloadString(root, "workspace_path")}\nИнструментов: {toolCount}\nЗадача: {(string.IsNullOrWhiteSpace(userPrompt) ? "(не передана)" : TrimTrace(userPrompt))}", true);
                    break;
                case "agent.message.delta":
                    AppendAssistantDelta(PayloadString(root, "content"));
                    break;
                case "tool.started":
                    AddConversationActivity($"Ева использует инструмент: {PayloadString(root, "tool_name")}");
                    AddTraceLine($"→ tool.started\n{PayloadString(root, "tool_name")}");
                    break;
                case "tool.output":
                    AddConversationActivity($"Инструмент завершён: {PayloadString(root, "tool_name")}");
                    AddTraceLine($"← tool.output\n{PayloadString(root, "tool_name")}\n{TrimTrace(PayloadString(root, "output"))}");
                    break;
                case "task.completed":
                    var finalMessage = PayloadString(root, "final_message");
                    if (!string.IsNullOrWhiteSpace(finalMessage) && _streamingAssistantText is null)
                    {
                        AddConversationMessage("Ева", finalMessage, false);
                    }
                    else if (string.IsNullOrWhiteSpace(finalMessage) && _streamingAssistantText is null)
                    {
                        AddConversationMessage("Ева", "Задача завершена, но итоговый ответ от модели не пришёл.", false);
                    }
                    AddConversationActivity("Задача завершена");
                    AddTraceLine($"✓ task.completed\n{TrimTrace(finalMessage)}", true);
                    _streamingAssistantText = null;
                    CompleteTaskUi("Задача завершена");
                    break;
                case "task.failed":
                    AddConversationMessage("Ева", $"Задача завершилась с ошибкой: {PayloadString(root, "error")}", false);
                    AddTraceLine($"✕ task.failed\n{TrimTrace(PayloadString(root, "error"))}", true);
                    _streamingAssistantText = null;
                    CompleteTaskUi("Задача завершилась с ошибкой");
                    break;
                case "task.stopped":
                    AddConversationActivity("Задача остановлена");
                    _streamingAssistantText = null;
                    CompleteTaskUi("Задача остановлена");
                    break;
            }
        }
        catch (JsonException)
        {
            AddConversationActivity(NativeEventFormatter.Format(envelope));
        }
    }

    private static string PayloadString(JsonElement root, string property) =>
        root.TryGetProperty(property, out var value) ? value.GetString() ?? string.Empty : string.Empty;

    private static string TrimTrace(string value) =>
        value.Length <= 900 ? value : value[..900] + "…";

    private void UpdateModelContext(JsonElement root)
    {
        var workspace = PayloadString(root, "workspace_path");
        var model = PayloadString(root, "model");
        var systemPrompt = PayloadString(root, "system_prompt");
        var userPrompt = PayloadString(root, "user_prompt");
        var estimatedTokens = root.TryGetProperty("estimated_tokens", out var tokenValue)
            ? tokenValue.GetInt32().ToString()
            : "неизвестно";
        var contextLimit = root.TryGetProperty("context_limit_tokens", out var limitValue)
            ? limitValue.GetInt32()
            : 128000;
        var tools = root.TryGetProperty("tools", out var toolsValue) && toolsValue.ValueKind == JsonValueKind.Array
            ? string.Join(", ", toolsValue.EnumerateArray().Select(item => item.GetString()).Where(item => !string.IsNullOrWhiteSpace(item)))
            : "нет инструментов";

        var estimated = int.TryParse(estimatedTokens, out var parsedTokens) ? parsedTokens : 0;
        var percent = contextLimit > 0 ? Math.Clamp((double)estimated / contextLimit * 100, 0, 100) : 0;
        _modelContextDetails = $"Модель: {model}\nWorkspace: {workspace}\nКонтекст: ~{estimatedTokens} из {contextLimit:N0} токенов ({percent:0}%)\n\nСистемная инструкция:\n{systemPrompt}\n\nЗапрос:\n{userPrompt}\n\nДоступные инструменты ({(tools == "нет инструментов" ? 0 : tools.Split(", ").Length)}):\n{tools}";
        UpdateContextProgress(percent);
        if (_contextButton is not null)
        {
            _contextButton.IsEnabled = true;
        }
    }

    private void UpdateContextProgress(double percent)
    {
        if (_contextPercentText is not null)
        {
            _contextPercentText.Text = $"{percent:0}%";
        }

        if (_contextProgressArc is null)
        {
            return;
        }

        if (percent <= 0)
        {
            _contextProgressArc.Data = null;
            return;
        }

        var radius = 17d;
        var center = 21d;
        var start = -Math.PI / 2;
        var angle = start + Math.Min(percent, 99.99) / 100d * Math.PI * 2;
        var end = new Windows.Foundation.Point(center + radius * Math.Cos(angle), center + radius * Math.Sin(angle));
        var figure = new XamlPathFigure
        {
            StartPoint = new Windows.Foundation.Point(center, center - radius),
            IsClosed = false,
        };
        figure.Segments.Add(new XamlArcSegment
        {
            Point = end,
            Size = new Windows.Foundation.Size(radius, radius),
            IsLargeArc = percent > 50,
            SweepDirection = SweepDirection.Clockwise,
        });
        var geometry = new XamlPathGeometry();
        geometry.Figures.Add(figure);
        _contextProgressArc.Data = geometry;
    }

    private async void ContextButton_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new ContentDialog
        {
            Title = "Контекст модели",
            Content = new ScrollViewer
            {
                MaxHeight = 520,
                Content = new TextBlock
                {
                    Text = _modelContextDetails,
                    TextWrapping = TextWrapping.Wrap,
                    IsTextSelectionEnabled = true,
                },
            },
            CloseButtonText = "Закрыть",
            XamlRoot = (Content as FrameworkElement)?.XamlRoot,
        };
        await dialog.ShowAsync();
    }

    private void ScrollConversationToBottom() =>
        _ = DispatcherQueue.TryEnqueue(() => _conversationScroll?.ChangeView(null, double.MaxValue, null));

    private void CompleteTaskUi(string status)
    {
        _activeTaskId = null;
        StartButton.IsEnabled = true;
        StopButton.IsEnabled = false;
        ConnectionStatus.Text = status;
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
        AddPermissionMenuItem(menu, button, "ask", "Доступ: спрашивать");
        AddPermissionMenuItem(menu, button, "read_only", "Доступ: только чтение");
        AddPermissionMenuItem(menu, button, "full", "Доступ: полный");
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
            models = ModelCatalogFilter.Filter(models, mode).ToList();
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

        AddConversationMessage("Вы", prompt, true);
        PromptBox.Text = string.Empty;
        _streamingAssistantText = null;
        if (_contextButton is not null)
        {
            _contextButton.IsEnabled = false;
        }
        UpdateContextProgress(0);
        AddConversationActivity("Ева подключается к Core…");

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
                _newChatRequested = false;
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
            if (_eventCts is null)
            {
                _eventCts = new CancellationTokenSource();
                _ = PumpEventsAsync(_eventCts.Token);
            }
            StartButton.IsEnabled = false;
            StopButton.IsEnabled = true;
        }
        catch (Exception error)
        {
            ConnectionStatus.Text = $"Ошибка IPC: {error.Message}";
            AddConversationMessage("Ева", $"Не удалось начать задачу: {error.Message}", false);
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
        while (!cancellationToken.IsCancellationRequested)
        {
            try
            {
                if (!_ipc.IsConnected)
                {
                    SetConnectionStatus("Восстановление IPC...");
                    await _ipc.ConnectAndHandshakeAsync(cancellationToken);
                    _reconnectAttempt = 0;
                }

                var recoveryBlocked = false;
                var reconciliationCompleted = false;
                SetConnectionStatus("RECOVERING: replay durable state...");
                await _ipc.ReadReplayAsync(
                    _state.LastSequence,
                    envelope =>
                    {
                        if (_state.ApplyEvent(envelope))
                        {
                            if (envelope.EventType == "run.recovery.blocked")
                            {
                                recoveryBlocked = true;
                                _recoveryOutcomeStatus = "BLOCKED: recovery требует проверки";
                                SetConnectionStatus("BLOCKED: unknown effect остановлен после recovery");
                            }
                            if (envelope.EventType == "run.reconciliation.completed")
                            {
                                reconciliationCompleted = true;
                                _recoveryOutcomeStatus = "RESUMABLE: outcome подтверждён reconciliation";
                                SetConnectionStatus(_recoveryOutcomeStatus);
                            }
                            if (envelope.TaskId == _activeTaskId)
                            {
                                _ = DispatcherQueue.TryEnqueue(() => RenderConversationEvent(envelope));
                            }
                            if (envelope.EventType == "approval.required")
                            {
                                ShowApproval(envelope);
                            }
                            if (envelope.TaskId == _activeTaskId || envelope.EventType is "run.recovery.blocked" or "run.reconciliation.completed")
                            {
                                var notification = envelope.EventType switch
                                {
                                    "task.completed" => ("Задача завершена", "EvoHime завершила задачу."),
                                    "task.failed" => ("Задача завершилась с ошибкой", "Проверьте журнал событий EvoHime."),
                                    "task.stopped" => ("Задача остановлена", "Выполнение остановлено пользователем."),
                                    "run.recovery.blocked" => ("Run заблокирован после восстановления", "Неизвестный effect не был запущен повторно; требуется проверка."),
                                    "run.reconciliation.completed" => ("Run подтверждён после восстановления", "Durable snapshot подтвердил результат; повторный effect не запускался."),
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
                SetConnectionStatus(recoveryBlocked
                    ? "BLOCKED: recovery требует проверки"
                    : reconciliationCompleted
                        ? "RESUMABLE: outcome подтверждён reconciliation"
                        : _recoveryOutcomeStatus ?? "Подключено");
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
