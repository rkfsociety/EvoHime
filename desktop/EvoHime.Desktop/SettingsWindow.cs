using System.Diagnostics;
using EvoHime.Desktop.Services;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace EvoHime.Desktop;

public sealed class SettingsWindow : Window
{
    private readonly WorkspaceSettings _workspaceSettings = new();
    private readonly TextBlock _workspaceText;
    private readonly string _dataDirectory;

    public event Action<string>? WorkspaceChanged;

    public SettingsWindow(string workspacePath, string modelLabel)
    {
        _dataDirectory = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "EvoHime");

        var text = Brush("TextBrush", 247, 244, 245);
        var muted = Brush("MutedTextBrush", 143, 146, 157);
        var surface = Brush("SurfaceBrush", 16, 20, 27);
        var raised = Brush("SurfaceRaisedBrush", 23, 28, 37);
        var root = new Grid
        {
            Background = Brush("NightBackgroundBrush", 9, 11, 16),
            Padding = new Thickness(28),
        };
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        var header = new StackPanel { Spacing = 5, Margin = new Thickness(0, 0, 0, 22) };
        header.Children.Add(new TextBlock
        {
            Text = "Настройки",
            FontSize = 28,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            Foreground = text,
        });
        header.Children.Add(new TextBlock
        {
            Text = "Конфигурация локального агента Евы",
            FontSize = 14,
            Foreground = muted,
        });
        root.Children.Add(header);

        var sections = new StackPanel { Spacing = 14 };
        sections.Children.Add(CreateSection(
            "Модель и провайдер",
            "Активная конфигурация приходит от Core через IPC.",
            new TextBlock
            {
                Text = modelLabel,
                FontSize = 16,
                Foreground = Brush("TealBrush", 255, 59, 95),
            },
            raised));

        _workspaceText = new TextBlock
        {
            Text = workspacePath,
            TextWrapping = TextWrapping.Wrap,
            Foreground = text,
            FontSize = 14,
        };
        var workspaceActions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10, Margin = new Thickness(0, 12, 0, 0) };
        var chooseWorkspace = new Button { Content = "Изменить workspace" };
        chooseWorkspace.Click += ChooseWorkspace_Click;
        workspaceActions.Children.Add(chooseWorkspace);
        sections.Children.Add(CreateSection(
            "Рабочее пространство",
            "Папка, в которой Ева выполняет задачи.",
            new StackPanel { Children = { _workspaceText, workspaceActions } },
            surface));

        var runtime = new StackPanel { Spacing = 7 };
        runtime.Children.Add(StatusLine("Core", "Внутренний процесс агента", "Запускается вместе с клиентом", text, muted));
        runtime.Children.Add(StatusLine("IPC", "Связь с Core", "Versioned named pipe", text, muted));
        runtime.Children.Add(StatusLine("Данные", "Локальное хранилище", _dataDirectory, text, muted));
        var diagnosticsButton = new Button { Content = "Открыть папку диагностики", Margin = new Thickness(0, 8, 0, 0) };
        diagnosticsButton.Click += (_, _) => OpenDiagnostics();
        runtime.Children.Add(diagnosticsButton);
        sections.Children.Add(CreateSection("Состояние и диагностика", "Служебная информация приложения.", runtime, raised));

        var scroll = new ScrollViewer { Content = sections, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        Grid.SetRow(scroll, 1);
        root.Children.Add(scroll);

        var close = new Button { Content = "Готово", HorizontalAlignment = HorizontalAlignment.Right, Margin = new Thickness(0, 18, 0, 0) };
        close.Click += (_, _) => Close();
        Grid.SetRow(close, 2);
        root.Children.Add(close);
        Content = root;
    }

    private static Border CreateSection(string title, string description, UIElement content, Brush background) =>
        new()
        {
            Background = background,
            BorderBrush = Brush("BorderBrush", 68, 32, 43),
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(12),
            Padding = new Thickness(18),
            Child = new StackPanel
            {
                Spacing = 7,
                Children =
                {
                    new TextBlock { Text = title, FontSize = 16, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Foreground = Brush("TextBrush", 247, 244, 245) },
                    new TextBlock { Text = description, FontSize = 12, Foreground = Brush("MutedTextBrush", 143, 146, 157) },
                    content,
                },
            },
        };

    private static StackPanel StatusLine(string title, string description, string value, Brush text, Brush muted)
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

    private async void ChooseWorkspace_Click(object sender, RoutedEventArgs e)
    {
        var picker = new FolderPicker();
        picker.FileTypeFilter.Add("*");
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var folder = await picker.PickSingleFolderAsync();
        if (folder is null)
        {
            return;
        }

        await _workspaceSettings.SaveWorkspaceAsync(folder.Path);
        _workspaceText.Text = folder.Path;
        WorkspaceChanged?.Invoke(folder.Path);
    }

    private void OpenDiagnostics()
    {
        var logsPath = Path.Combine(_dataDirectory, "logs");
        Directory.CreateDirectory(logsPath);
        Process.Start(new ProcessStartInfo
        {
            FileName = "explorer.exe",
            Arguments = $"\"{logsPath}\"",
            UseShellExecute = true,
        });
    }

    private static SolidColorBrush Brush(string key, byte r, byte g, byte b) =>
        Application.Current.Resources.TryGetValue(key, out var value) && value is SolidColorBrush brush
            ? brush
            : new SolidColorBrush(Windows.UI.Color.FromArgb(255, r, g, b));
}
