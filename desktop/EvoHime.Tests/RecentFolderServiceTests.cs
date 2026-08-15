#nullable enable

using EvoHime.Desktop.Services;
using Microsoft.VisualStudio.TestTools.UnitTesting;
using System;
using System.IO;

namespace EvoHime.Tests;

[TestClass]
public sealed class RecentFolderServiceTests
{
    private string _root = string.Empty;

    [TestInitialize]
    public void Setup()
    {
        _root = Path.Combine(Path.GetTempPath(), "evohime-recent-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(_root);
    }

    [TestCleanup]
    public void Cleanup()
    {
        try
        {
            Directory.Delete(_root, recursive: true);
        }
        catch (IOException)
        {
        }
    }

    [TestMethod]
    public void RemembersFolderOfTheSelectedFileAcrossInstances()
    {
        var storePath = Path.Combine(_root, "store", "recent-folders.json");
        var attachment = Path.Combine(_root, "docs", "план.md");
        Directory.CreateDirectory(Path.GetDirectoryName(attachment)!);
        File.WriteAllText(attachment, "x");

        new RecentFolderService(storePath).RememberFile(RecentFolderService.AttachmentsKey, attachment);

        var reopened = new RecentFolderService(storePath);
        Assert.AreEqual(
            Path.GetDirectoryName(attachment),
            reopened.Get(RecentFolderService.AttachmentsKey));
    }

    [TestMethod]
    public void ForgetsFolderThatNoLongerExists()
    {
        var storePath = Path.Combine(_root, "recent-folders.json");
        var removed = Path.Combine(_root, "gone");
        Directory.CreateDirectory(removed);

        var service = new RecentFolderService(storePath);
        service.Remember(RecentFolderService.AttachmentsKey, removed);
        Directory.Delete(removed);

        Assert.IsNull(service.Get(RecentFolderService.AttachmentsKey));
    }

    [TestMethod]
    public void ReturnsNullForUnknownKey()
    {
        var service = new RecentFolderService(Path.Combine(_root, "recent-folders.json"));
        Assert.IsNull(service.Get(RecentFolderService.AttachmentsKey));
    }
}
