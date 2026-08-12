using Microsoft.VisualStudio.TestTools.UnitTesting;
using EvoHime.Desktop;
using EvoHime.Desktop.Services;
using System.Linq;

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
            new[] { "Новый чат", "Задачи", "Файлы", "Git", "Запланировано", "Плагины", "Проекты", "Настройки" },
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
}
