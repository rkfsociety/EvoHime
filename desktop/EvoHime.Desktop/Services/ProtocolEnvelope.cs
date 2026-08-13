using Google.Protobuf;

namespace EvoHime.Desktop.Services;

public sealed record CoreEventEnvelope(
    ulong SequenceId,
    string TaskId,
    string EventType,
    byte[] Payload);

public static class ProtocolEnvelope
{
    public const uint ProtocolMajor = 1;
    public const uint ProtocolMinor = 0;

    public static byte[] Handshake()
    {
        using var buffer = new MemoryStream();
        using var output = new CodedOutputStream(buffer, leaveOpen: true);
        WriteProtocol(output);
        output.WriteTag(2, WireFormat.WireType.LengthDelimited);
        output.WriteString(Guid.NewGuid().ToString("N"));
        output.WriteTag(10, WireFormat.WireType.LengthDelimited);
        using var handshake = new MemoryStream();
        using (var nested = new CodedOutputStream(handshake, leaveOpen: true))
        {
            WriteProtocol(nested);
            nested.WriteTag(2, WireFormat.WireType.LengthDelimited);
            nested.WriteString("EvoHime.Desktop");
            nested.Flush();
        }
        output.WriteBytes(ByteString.CopyFrom(handshake.ToArray()));
        output.Flush();
        return buffer.ToArray();
    }

    public static byte[] Replay(ulong afterSequence)
    {
        using var buffer = new MemoryStream();
        using var output = new CodedOutputStream(buffer, leaveOpen: true);
        WriteProtocol(output);
        output.WriteTag(2, WireFormat.WireType.LengthDelimited);
        output.WriteString(Guid.NewGuid().ToString("N"));
        output.WriteTag(11, WireFormat.WireType.LengthDelimited);
        using var nested = new MemoryStream();
        using (var replay = new CodedOutputStream(nested, leaveOpen: true))
        {
            replay.WriteTag(1, WireFormat.WireType.Varint);
            replay.WriteUInt64(afterSequence);
            replay.Flush();
        }
        output.WriteBytes(ByteString.CopyFrom(nested.ToArray()));
        output.Flush();
        return buffer.ToArray();
    }

    public static byte[] StartTask(string taskId, string prompt) => StartTask(taskId, prompt, string.Empty);

    public static byte[] StartTask(string taskId, string prompt, string workspacePath) => TaskCommand(12, output =>
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        output.WriteString(taskId);
        output.WriteTag(2, WireFormat.WireType.LengthDelimited);
        output.WriteString(prompt);
        if (!string.IsNullOrWhiteSpace(workspacePath))
        {
            output.WriteTag(3, WireFormat.WireType.LengthDelimited);
            output.WriteString(workspacePath);
        }
    });

    public static byte[] StopTask(string taskId) => TaskCommand(13, output =>
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        output.WriteString(taskId);
    });

    public static byte[] ModelConfig() => TaskCommand(15, _ => { });

    public static byte[] ModelCatalog(string mode) => TaskCommand(16, output =>
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        output.WriteString(mode);
    });

    public static byte[] PermissionMode(string mode) => TaskCommand(17, output =>
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        output.WriteString(mode);
    });

    public static byte[] ResolveApproval(string approvalId, bool granted) => TaskCommand(14, output =>
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        output.WriteString(approvalId);
        output.WriteTag(2, WireFormat.WireType.Varint);
        output.WriteBool(granted);
    });

    public static byte[] GetTaskGraph(string projectId) => TaskCommand(22, output =>
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        output.WriteString(projectId);
    });

    public static byte[] NextReadyTask(string projectId) => TaskCommand(23, output =>
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        output.WriteString(projectId);
    });

    public static byte[] ImportPrd(
        string importId,
        string projectId,
        string origin,
        string version,
        string sourceText) => TaskCommand(24, output =>
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        output.WriteString(importId);
        output.WriteTag(2, WireFormat.WireType.LengthDelimited);
        output.WriteString(projectId);
        output.WriteTag(3, WireFormat.WireType.LengthDelimited);
        output.WriteString(origin);
        output.WriteTag(4, WireFormat.WireType.LengthDelimited);
        output.WriteString(version);
        output.WriteTag(5, WireFormat.WireType.LengthDelimited);
        output.WriteString(sourceText);
    });

    public static byte[] CreateProject(
        string projectId,
        string title,
        string workspacePath,
        string sourceRef = "") => TaskCommand(18, output =>
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        output.WriteString(projectId);
        output.WriteTag(2, WireFormat.WireType.LengthDelimited);
        output.WriteString(title);
        output.WriteTag(3, WireFormat.WireType.LengthDelimited);
        output.WriteString(workspacePath);
        if (!string.IsNullOrWhiteSpace(sourceRef))
        {
            output.WriteTag(4, WireFormat.WireType.LengthDelimited);
            output.WriteString(sourceRef);
        }
    });

    public static byte[] ListWorkspace(string workspacePath, string relativePath = ".", uint maxEntries = 200) => TaskCommand(52, output =>
    {
        WriteString(output, 1, workspacePath);
        WriteString(output, 2, relativePath);
        output.WriteTag(3, WireFormat.WireType.Varint);
        output.WriteUInt32(maxEntries);
    });

    public static byte[] ReadWorkspaceFile(string workspacePath, string relativePath, uint maxBytes = 512 * 1024) => TaskCommand(53, output =>
    {
        WriteString(output, 1, workspacePath);
        WriteString(output, 2, relativePath);
        output.WriteTag(3, WireFormat.WireType.Varint);
        output.WriteUInt32(maxBytes);
    });

    public static byte[] GitStatus(string workspacePath, uint maxBytes = 512 * 1024) => TaskCommand(54, output =>
    {
        WriteString(output, 1, workspacePath);
        output.WriteTag(2, WireFormat.WireType.Varint);
        output.WriteUInt32(maxBytes);
    });

    public static byte[] GitDiff(string workspacePath, string relativePath = "", uint maxBytes = 512 * 1024) => TaskCommand(55, output =>
    {
        WriteString(output, 1, workspacePath);
        WriteString(output, 2, relativePath);
        output.WriteTag(3, WireFormat.WireType.Varint);
        output.WriteUInt32(maxBytes);
    });

    public static byte[] TerminalExecute(
        string taskId,
        string workspacePath,
        string program,
        IReadOnlyList<string> args,
        string cwd = "",
        uint timeoutMs = 30_000,
        string approvalId = "") => TaskCommand(56, output =>
    {
        WriteString(output, 1, taskId);
        WriteString(output, 2, workspacePath);
        WriteString(output, 3, program);
        foreach (var arg in args)
        {
            WriteString(output, 4, arg);
        }
        WriteString(output, 5, cwd);
        output.WriteTag(6, WireFormat.WireType.Varint);
        output.WriteUInt32(timeoutMs);
        WriteString(output, 7, approvalId);
    });

    public static byte[] UpdateTaskStatus(string taskId, long expectedVersion, string status) => TaskCommand(20, output =>
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        output.WriteString(taskId);
        output.WriteTag(2, WireFormat.WireType.Varint);
        output.WriteInt64(expectedVersion);
        output.WriteTag(3, WireFormat.WireType.LengthDelimited);
        output.WriteString(status);
    });

    public static byte[] GetTaskHistory(string taskId, uint limit = 20) => TaskCommand(25, output =>
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        output.WriteString(taskId);
        output.WriteTag(2, WireFormat.WireType.Varint);
        output.WriteUInt32(limit);
    });

    public static byte[] CreateTask(
        string taskId,
        string projectId,
        string parentId,
        string title,
        string description,
        string acceptanceCriteria,
        long priority,
        string complexity) => TaskCommand(19, output =>
    {
        WriteString(output, 1, taskId);
        WriteString(output, 2, projectId);
        WriteString(output, 3, parentId);
        WriteString(output, 4, title);
        WriteString(output, 5, description);
        WriteString(output, 7, acceptanceCriteria);
        WriteString(output, 9, "backlog");
        output.WriteTag(10, WireFormat.WireType.Varint);
        output.WriteInt64(priority);
        WriteString(output, 12, complexity);
    });

    public static byte[] AddTaskEdge(string fromTaskId, string toTaskId, string kind) => TaskCommand(21, output =>
    {
        WriteString(output, 1, fromTaskId);
        WriteString(output, 2, toTaskId);
        WriteString(output, 3, kind);
    });

    public static byte[] GetTaskContext(string projectId, string taskId, uint maxChars = 4096) => TaskCommand(26, output =>
    {
        WriteString(output, 1, projectId);
        WriteString(output, 2, taskId);
        output.WriteTag(3, WireFormat.WireType.Varint);
        output.WriteUInt32(maxChars);
    });

    public static byte[] GetTaskPlanSpec(string projectId, string taskId, uint maxChars = 4096) => TaskCommand(27, output =>
    {
        WriteString(output, 1, projectId);
        WriteString(output, 2, taskId);
        output.WriteTag(3, WireFormat.WireType.Varint);
        output.WriteUInt32(maxChars);
    });

    public static byte[] PrepareBuild(string projectId, byte[] proposalJson) => TaskCommand(29, output =>
    {
        WriteString(output, 1, projectId);
        output.WriteTag(2, WireFormat.WireType.LengthDelimited);
        output.WriteBytes(ByteString.CopyFrom(proposalJson));
    });

    public static byte[] ApplyApprovedBuild(string projectId, string runId, string taskId, byte[] approvedBuildJson) => TaskCommand(28, output =>
    {
        WriteString(output, 1, projectId);
        output.WriteTag(2, WireFormat.WireType.LengthDelimited);
        output.WriteBytes(ByteString.CopyFrom(approvedBuildJson));
        WriteString(output, 3, runId);
        WriteString(output, 4, taskId);
    });

    public static byte[] GetTaskSnapshot(string projectId, string taskId) => TaskCommand(30, output =>
    {
        WriteString(output, 1, projectId);
        WriteString(output, 2, taskId);
    });

    public static byte[] RestoreTaskSnapshot(string projectId, string taskId, string snapshotId) => TaskCommand(31, output =>
    {
        WriteString(output, 1, projectId);
        WriteString(output, 2, taskId);
        WriteString(output, 3, snapshotId);
    });

    public static byte[] GetBuildPolicy(string projectId) => TaskCommand(32, output => WriteString(output, 1, projectId));

    // Submits one bounded, local-only feedback record (useful/not-useful,
    // optional correction, optional rejection reason) about an existing
    // run's tool result or approval decision. signal is one of
    // "useful" | "not_useful" | "neutral". Mirrors the field numbers in
    // evohime.desktop.proto's SubmitFeedback message.
    public static byte[] SubmitFeedback(
        string runId,
        string taskId,
        string subjectRef,
        string signal,
        string correction = "",
        string rejectionReason = "",
        string outcome = "") => TaskCommand(57, output =>
    {
        WriteString(output, 1, runId);
        WriteString(output, 2, taskId);
        WriteString(output, 3, subjectRef);
        WriteString(output, 4, signal);
        WriteString(output, 5, correction);
        WriteString(output, 6, rejectionReason);
        WriteString(output, 7, outcome);
    });

    public static byte[] SaveBuildPolicy(string projectId, byte[] policyJson, long expectedVersion) => TaskCommand(33, output =>
    {
        WriteString(output, 1, projectId);
        output.WriteTag(2, WireFormat.WireType.LengthDelimited);
        output.WriteBytes(ByteString.CopyFrom(policyJson));
        output.WriteTag(3, WireFormat.WireType.Varint);
        output.WriteInt64(expectedVersion);
    });

    public static CoreEventEnvelope ReadEvent(ReadOnlySpan<byte> payload)
    {
        using var input = new CodedInputStream(payload.ToArray());
        ulong sequenceId = 0;
        string taskId = string.Empty;
        string eventType = string.Empty;
        byte[] eventPayload = [];
        while (!input.IsAtEnd)
        {
            var tag = input.ReadTag();
            switch (tag >> 3)
            {
                case 1:
                    input.ReadBytes();
                    break;
                case 2:
                    sequenceId = input.ReadUInt64();
                    break;
                case 3:
                    taskId = input.ReadString();
                    break;
                case 4:
                    eventType = input.ReadString();
                    break;
                case 5:
                    eventPayload = input.ReadBytes().ToByteArray();
                    break;
                default:
                    input.SkipLastField();
                    break;
            }
        }
        return new CoreEventEnvelope(sequenceId, taskId, eventType, eventPayload);
    }

    private static void WriteProtocol(CodedOutputStream output)
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        using var nested = new MemoryStream();
        using (var protocol = new CodedOutputStream(nested, leaveOpen: true))
        {
            protocol.WriteTag(1, WireFormat.WireType.Varint);
            protocol.WriteUInt32(ProtocolMajor);
            protocol.WriteTag(2, WireFormat.WireType.Varint);
            protocol.WriteUInt32(ProtocolMinor);
            protocol.Flush();
        }
        output.WriteBytes(ByteString.CopyFrom(nested.ToArray()));
    }

    private static void WriteString(CodedOutputStream output, int field, string value)
    {
        if (!string.IsNullOrEmpty(value))
        {
            output.WriteTag(field, WireFormat.WireType.LengthDelimited);
            output.WriteString(value);
        }
    }

    private static byte[] TaskCommand(uint field, Action<CodedOutputStream> writeNested)
    {
        using var buffer = new MemoryStream();
        using var output = new CodedOutputStream(buffer, leaveOpen: true);
        WriteProtocol(output);
        output.WriteTag(2, WireFormat.WireType.LengthDelimited);
        output.WriteString(Guid.NewGuid().ToString("N"));
        output.WriteTag((int)field, WireFormat.WireType.LengthDelimited);
        using var nested = new MemoryStream();
        using (var command = new CodedOutputStream(nested, leaveOpen: true))
        {
            writeNested(command);
            command.Flush();
        }
        output.WriteBytes(ByteString.CopyFrom(nested.ToArray()));
        output.Flush();
        return buffer.ToArray();
    }
}
