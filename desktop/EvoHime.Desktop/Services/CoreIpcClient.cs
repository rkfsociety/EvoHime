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

    public Task RequestWorkspaceListAsync(
        string workspacePath,
        string relativePath,
        CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.ListWorkspace(workspacePath, relativePath), cancellationToken);

    public Task RequestWorkspaceFileAsync(
        string workspacePath,
        string relativePath,
        CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.ReadWorkspaceFile(workspacePath, relativePath), cancellationToken);

    public Task RequestGitStatusAsync(string workspacePath, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.GitStatus(workspacePath), cancellationToken);

    public Task RequestGitDiffAsync(
        string workspacePath,
        string relativePath,
        CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.GitDiff(workspacePath, relativePath), cancellationToken);

    public Task ExecuteTerminalAsync(
        string taskId,
        string workspacePath,
        string program,
        IReadOnlyList<string> args,
        string cwd,
        uint timeoutMs,
        string approvalId,
        CancellationToken cancellationToken) =>
        SendPayloadAsync(
            ProtocolEnvelope.TerminalExecute(taskId, workspacePath, program, args, cwd, timeoutMs, approvalId),
            cancellationToken);

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

    public Task CreateTaskAsync(
        string taskId,
        string projectId,
        string parentId,
        string title,
        string description,
        string acceptanceCriteria,
        long priority,
        string complexity,
        CancellationToken cancellationToken) =>
        SendPayloadAsync(
            ProtocolEnvelope.CreateTask(taskId, projectId, parentId, title, description, acceptanceCriteria, priority, complexity),
            cancellationToken);

    public Task AddTaskEdgeAsync(string fromTaskId, string toTaskId, string kind, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.AddTaskEdge(fromTaskId, toTaskId, kind), cancellationToken);

    public Task RequestTaskContextAsync(string projectId, string taskId, uint maxChars, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.GetTaskContext(projectId, taskId, maxChars), cancellationToken);

    public Task RequestTaskPlanSpecAsync(string projectId, string taskId, uint maxChars, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.GetTaskPlanSpec(projectId, taskId, maxChars), cancellationToken);

    public Task PrepareBuildAsync(string projectId, byte[] proposalJson, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.PrepareBuild(projectId, proposalJson), cancellationToken);

    public Task ApplyApprovedBuildAsync(string projectId, string runId, string taskId, byte[] approvedBuildJson, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.ApplyApprovedBuild(projectId, runId, taskId, approvedBuildJson), cancellationToken);

    public Task RequestTaskSnapshotAsync(string projectId, string taskId, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.GetTaskSnapshot(projectId, taskId), cancellationToken);

    public Task RestoreTaskSnapshotAsync(string projectId, string taskId, string snapshotId, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.RestoreTaskSnapshot(projectId, taskId, snapshotId), cancellationToken);

    public Task RequestBuildPolicyAsync(string projectId, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.GetBuildPolicy(projectId), cancellationToken);

    public Task SaveBuildPolicyAsync(string projectId, byte[] policyJson, long expectedVersion, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.SaveBuildPolicy(projectId, policyJson, expectedVersion), cancellationToken);

    public Task RequestDoctorReportAsync(string projectId, int detailLevel, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.RunDoctor(projectId, detailLevel), cancellationToken);

    public Task RequestDoctorExportAsync(string destinationPath, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.ExportDoctorLogs(destinationPath), cancellationToken);

    public Task RequestCapabilityListAsync(uint limit, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.ListCapabilities(limit), cancellationToken);

    public Task RequestCapabilitySelectionAsync(
        string taskId,
        string intent,
        IReadOnlyList<string> requiredTools,
        IReadOnlyList<string> requiredDomains,
        string requestedRisk,
        CancellationToken cancellationToken) =>
        SendPayloadAsync(
            ProtocolEnvelope.GetCapabilitySelection(taskId, intent, requiredTools, requiredDomains, requestedRisk),
            cancellationToken);

    public Task PinCapabilitySelectionAsync(string taskId, CancellationToken cancellationToken) =>
        SendPayloadAsync(ProtocolEnvelope.PinCapabilitySelection(taskId), cancellationToken);

    public Task ReplaceCapabilitySelectionAsync(
        string taskId,
        string manifestName,
        string intent,
        IReadOnlyList<string> requiredTools,
        IReadOnlyList<string> requiredDomains,
        string requestedRisk,
        CancellationToken cancellationToken) =>
        SendPayloadAsync(
            ProtocolEnvelope.ReplaceCapabilitySelection(taskId, manifestName, intent, requiredTools, requiredDomains, requestedRisk),
            cancellationToken);

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
