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
    string? InstalledPath);

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
        var search = string.IsNullOrWhiteSpace(query) ? "agent plugin" : query.Trim();
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

            var folder = Path.Combine(_pluginsDirectory, SafeFolderName(fullName));
            result.Add(new GitHubPlugin(
                fullName,
                item.GetProperty("name").GetString() ?? fullName,
                item.GetProperty("html_url").GetString() ?? $"https://github.com/{fullName}",
                item.GetProperty("clone_url").GetString() ?? $"https://github.com/{fullName}.git",
                item.GetProperty("description").GetString() ?? "Описание отсутствует.",
                item.GetProperty("stargazers_count").GetInt32(),
                Directory.Exists(folder),
                Directory.Exists(folder) ? folder : null));
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
