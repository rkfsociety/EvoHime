using System.Text.Json;

namespace EvoHime.Desktop.Services;

/// <summary>
/// Запоминает последнюю папку, из которой пользователь выбирал файлы, чтобы диалог
/// открывался там же в следующий раз. Ключ разделяет сценарии (вложения, план и т.п.).
/// </summary>
public sealed class RecentFolderService
{
    public const string AttachmentsKey = "attachments";

    private readonly JsonSerializerOptions _jsonOptions = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true,
    };

    private readonly object _sync = new();

    public RecentFolderService()
        : this(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "EvoHime",
            "recent-folders.json"))
    {
    }

    public RecentFolderService(string filePath)
    {
        FilePath = Path.GetFullPath(filePath);
    }

    public string FilePath { get; }

    /// <summary>Возвращает последнюю папку для сценария или null, если её нет или она удалена.</summary>
    public string? Get(string key)
    {
        lock (_sync)
        {
            var folder = Read().GetValueOrDefault(key);
            return !string.IsNullOrWhiteSpace(folder) && Directory.Exists(folder) ? folder : null;
        }
    }

    /// <summary>Сохраняет папку выбранного файла как стартовую для следующего вызова диалога.</summary>
    public void RememberFile(string key, string filePath)
    {
        var folder = Path.GetDirectoryName(Path.GetFullPath(filePath));
        if (!string.IsNullOrWhiteSpace(folder))
        {
            Remember(key, folder);
        }
    }

    public void Remember(string key, string folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath))
        {
            return;
        }

        lock (_sync)
        {
            var folders = Read();
            folders[key] = Path.GetFullPath(folderPath);
            Write(folders);
        }
    }

    private Dictionary<string, string> Read()
    {
        if (!File.Exists(FilePath))
        {
            return [];
        }

        try
        {
            return JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText(FilePath), _jsonOptions)
                ?? [];
        }
        catch (Exception exception) when (exception is JsonException or IOException or UnauthorizedAccessException)
        {
            return [];
        }
    }

    private void Write(Dictionary<string, string> folders)
    {
        try
        {
            var directory = Path.GetDirectoryName(FilePath);
            if (!string.IsNullOrEmpty(directory))
            {
                Directory.CreateDirectory(directory);
            }

            File.WriteAllText(FilePath, JsonSerializer.Serialize(folders, _jsonOptions));
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            // Потеря последней папки не должна ломать выбор файлов.
        }
    }
}
