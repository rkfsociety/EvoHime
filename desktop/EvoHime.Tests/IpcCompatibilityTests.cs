using EvoHime.Desktop.Services;
using Microsoft.VisualStudio.TestTools.UnitTesting;
using System.IO;
using System;

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
}
