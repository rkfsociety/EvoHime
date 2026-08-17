namespace EvoHime.Desktop.Services;

public static class ModelCatalogFilter
{
    private const string HiddenModel = "mythomax-l2-13b:free";

    public static bool IsAllowed(string model, string mode) =>
        !string.Equals(model.Trim(), HiddenModel, StringComparison.OrdinalIgnoreCase) &&
        (string.Equals(mode, "paid", StringComparison.OrdinalIgnoreCase)
            ? !model.EndsWith(":free", StringComparison.OrdinalIgnoreCase)
            : model.EndsWith(":free", StringComparison.OrdinalIgnoreCase));

    public static IReadOnlyList<string> Filter(IEnumerable<string> models, string mode) =>
        models
            .Where(model => !string.IsNullOrWhiteSpace(model) && IsAllowed(model, mode))
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .OrderBy(model => model, StringComparer.OrdinalIgnoreCase)
            .ToList();
}
