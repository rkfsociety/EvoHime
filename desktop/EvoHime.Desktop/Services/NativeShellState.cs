namespace EvoHime.Desktop.Services;

public sealed class NativeShellState
{
    public string? WorkspacePath { get; private set; }
    public ulong LastSequence { get; private set; }

    public void SelectWorkspace(string path)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        WorkspacePath = Path.GetFullPath(path);
    }

    public bool ApplyEvent(CoreEventEnvelope envelope)
    {
        if (envelope.SequenceId <= LastSequence)
        {
            return false;
        }

        LastSequence = envelope.SequenceId;
        return true;
    }
}
