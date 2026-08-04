using System.Text.Json;

namespace EvoHime.Desktop.Services;

public sealed class WorkspaceSettings
{
    private sealed record SettingsDocument(string? WorkspacePath);

    public WorkspaceSettings()
        : this(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "EvoHime",
            "settings.json"))
    {
    }

    public WorkspaceSettings(string filePath)
    {
        FilePath = Path.GetFullPath(filePath);
    }

    public string FilePath { get; }

    public async Task SaveWorkspaceAsync(string path, CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        var directory = Path.GetDirectoryName(FilePath);
        if (directory is not null)
        {
            Directory.CreateDirectory(directory);
        }

        var temporaryPath = $"{FilePath}.{Guid.NewGuid():N}.tmp";
        var json = JsonSerializer.Serialize(new SettingsDocument(Path.GetFullPath(path)));
        try
        {
            await File.WriteAllTextAsync(temporaryPath, json, cancellationToken);
            File.Move(temporaryPath, FilePath, overwrite: true);
        }
        finally
        {
            if (File.Exists(temporaryPath))
            {
                File.Delete(temporaryPath);
            }
        }
    }

    public async Task<string?> LoadWorkspaceAsync(CancellationToken cancellationToken = default)
    {
        if (!File.Exists(FilePath))
        {
            return null;
        }

        try
        {
            var json = await File.ReadAllTextAsync(FilePath, cancellationToken);
            var document = JsonSerializer.Deserialize<SettingsDocument>(json);
            return string.IsNullOrWhiteSpace(document?.WorkspacePath)
                ? null
                : document.WorkspacePath;
        }
        catch (JsonException)
        {
            return null;
        }
        catch (IOException)
        {
            return null;
        }
    }
}
