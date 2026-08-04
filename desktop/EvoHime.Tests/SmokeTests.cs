using Microsoft.VisualStudio.TestTools.UnitTesting;
using EvoHime.Desktop;

namespace EvoHime.Tests;

[TestClass]
public sealed class SmokeTests
{
    [TestMethod]
    public void DesktopAssemblyExposesExpectedAppType()
    {
        Assert.IsNotNull(typeof(App));
    }
}
