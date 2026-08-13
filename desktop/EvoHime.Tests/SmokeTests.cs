#nullable enable

using Microsoft.VisualStudio.TestTools.UnitTesting;
using EvoHime.Desktop;
using EvoHime.Desktop.Services;
using System;
using System.Linq;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace EvoHime.Tests;

[TestClass]
public sealed class SmokeTests
{
    [TestMethod]
    public void DesktopAssemblyExposesExpectedAppType()
    {
        Assert.IsNotNull(typeof(App));
    }

    [TestMethod]
    public void ShellNavigationContainsTheFirstVisualWorkspaceSections()
    {
        CollectionAssert.AreEqual(
            new[] { "Новый чат", "Задачи", "Файлы", "Git", "Терминал", "Запланировано", "Плагины", "Проекты", "Настройки" },
            ShellNavigationCatalog.Items.Select(item => item.Title).ToArray());
    }

    [TestMethod]
    public void FreeCatalogContainsOnlyFreeModels()
    {
        var models = ModelCatalogFilter.Filter(
            new[] { "claude-haiku:free", "gpt-5", "deepseek:free", "gpt-5:preview" },
            "free");

        CollectionAssert.AreEqual(
            new[] { "claude-haiku:free", "deepseek:free" },
            models.ToArray());
    }

    [TestMethod]
    public void PaidCatalogExcludesFreeModels()
    {
        var models = ModelCatalogFilter.Filter(
            new[] { "claude-haiku:free", "gpt-5", "deepseek:free", "gpt-5:preview" },
            "paid");

        CollectionAssert.AreEqual(
            new[] { "gpt-5", "gpt-5:preview" },
            models.ToArray());
    }

    [TestMethod]
    public void ProviderSettingsKeepSecretInCredentialStoreAndRotateReferences()
    {
        var path = Path.Combine(Path.GetTempPath(), $"evohime-provider-settings-{Guid.NewGuid():N}.bin");
        var store = new MemoryProviderSecretStore();
        try
        {
            var service = new ProviderSettingsService(path, store);
            service.Save(new ProviderSettings("literouter", "https://example.test/v1", "model-a", "first-secret"));
            var first = service.Load();

            Assert.AreEqual("first-secret", first.ApiKey);
            Assert.IsFalse(string.IsNullOrWhiteSpace(first.CredentialId));
            Assert.IsFalse(Encoding.UTF8.GetString(File.ReadAllBytes(path)).Contains("first-secret", StringComparison.Ordinal));

            service.Save(new ProviderSettings("literouter", "https://example.test/v1", "model-b", "second-secret"));
            var second = service.Load();

            Assert.AreEqual("second-secret", second.ApiKey);
            Assert.AreNotEqual(first.CredentialId, second.CredentialId);
            Assert.IsTrue(store.Deleted.Contains(first.CredentialId!));
            Assert.IsFalse(store.Values.Values.Contains("first-secret"));
        }
        finally
        {
            if (File.Exists(path)) File.Delete(path);
            if (File.Exists(path + ".tmp")) File.Delete(path + ".tmp");
        }
    }

    [TestMethod]
    public void ProviderSettingsFailClosedWhenCredentialIsUnavailable()
    {
        var path = Path.Combine(Path.GetTempPath(), $"evohime-provider-settings-{Guid.NewGuid():N}.bin");
        var store = new MemoryProviderSecretStore { ThrowOnRead = true };
        try
        {
            var writer = new ProviderSettingsService(path, new MemoryProviderSecretStore());
            writer.Save(new ProviderSettings("literouter", "https://example.test/v1", "model-a", "secret"));

            var loaded = new ProviderSettingsService(path, store).Load();
            Assert.AreEqual(string.Empty, loaded.ApiKey);
            Assert.AreEqual("model-a", loaded.Model);
        }
        finally
        {
            if (File.Exists(path)) File.Delete(path);
            if (File.Exists(path + ".tmp")) File.Delete(path + ".tmp");
        }
    }

    private sealed class MemoryProviderSecretStore : IProviderSecretStore
    {
        public Dictionary<string, string> Values { get; } = new(StringComparer.Ordinal);
        public HashSet<string> Deleted { get; } = new(StringComparer.Ordinal);
        public bool ThrowOnRead { get; init; }

        public string? Read(string credentialId)
        {
            if (ThrowOnRead) throw new System.Security.Cryptography.CryptographicException("credential unavailable");
            return Values.TryGetValue(credentialId, out var value) ? value : null;
        }

        public void Write(string credentialId, string secret) => Values[credentialId] = secret;

        public void Delete(string credentialId)
        {
            Deleted.Add(credentialId);
            Values.Remove(credentialId);
        }
    }
}
