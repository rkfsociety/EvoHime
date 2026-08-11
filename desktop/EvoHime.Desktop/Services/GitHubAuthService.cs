using System.Diagnostics;
using System.ComponentModel;

namespace EvoHime.Desktop.Services;

public sealed record GitHubProfile(string DisplayName, string Login, bool IsAuthenticated, string Provider, string? Error = null)
{
    public static GitHubProfile SignedOut(string? error = null) =>
        new("GitHub", "Не авторизован", false, "", error);
}

public sealed class GitHubAuthService
{
    public async Task<GitHubProfile> GetProfileAsync(CancellationToken cancellationToken = default)
    {
        if (await IsCommandAvailableAsync("gh", cancellationToken))
        {
            var result = await RunAsync("gh", "api user --jq .login", cancellationToken);
            if (result.ExitCode == 0)
            {
                var login = result.Stdout.Trim();
                if (!string.IsNullOrWhiteSpace(login))
                {
                    return new GitHubProfile(login, $"@{login}", true, "gh");
                }
            }
        }

        if (await IsCommandAvailableAsync("git", cancellationToken))
        {
            var name = await RunAsync("git", "config --global user.name", cancellationToken);
            var email = await RunAsync("git", "config --global user.email", cancellationToken);
            if (name.ExitCode == 0 && !string.IsNullOrWhiteSpace(name.Stdout))
            {
                return new GitHubProfile(name.Stdout.Trim(), email.Stdout.Trim(), true, "git");
            }
        }

        return GitHubProfile.SignedOut("Войдите через gh или настройте Git CLI.");
    }

    public Task<CommandResult> StartGhLoginAsync(CancellationToken cancellationToken = default) =>
        RunAsync("gh", "auth login --hostname github.com --git-protocol https --web", cancellationToken, 120_000);

    public Task<CommandResult> StartGitLoginAsync(CancellationToken cancellationToken = default) =>
        RunAsync("git", "credential-manager github login", cancellationToken, 120_000);

    private static async Task<bool> IsCommandAvailableAsync(string fileName, CancellationToken cancellationToken)
    {
        var result = await RunAsync(fileName, "--version", cancellationToken, 5_000);
        return result.ExitCode == 0;
    }

    private static async Task<CommandResult> RunAsync(string fileName, string arguments, CancellationToken cancellationToken, int timeoutMs = 10_000)
    {
        using var process = new Process
        {
            StartInfo = new ProcessStartInfo
            {
                FileName = fileName,
                Arguments = arguments,
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                WorkingDirectory = Environment.CurrentDirectory,
            },
        };

        try
        {
            process.Start();
            var stdout = process.StandardOutput.ReadToEndAsync(cancellationToken);
            var stderr = process.StandardError.ReadToEndAsync(cancellationToken);
            using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            timeout.CancelAfter(timeoutMs);
            await process.WaitForExitAsync(timeout.Token);
            return new CommandResult(process.ExitCode, await stdout, await stderr);
        }
        catch (Exception error) when (error is Win32Exception or OperationCanceledException)
        {
            return new CommandResult(-1, string.Empty, error.Message);
        }
    }

    public sealed record CommandResult(int ExitCode, string Stdout, string Stderr);
}
