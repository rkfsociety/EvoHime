using Google.Protobuf;
using System.IO.Pipes;

namespace EvoHime.Desktop.Services;

public sealed class CoreIpcClient
{
    public const uint CurrentProtocolMajor = 1;
    public const uint CurrentProtocolMinor = 0;

    private readonly string _pipeName;
    private NamedPipeClientStream? _pipe;

    public CoreIpcClient(string pipeName)
    {
        _pipeName = pipeName;
    }

    public async Task ConnectAsync(CancellationToken cancellationToken)
    {
        var pipe = new NamedPipeClientStream(
            ".",
            _pipeName,
            PipeDirection.InOut,
            PipeOptions.Asynchronous | PipeOptions.WriteThrough);
        await pipe.ConnectAsync(cancellationToken);
        _pipe = pipe;
    }

    public bool IsConnected => _pipe?.IsConnected == true;

    public async Task ConnectAndHandshakeAsync(CancellationToken cancellationToken)
    {
        await ConnectAsync(cancellationToken);
        await SendPayloadAsync(ProtocolEnvelope.Handshake(), cancellationToken);
    }

    public Task RequestReplayAsync(ulong afterSequence, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.Replay(afterSequence), cancellationToken);

    public Task StartTaskAsync(string taskId, string prompt, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.StartTask(taskId, prompt), cancellationToken);

    public Task StartTaskAsync(
        string taskId,
        string prompt,
        string workspacePath,
        CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.StartTask(taskId, prompt, workspacePath), cancellationToken);

    public Task StopTaskAsync(string taskId, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.StopTask(taskId), cancellationToken);

    public Task RequestModelConfigAsync(CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.ModelConfig(), cancellationToken);

    public Task RequestModelCatalogAsync(string mode, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.ModelCatalog(mode), cancellationToken);

    public Task SetPermissionModeAsync(string mode, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.PermissionMode(mode), cancellationToken);

    public Task ResolveApprovalAsync(string approvalId, bool granted, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.ResolveApproval(approvalId, granted), cancellationToken);

    public Task RequestTaskGraphAsync(string projectId, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.GetTaskGraph(projectId), cancellationToken);

    public Task RequestNextReadyTaskAsync(string projectId, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.NextReadyTask(projectId), cancellationToken);

    public Task ImportPrdAsync(
        string importId,
        string projectId,
        string origin,
        string version,
        string sourceText,
        CancellationToken cancellationToken) =>
        SendPayloadAsync(
            ProtocolEnvelope.ImportPrd(importId, projectId, origin, version, sourceText),
            cancellationToken);

    public Task CreateProjectAsync(
        string projectId,
        string title,
        string workspacePath,
        CancellationToken cancellationToken) =>
        SendPayloadAsync(
            ProtocolEnvelope.CreateProject(projectId, title, workspacePath),
            cancellationToken);

    public Task UpdateTaskStatusAsync(
        string taskId,
        long expectedVersion,
        string status,
        CancellationToken cancellationToken) =>
        SendPayloadAsync(
            ProtocolEnvelope.UpdateTaskStatus(taskId, expectedVersion, status),
            cancellationToken);

    public Task RequestTaskHistoryAsync(string taskId, uint limit, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.GetTaskHistory(taskId, limit), cancellationToken);

    public async Task<ulong> ReadReplayAsync(
        ulong afterSequence,
        Func<CoreEventEnvelope, Task> onEvent,
        CancellationToken cancellationToken)
    {
        await RequestReplayAsync(afterSequence, cancellationToken);
        var lastSequence = afterSequence;
        while (true)
        {
            var envelope = await ReadEventAsync(cancellationToken);
            if (envelope.EventType == "replay.end")
            {
                return Math.Max(lastSequence, envelope.SequenceId);
            }
            if (envelope.EventType == "core.ready")
            {
                continue;
            }
            lastSequence = Math.Max(lastSequence, envelope.SequenceId);
            await onEvent(envelope);
        }
    }

    public async Task<CoreEventEnvelope> ReadEventAsync(CancellationToken cancellationToken)
    {
        var pipe = GetConnectedPipe();
        var prefix = new byte[sizeof(uint)];
        await ReadExactlyAsync(pipe, prefix, cancellationToken);
        var length = BitConverter.ToUInt32(prefix, 0);
        var frame = new byte[sizeof(uint) + checked((int)length)];
        prefix.CopyTo(frame, 0);
        await ReadExactlyAsync(pipe, frame.AsMemory(sizeof(uint)), cancellationToken);
        return ProtocolEnvelope.ReadEvent(FrameCodec.Decode(frame));
    }

    public async Task SendAsync(IMessage message, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(message);
        await SendPayloadAsync(message.ToByteArray(), cancellationToken);
    }

    private async Task SendPayloadAsync(byte[] payload, CancellationToken cancellationToken)
    {
        var pipe = GetConnectedPipe();
        var frame = FrameCodec.Encode(payload);
        await pipe.WriteAsync(frame, cancellationToken);
        await pipe.FlushAsync(cancellationToken);
    }

    private static async Task ReadExactlyAsync(Stream stream, Memory<byte> buffer, CancellationToken cancellationToken)
    {
        while (!buffer.IsEmpty)
        {
            var read = await stream.ReadAsync(buffer, cancellationToken);
            if (read == 0)
            {
                throw new EndOfStreamException("Core IPC pipe closed before the frame completed.");
            }
            buffer = buffer[read..];
        }
    }

    private NamedPipeClientStream GetConnectedPipe() =>
        IsConnected ? _pipe! : throw new InvalidOperationException("Core IPC pipe is not connected.");

    public async ValueTask DisposeAsync()
    {
        if (_pipe is not null)
        {
            await _pipe.DisposeAsync();
            _pipe = null;
        }
    }
}
