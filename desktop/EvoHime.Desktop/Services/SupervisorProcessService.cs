using System.Diagnostics;

namespace EvoHime.Desktop.Services;

public sealed class SupervisorProcessService : IDisposable
{
    private Process? _process;

    public bool Start()
    {
        if (_process is { HasExited: false })
        {
            return true;
        }

        var baseDirectory = AppContext.BaseDirectory;
        var supervisorPath = Path.Combine(baseDirectory, "evohime-supervisor.exe");
        var corePath = Environment.GetEnvironmentVariable("EVOHIME_CORE_EXE")
            ?? Path.Combine(baseDirectory, "evohime-core.exe");
        if (!File.Exists(supervisorPath) || !File.Exists(corePath))
        {
            return false;
        }

        var dataDirectory = Environment.GetEnvironmentVariable("EVOHIME_DATA_DIR")
            ?? Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "EvoHime");
        Directory.CreateDirectory(dataDirectory);
        _process = Process.Start(new ProcessStartInfo
        {
            FileName = supervisorPath,
            WorkingDirectory = baseDirectory,
            UseShellExecute = false,
            CreateNoWindow = true,
            Environment =
            {
                ["EVOHIME_CORE_EXE"] = corePath,
                ["EVOHIME_DATA_DIR"] = dataDirectory,
            },
        });
        return _process is not null;
    }

    public void Dispose()
    {
        if (_process is null)
        {
            return;
        }

        try
        {
            if (!_process.HasExited)
            {
                _process.Kill(entireProcessTree: true);
                _process.WaitForExit(2_000);
            }
        }
        catch (InvalidOperationException)
        {
        }
        catch (System.ComponentModel.Win32Exception)
        {
        }
        finally
        {
            _process.Dispose();
            _process = null;
        }
    }
}
