using EvoHime.Desktop.Services;
using Google.Protobuf;
using Microsoft.VisualStudio.TestTools.UnitTesting;
using System.IO;
using System;
using System.Collections.Generic;

namespace EvoHime.Tests;

[TestClass]
public sealed class IpcCompatibilityTests
{
    [TestMethod]
    public void AdditiveMinorVersionIsCompatible()
    {
        Assert.IsTrue(ProtocolVersion.IsCompatible(1, 0, 1, 1));
        Assert.IsTrue(ProtocolVersion.IsCompatible(1, 1, 1, 0));
    }

    [TestMethod]
    public void MajorVersionMismatchIsRejected()
    {
        Assert.IsFalse(ProtocolVersion.IsCompatible(1, 0, 2, 0));
    }

    [TestMethod]
    public void FrameCodecRoundTripsPayload()
    {
        var frame = FrameCodec.Encode("hello"u8);
        CollectionAssert.AreEqual("hello"u8.ToArray(), FrameCodec.Decode(frame));
    }

    [TestMethod]
    public void FrameCodecRejectsOversizedPayload()
    {
        Assert.ThrowsException<ArgumentOutOfRangeException>(
            () => FrameCodec.Encode(new byte[FrameCodec.MaxFrameBytes + 1]));
    }

    [TestMethod]
    public void ReplayEnvelopeUsesTheSharedProtocolFields()
    {
        var payload = ProtocolEnvelope.Replay(42);
        var input = new CodedInputStream(payload);
        var fields = new HashSet<int>();
        while (!input.IsAtEnd)
        {
            var tag = input.ReadTag();
            fields.Add((int)(tag >> 3));
            input.SkipLastField();
        }

        Assert.IsTrue(fields.Contains(1));
        Assert.IsTrue(fields.Contains(2));
        Assert.IsTrue(fields.Contains(11));
    }

    [TestMethod]
    public void EventEnvelopeRoundTripsSequenceAndPayload()
    {
        using var buffer = new MemoryStream();
        using (var output = new CodedOutputStream(buffer, leaveOpen: true))
        {
            output.WriteTag(2, WireFormat.WireType.Varint);
            output.WriteUInt64(9);
            output.WriteTag(3, WireFormat.WireType.LengthDelimited);
            output.WriteString("task-9");
            output.WriteTag(4, WireFormat.WireType.LengthDelimited);
            output.WriteString("task.completed");
            output.WriteTag(5, WireFormat.WireType.LengthDelimited);
            output.WriteBytes(ByteString.CopyFromUtf8("done"));
            output.Flush();
        }

        var envelope = ProtocolEnvelope.ReadEvent(buffer.ToArray());
        Assert.AreEqual((ulong)9, envelope.SequenceId);
        Assert.AreEqual("task-9", envelope.TaskId);
        Assert.AreEqual("task.completed", envelope.EventType);
        CollectionAssert.AreEqual("done"u8.ToArray(), envelope.Payload);
    }
}
