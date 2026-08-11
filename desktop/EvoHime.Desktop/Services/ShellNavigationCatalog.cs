namespace EvoHime.Desktop.Services;

public sealed record ShellNavigationItem(string Title, string Glyph, string Description);

public static class ShellNavigationCatalog
{
    public static IReadOnlyList<ShellNavigationItem> Items { get; } =
    [
        new("Новый чат", "＋", "Начать новую задачу"),
        new("Запланировано", "◷", "Будущие задачи"),
        new("Плагины", "◇", "Подключённые источники"),
        new("Проекты", "⌂", "Рабочие пространства"),
        new("Настройки", "⚙", "Тема, обновления и диагностика"),
    ];
}
