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
            new[] { "Новый чат", "Пульс", "Запланировано", "Плагины", "Проекты", "Настройки" },
            ShellNavigationCatalog.Items.Select(item => item.Title).ToArray());
    }
}
