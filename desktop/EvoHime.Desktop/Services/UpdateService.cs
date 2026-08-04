using System.Diagnostics;
using System.Security.Cryptography;
using System.Net.Http.Headers;
using System.Reflection;
using System.Text.Json;

namespace EvoHime.Desktop.Services;

public sealed record UpdateInfo(string Version, Uri InstallerUri, string TagName, string Sha256);

public sealed class UpdateService
{
    private const string Repository = "rkfsociety/EvoHime";
    private readonly HttpClient _httpClient;

    public static string CurrentVersion =>
        typeof(UpdateService).Assembly
            .GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion
            ?.Split('+', 2)[0] ?? "0.0.0001";

    public UpdateService(HttpClient? httpClient = null)
    {
        _httpClient = httpClient ?? new HttpClient();
        _httpClient.DefaultRequestHeaders.UserAgent.Add(new ProductInfoHeaderValue("EvoHime", CurrentVersion));
        _httpClient.DefaultRequestHeaders.Accept.Add(new MediaTypeWithQualityHeaderValue("application/vnd.github+json"));
    }

    public async Task<UpdateInfo?> CheckLatestAsync(string currentVersion, CancellationToken cancellationToken)
    {
        using var response = await _httpClient.GetAsync(
            $"https://api.github.com/repos/{Repository}/releases/latest",
            cancellationToken);
        if (!response.IsSuccessStatusCode)
        {
            return null;
        }

        await using var stream = await response.Content.ReadAsStreamAsync(cancellationToken);
        using var document = await JsonDocument.ParseAsync(stream, cancellationToken: cancellationToken);
        var root = document.RootElement;
        var tagName = root.GetProperty("tag_name").GetString() ?? string.Empty;
        if (!IsNewerVersion(tagName, currentVersion))
        {
            return null;
        }

        foreach (var asset in root.GetProperty("assets").EnumerateArray())
        {
            if (!asset.TryGetProperty("digest", out var digestElement))
            {
                continue;
            }
            if (asset.GetProperty("name").GetString() is "EvoHime-Setup.exe" &&
                Uri.TryCreate(asset.GetProperty("browser_download_url").GetString(), UriKind.Absolute, out var installerUri) &&
                digestElement.GetString() is string digest &&
                digest.StartsWith("sha256:", StringComparison.OrdinalIgnoreCase))
            {
                return new UpdateInfo(tagName.TrimStart('v'), installerUri, tagName, digest["sha256:".Length..]);
            }
        }

        return null;
    }

    public async Task<string> DownloadInstallerAsync(UpdateInfo update, CancellationToken cancellationToken)
    {
        var updateDirectory = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "EvoHime", "updates");
        Directory.CreateDirectory(updateDirectory);
        var finalPath = Path.Combine(updateDirectory, $"EvoHime-Setup-{update.Version}.exe");
        var temporaryPath = finalPath + ".download";
        try
        {
            await using (var source = await _httpClient.GetStreamAsync(update.InstallerUri, cancellationToken))
            await using (var destination = File.Create(temporaryPath))
            {
                await source.CopyToAsync(destination, cancellationToken);
            }

            await using var downloaded = File.OpenRead(temporaryPath);
            var actualSha256 = Convert.ToHexString(SHA256.HashData(downloaded));
            if (!actualSha256.Equals(update.Sha256, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidDataException("Хэш установщика обновления не совпадает.");
            }

            File.Move(temporaryPath, finalPath, overwrite: true);
            return finalPath;
        }
        finally
        {
            if (File.Exists(temporaryPath))
            {
                File.Delete(temporaryPath);
            }
        }
    }

    public static Process LaunchUpdater(string installerPath, string installDirectory)
    {
        var updaterPath = Path.Combine(AppContext.BaseDirectory, "evohime-transaction.exe");
        if (!File.Exists(updaterPath))
        {
            throw new FileNotFoundException("Компонент обновления не найден.", updaterPath);
        }

        var stateDirectory = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "EvoHime", "update-state");
        return Process.Start(new ProcessStartInfo
        {
            FileName = updaterPath,
            UseShellExecute = false,
            CreateNoWindow = true,
            ArgumentList =
            {
                "--installer", installerPath,
                "--install-dir", installDirectory,
                "--state-dir", stateDirectory,
            },
        }) ?? throw new InvalidOperationException("Не удалось запустить обновление Евы.");
    }

    private static bool TryParseVersion(string value, out Version version)
    {
        return Version.TryParse(value.Trim().TrimStart('v'), out version!);
    }

    public static bool IsNewerVersion(string latestVersion, string currentVersion) =>
        TryParseVersion(latestVersion, out var latest) &&
        TryParseVersion(currentVersion, out var current) &&
        latest > current;
}
