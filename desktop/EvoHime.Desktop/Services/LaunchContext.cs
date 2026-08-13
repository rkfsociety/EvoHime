using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace EvoHime.Desktop.Services;

/// <summary>
/// Protected launch context written by the supervisor for one session
/// (see <c>evohime_desktop_ipc::session</c>). It carries the unpredictable pipe
/// name and the session secret this shell proves knowledge of when Core issues
/// its single-use nonce.
///
/// The WinUI shell is the compatibility client during the Electron migration,
/// so it authenticates with the <c>compatibility-shell</c> role.
/// </summary>
public sealed record LaunchContext(string PipeName, string Secret)
{
    public const string LegacyPipeName = "evohime-core-v1";
    public const string ClientRole = "compatibility-shell";
    private const int SecretHexLength = 64;

    public static LaunchContext Legacy { get; } = new(LegacyPipeName, string.Empty);

    public bool IsAuthenticated => Secret.Length == SecretHexLength;

    /// <summary>
    /// Reads the current context, falling back to the legacy pipe without a
    /// secret when no supervisor session is present (developer launch).
    /// </summary>
    public static LaunchContext Load(string? path = null)
    {
        try
        {
            var contextPath = path ?? DefaultPath();
            if (!File.Exists(contextPath))
            {
                return Legacy;
            }
            return Parse(File.ReadAllText(contextPath));
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            return Legacy;
        }
    }

    public static LaunchContext Parse(string json)
    {
        try
        {
            using var document = JsonDocument.Parse(json);
            var root = document.RootElement;
            if (root.ValueKind != JsonValueKind.Object)
            {
                return Legacy;
            }

            var pipeName = NormalizePipeName(
                root.TryGetProperty("pipe_name", out var pipe) ? pipe.GetString() : null);
            var secret = root.TryGetProperty("secret", out var value) ? value.GetString() : null;
            if (pipeName is null || !IsHexSecret(secret))
            {
                return Legacy;
            }
            return new LaunchContext(pipeName, secret!);
        }
        catch (JsonException)
        {
            return Legacy;
        }
    }

    /// <summary>Answers Core's nonce: HMAC-SHA256(secret, role | clientId | nonce).</summary>
    public string Proof(string clientId, string nonce)
    {
        if (!IsAuthenticated || string.IsNullOrEmpty(nonce))
        {
            return string.Empty;
        }
        var message = $"{ClientRole}\n{clientId}\n{nonce}";
        var digest = HMACSHA256.HashData(
            Encoding.UTF8.GetBytes(Secret),
            Encoding.UTF8.GetBytes(message));
        return Convert.ToHexStringLower(digest);
    }

    /// <summary>
    /// Returns the pipe name without the local prefix, which is the form
    /// <see cref="System.IO.Pipes.NamedPipeClientStream"/> expects. A remote
    /// (<c>\\host\pipe\…</c>) or malformed name is refused.
    /// </summary>
    private static string? NormalizePipeName(string? value)
    {
        var candidate = value?.Trim();
        if (string.IsNullOrEmpty(candidate) || candidate.Length > 256)
        {
            return null;
        }
        const string prefix = @"\\.\pipe\";
        var name = candidate.StartsWith(prefix, StringComparison.Ordinal)
            ? candidate[prefix.Length..]
            : candidate;
        if (name.Length == 0 ||
            !name.All(character => char.IsAsciiLetterOrDigit(character) || character is '-' or '_' or '.'))
        {
            return null;
        }
        return name;
    }

    private static bool IsHexSecret(string? value) =>
        value is { Length: SecretHexLength } && value.All(Uri.IsHexDigit);

    private static string DefaultPath()
    {
        var dataDirectory = Environment.GetEnvironmentVariable("EVOHIME_DATA_DIR");
        if (string.IsNullOrWhiteSpace(dataDirectory))
        {
            dataDirectory = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "EvoHime");
        }
        return Path.Combine(dataDirectory.Trim(), "runtime", "session.json");
    }
}
