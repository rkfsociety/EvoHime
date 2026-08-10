using EvoHime.Desktop.Services;
using Google.Protobuf;
using Microsoft.VisualStudio.TestTools.UnitTesting;
using System.IO;
using System.Net;
using System.Net.Http;
using System;
using System.Collections.Generic;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

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
    public void StartTaskEnvelopeCarriesWorkspacePath()
    {
        var payload = ProtocolEnvelope.StartTask("task", "prompt", "C:\\Projects\\demo");
        using var input = new CodedInputStream(payload);
        var nested = new List<byte>();
        while (!input.IsAtEnd)
        {
            var tag = input.ReadTag();
            if ((tag >> 3) == 12)
            {
                nested.AddRange(input.ReadBytes().ToByteArray());
                break;
            }
            input.SkipLastField();
        }

        using var command = new CodedInputStream(nested.ToArray());
        var workspace = string.Empty;
        while (!command.IsAtEnd)
        {
            var tag = command.ReadTag();
            if ((tag >> 3) == 3)
            {
                workspace = command.ReadString();
                break;
            }
            command.SkipLastField();
        }

        Assert.AreEqual("C:\\Projects\\demo", workspace);
    }

    [TestMethod]
    public void ResolveApprovalEnvelopeCarriesDecision()
    {
        var payload = ProtocolEnvelope.ResolveApproval("approval-1", true);
        using var input = new CodedInputStream(payload);
        var nested = Array.Empty<byte>();
        while (!input.IsAtEnd)
        {
            var tag = input.ReadTag();
            if ((tag >> 3) == 14)
            {
                nested = input.ReadBytes().ToByteArray();
                break;
            }
            input.SkipLastField();
        }

        using var command = new CodedInputStream(nested);
        var approvalId = string.Empty;
        var granted = false;
        while (!command.IsAtEnd)
        {
            var tag = command.ReadTag();
            switch (tag >> 3)
            {
                case 1: approvalId = command.ReadString(); break;
                case 2: granted = command.ReadBool(); break;
                default: command.SkipLastField(); break;
            }
        }

        Assert.AreEqual("approval-1", approvalId);
        Assert.IsTrue(granted);
    }

    [TestMethod]
    public void PermissionModeEnvelopeCarriesMode()
    {
        var payload = ProtocolEnvelope.PermissionMode("read_only");
        using var input = new CodedInputStream(payload);
        var nested = Array.Empty<byte>();
        while (!input.IsAtEnd)
        {
            var tag = input.ReadTag();
            if ((tag >> 3) == 17)
            {
                nested = input.ReadBytes().ToByteArray();
                break;
            }
            input.SkipLastField();
        }

        using var command = new CodedInputStream(nested);
        var mode = string.Empty;
        while (!command.IsAtEnd)
        {
            var tag = command.ReadTag();
            if ((tag >> 3) == 1)
            {
                mode = command.ReadString();
                break;
            }
            command.SkipLastField();
        }

        Assert.AreEqual("read_only", mode);
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

    [TestMethod]
    public void ShellStateKeepsWorkspaceAndIgnoresDuplicateEvents()
    {
        var state = new NativeShellState();
        state.SelectWorkspace(".");

        Assert.AreEqual(Path.GetFullPath("."), state.WorkspacePath);
        Assert.IsTrue(state.ApplyEvent(new CoreEventEnvelope(4, "task", "task.started", [])));
        Assert.IsFalse(state.ApplyEvent(new CoreEventEnvelope(4, "task", "task.started", [])));
        Assert.AreEqual((ulong)4, state.LastSequence);
    }

    [TestMethod]
    public async Task WorkspaceSettingsRoundTripAndRecoverFromCorruptJson()
    {
        var root = Path.Combine(Path.GetTempPath(), "evohime-settings-" + Guid.NewGuid().ToString("N"));
        var settings = new WorkspaceSettings(Path.Combine(root, "settings.json"));
        try
        {
            await settings.SaveWorkspaceAsync("C:\\Projects\\demo");
            Assert.AreEqual("C:\\Projects\\demo", await settings.LoadWorkspaceAsync());

            await File.WriteAllTextAsync(settings.FilePath, "not-json");
            Assert.IsNull(await settings.LoadWorkspaceAsync());
        }
        finally
        {
            if (Directory.Exists(root))
            {
                Directory.Delete(root, recursive: true);
            }
        }
    }

    [TestMethod]
    public void ProjectCatalogKeepsChatsUnderTheirProject()
    {
        var root = Path.Combine(Path.GetTempPath(), "evohime-projects-" + Guid.NewGuid().ToString("N"));
        var catalogPath = Path.Combine(root, "projects.json");
        try
        {
            var service = new ProjectCatalogService(catalogPath);
            var catalog = service.Load();
            var project = service.EnsureProject(catalog, Path.Combine(root, "Demo"));
            var chat = service.AddChat(project, "Проверить проект");
            service.Save(catalog);

            var loaded = service.Load();
            Assert.AreEqual(1, loaded.Projects.Count);
            Assert.AreEqual(project.Path, loaded.Projects[0].Path);
            Assert.AreEqual(chat.Title, loaded.Projects[0].Chats[0].Title);
            Assert.IsTrue(service.RemoveProject(loaded, loaded.Projects[0]));
            service.Save(loaded);
            Assert.AreEqual(0, service.Load().Projects.Count);
        }
        finally
        {
            if (Directory.Exists(root))
            {
                Directory.Delete(root, recursive: true);
            }
        }
    }

    [TestMethod]
    public void TrayCommandsHaveStableIds()
    {
        Assert.AreEqual(1u, (uint)TrayMenuCommand.Show);
        Assert.AreEqual(2u, (uint)TrayMenuCommand.Exit);
    }

    [TestMethod]
    public void TrayNotificationTextIsSingleLineAndBounded()
    {
        var text = TrayNotificationText.Normalize("первая\nвторая\r\nтретья", 12);

        Assert.AreEqual("первая втор…", text);
        Assert.IsTrue(text.Length <= 12);
    }

    [TestMethod]
    public void TimelineFormatterIncludesUsefulEventDetails()
    {
        var envelope = new CoreEventEnvelope(
            8,
            "task",
            "tool.output",
            System.Text.Encoding.UTF8.GetBytes("{\"tool_name\":\"filesystem.search\",\"output\":\"found\"}"));

        Assert.AreEqual("[8] tool.output · filesystem.search: found", NativeEventFormatter.Format(envelope));
    }

    [TestMethod]
    public void UpdateServiceComparesClientVersions()
    {
        Assert.AreEqual("0.0.000032", UpdateService.CurrentVersion);
        Assert.IsTrue(UpdateService.IsNewerVersion("v0.0.000033", "0.0.000032"));
        Assert.IsFalse(UpdateService.IsNewerVersion("v0.0.000032", "0.0.000032"));
        Assert.IsFalse(UpdateService.IsNewerVersion("not-a-version", "0.0.000032"));
    }

    [TestMethod]
    public async Task UpdateServiceReadsReleaseDigestBeforeOfferingInstaller()
    {
        const string digest = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        using var http = new HttpClient(new StubHandler(_ => new HttpResponseMessage(HttpStatusCode.OK)
        {
            Content = new StringContent(
                $$"""{"tag_name":"v0.0.0002","assets":[{"name":"EvoHime-Setup.exe","browser_download_url":"https://example.invalid/EvoHime-Setup.exe","digest":"{{digest}}"}]}""",
                Encoding.UTF8,
                "application/json"),
        }));
        var service = new UpdateService(http);

        var update = await service.CheckLatestAsync("0.0.0001", CancellationToken.None);

        Assert.IsNotNull(update);
        Assert.AreEqual("0.0.0002", update.Version);
        Assert.AreEqual(digest["sha256:".Length..], update.Sha256);
    }

    private sealed class StubHandler(Func<HttpRequestMessage, HttpResponseMessage> responseFactory) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken) =>
            Task.FromResult(responseFactory(request));
    }
}
