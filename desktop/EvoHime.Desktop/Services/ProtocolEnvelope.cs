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

    public static byte[] ResolveApproval(string approvalId, bool granted) => TaskCommand(14, output =>
    {
        output.WriteTag(1, WireFormat.WireType.LengthDelimited);
        output.WriteString(approvalId);
        output.WriteTag(2, WireFormat.WireType.Varint);
        output.WriteBool(granted);
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
