using System.Diagnostics;
using System.Net.Http.Headers;
using System.Text.Json;

namespace EvoHime.Desktop.Services;

public sealed record GitHubPlugin(
    string FullName,
    string Name,
    string HtmlUrl,
    string CloneUrl,
    string Description,
    int Stars,
    bool Installed,
    string? InstalledPath,
    PluginManifest Manifest);

public sealed record PluginManifest(
    string Schema,
    string Name,
    string Version,
    string Description,
    string? Homepage,
    bool HasSkills,
    bool HasMcp);

public sealed class PluginCatalogService
{
    private static readonly HttpClient Http = CreateHttpClient();
    private readonly string _pluginsDirectory;

    public PluginCatalogService()
        : this(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "EvoHime",
            "plugins"))
    {
    }

    public PluginCatalogService(string pluginsDirectory)
    {
        _pluginsDirectory = Path.GetFullPath(pluginsDirectory);
    }

    public string PluginsDirectory => _pluginsDirectory;

    public async Task<IReadOnlyList<GitHubPlugin>> SearchAsync(
        string query,
        CancellationToken cancellationToken = default)
    {
        var search = string.IsNullOrWhiteSpace(query) ? "agent plugins" : query.Trim();
        var url = $"https://api.github.com/search/repositories?q={Uri.EscapeDataString(search)}&sort=stars&order=desc&per_page=30";
        using var response = await Http.GetAsync(url, cancellationToken);
        response.EnsureSuccessStatusCode();
        await using var stream = await response.Content.ReadAsStreamAsync(cancellationToken);
        using var document = await JsonDocument.ParseAsync(stream, cancellationToken: cancellationToken);
        var result = new List<GitHubPlugin>();
        if (!document.RootElement.TryGetProperty("items", out var items))
        {
            return result;
        }

        foreach (var item in items.EnumerateArray())
        {
            var fullName = item.GetProperty("full_name").GetString() ?? string.Empty;
            if (string.IsNullOrWhiteSpace(fullName))
            {
                continue;
            }

            var manifest = await ReadGitHubManifestAsync(fullName, cancellationToken);
            if (manifest is null)
            {
                continue;
            }

            var folder = Path.Combine(_pluginsDirectory, SafeFolderName(fullName));
            result.Add(new GitHubPlugin(
                fullName,
                item.GetProperty("name").GetString() ?? fullName,
                item.GetProperty("html_url").GetString() ?? $"https://github.com/{fullName}",
                item.GetProperty("clone_url").GetString() ?? $"https://github.com/{fullName}.git",
                item.GetProperty("description").GetString() ?? "Описание отсутствует.",
                item.GetProperty("stargazers_count").GetInt32(),
                Directory.Exists(folder),
                Directory.Exists(folder) ? folder : null,
                manifest));
        }

        return result;
    }

    public async Task<string> InstallAsync(GitHubPlugin plugin, CancellationToken cancellationToken = default)
    {
        Directory.CreateDirectory(_pluginsDirectory);
        var target = Path.Combine(_pluginsDirectory, SafeFolderName(plugin.FullName));
        EnsureInsidePluginsDirectory(target);
        if (Directory.Exists(target))
        {
            return target;
        }

        var startInfo = new ProcessStartInfo
        {
            FileName = "git",
            WorkingDirectory = _pluginsDirectory,
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            CreateNoWindow = true,
        };
        startInfo.ArgumentList.Add("clone");
        startInfo.ArgumentList.Add("--depth");
        startInfo.ArgumentList.Add("1");
        startInfo.ArgumentList.Add(plugin.CloneUrl);
        startInfo.ArgumentList.Add(target);
        using var process = Process.Start(startInfo)
            ?? throw new InvalidOperationException("Не удалось запустить git для установки плагина.");
        await process.WaitForExitAsync(cancellationToken);
        if (process.ExitCode != 0)
        {
            var error = await process.StandardError.ReadToEndAsync(cancellationToken);
            if (Directory.Exists(target))
            {
                Directory.Delete(target, recursive: true);
            }
            throw new InvalidOperationException($"git clone завершился с ошибкой: {error.Trim()}");
        }

        if (ReadLocalManifest(target) is null)
        {
            Directory.Delete(target, recursive: true);
            throw new InvalidOperationException("Репозиторий не является Agent Plugin: отсутствует корректный корневой plugin.json.");
        }

        return target;
    }

    public void Uninstall(GitHubPlugin plugin)
    {
        var target = Path.Combine(_pluginsDirectory, SafeFolderName(plugin.FullName));
        EnsureInsidePluginsDirectory(target);
        if (Directory.Exists(target))
        {
            Directory.Delete(target, recursive: true);
        }
    }

    private static HttpClient CreateHttpClient()
    {
        var client = new HttpClient();
        client.DefaultRequestHeaders.UserAgent.Add(new ProductInfoHeaderValue("EvoHime", "0.0.0001"));
        client.DefaultRequestHeaders.Accept.Add(new MediaTypeWithQualityHeaderValue("application/vnd.github+json"));
        return client;
    }

    private static string SafeFolderName(string fullName) =>
        fullName.Replace('/', '_').Replace('\\', '_');

    private async Task<PluginManifest?> ReadGitHubManifestAsync(string fullName, CancellationToken cancellationToken)
    {
        try
        {
            using var response = await Http.GetAsync(
                $"https://api.github.com/repos/{fullName}/contents/plugin.json",
                cancellationToken);
            if (!response.IsSuccessStatusCode)
            {
                return null;
            }

            await using var stream = await response.Content.ReadAsStreamAsync(cancellationToken);
            using var document = await JsonDocument.ParseAsync(stream, cancellationToken: cancellationToken);
            var encoded = document.RootElement.GetProperty("content").GetString()?.Replace("\n", string.Empty);
            return encoded is null ? null : ParseManifest(System.Text.Encoding.UTF8.GetString(Convert.FromBase64String(encoded)), string.Empty);
        }
        catch (JsonException)
        {
            return null;
        }
        catch (FormatException)
        {
            return null;
        }
        catch (HttpRequestException)
        {
            return null;
        }
    }

    private static PluginManifest? ReadLocalManifest(string root)
    {
        var manifestPath = Path.Combine(root, "plugin.json");
        if (!File.Exists(manifestPath))
        {
            return null;
        }

        try
        {
            return ParseManifest(File.ReadAllText(manifestPath), root);
        }
        catch (JsonException)
        {
            return null;
        }
    }

    private static PluginManifest? ParseManifest(string json, string root)
    {
        using var document = JsonDocument.Parse(json);
        var objectRoot = document.RootElement;
        if (objectRoot.ValueKind != JsonValueKind.Object ||
            !objectRoot.TryGetProperty("$schema", out var schema) ||
            !objectRoot.TryGetProperty("name", out var name))
        {
            return null;
        }

        var schemaValue = schema.GetString();
        var nameValue = name.GetString();
        if (schemaValue != "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json" ||
            string.IsNullOrWhiteSpace(nameValue) ||
            nameValue.Length > 64 ||
            !System.Text.RegularExpressions.Regex.IsMatch(nameValue, "^[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?$") ||
            nameValue.Contains("..", StringComparison.Ordinal) ||
            nameValue.Contains("--", StringComparison.Ordinal))
        {
            return null;
        }

        var hasSkills = root.Length > 0 && Directory.Exists(Path.Combine(root, "skills"));
        var hasMcp = root.Length > 0 && File.Exists(Path.Combine(root, "mcp.json"));
        if (root.Length > 0 && !hasSkills && !hasMcp)
        {
            return null;
        }

        return new PluginManifest(
            schemaValue,
            nameValue,
            objectRoot.TryGetProperty("version", out var version) ? version.GetString() ?? "" : "",
            objectRoot.TryGetProperty("description", out var description) ? description.GetString() ?? "" : "",
            objectRoot.TryGetProperty("homepage", out var homepage) ? homepage.GetString() : null,
            hasSkills,
            hasMcp);
    }

    private void EnsureInsidePluginsDirectory(string path)
    {
        var root = _pluginsDirectory.TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar;
        var fullPath = Path.GetFullPath(path);
        if (!fullPath.StartsWith(root, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException("Путь плагина выходит за пределы каталога EvoHime.");
        }
    }
}
