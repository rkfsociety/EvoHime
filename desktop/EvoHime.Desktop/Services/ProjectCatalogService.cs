using System.Text.Json;

namespace EvoHime.Desktop.Services;

public sealed class ProjectCatalog
{
    public List<ProjectEntry> Projects { get; set; } = [];
}

public sealed class ProjectEntry
{
    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public string Path { get; set; } = string.Empty;
    public List<ChatEntry> Chats { get; set; } = [];
}

public sealed class ChatEntry
{
    public string Id { get; set; } = string.Empty;
    public string Title { get; set; } = string.Empty;
    public DateTimeOffset UpdatedAt { get; set; }
    public bool Archived { get; set; }
}

public sealed class ProjectCatalogService
{
    private readonly JsonSerializerOptions _jsonOptions = new(JsonSerializerDefaults.Web)
    {
        WriteIndented = true,
    };

    public ProjectCatalogService()
        : this(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "EvoHime",
            "projects.json"))
    {
    }

    public ProjectCatalogService(string filePath)
    {
        FilePath = Path.GetFullPath(filePath);
    }

    public string FilePath { get; }

    public ProjectCatalog Load()
    {
        if (!File.Exists(FilePath))
        {
            return new ProjectCatalog();
        }

        try
        {
            var catalog = JsonSerializer.Deserialize<ProjectCatalog>(File.ReadAllText(FilePath), _jsonOptions)
                ?? new ProjectCatalog();
            catalog.Projects ??= [];
            catalog.Projects = catalog.Projects
                .Where(project => !IsTechnicalProjectPath(project.Path))
                .ToList();
            foreach (var project in catalog.Projects)
            {
                project.Chats ??= [];
            }
            return catalog;
        }
        catch (JsonException)
        {
            return new ProjectCatalog();
        }
        catch (IOException)
        {
            return new ProjectCatalog();
        }
    }

    public void Save(ProjectCatalog catalog)
    {
        var directory = Path.GetDirectoryName(FilePath);
        if (directory is not null)
        {
            Directory.CreateDirectory(directory);
        }

        var temporaryPath = $"{FilePath}.{Guid.NewGuid():N}.tmp";
        try
        {
            File.WriteAllText(temporaryPath, JsonSerializer.Serialize(catalog, _jsonOptions));
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

    public static bool IsTechnicalProjectPath(string path)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return false;
        }

        var fullPath = Path.GetFullPath(path)
            .TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        return fullPath
            .Split([Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar], StringSplitOptions.RemoveEmptyEntries)
            .Any(part => string.Equals(part, ".evohime-native", StringComparison.OrdinalIgnoreCase));
    }

    public ProjectEntry? EnsureProject(ProjectCatalog catalog, string path)
    {
        if (IsTechnicalProjectPath(path))
        {
            return null;
        }

        var fullPath = System.IO.Path.GetFullPath(path);
        var project = catalog.Projects.FirstOrDefault(item =>
            string.Equals(item.Path, fullPath, StringComparison.OrdinalIgnoreCase));
        if (project is not null)
        {
            return project;
        }

        project = new ProjectEntry
        {
            Id = Guid.NewGuid().ToString("N"),
            Name = new DirectoryInfo(fullPath).Name,
            Path = fullPath,
        };
        catalog.Projects.Insert(0, project);
        return project;
    }

    public ChatEntry AddChat(ProjectEntry project, string title)
    {
        var chat = new ChatEntry
        {
            Id = Guid.NewGuid().ToString("N"),
            Title = string.IsNullOrWhiteSpace(title) ? "Новый чат" : title,
            UpdatedAt = DateTimeOffset.Now,
        };
        project.Chats.Insert(0, chat);
        return chat;
    }

    public bool RemoveProject(ProjectCatalog catalog, ProjectEntry project) =>
        catalog.Projects.Remove(project);

    public bool ArchiveChat(ProjectEntry project, ChatEntry chat)
    {
        if (!project.Chats.Contains(chat))
        {
            return false;
        }

        chat.Archived = true;
        return true;
    }
}
