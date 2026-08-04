using System.Text;
using System.Text.Json;

namespace EvoHime.Desktop.Services;

public static class NativeEventFormatter
{
    public static string Format(CoreEventEnvelope envelope)
    {
        var detail = envelope.EventType switch
        {
            "agent.message.delta" => GetString(envelope.Payload, "content"),
            "tool.started" => GetString(envelope.Payload, "tool_name"),
            "tool.output" => FormatToolOutput(envelope.Payload),
            "task.started" => GetString(envelope.Payload, "prompt"),
            "task.failed" => GetString(envelope.Payload, "error"),
            "task.completed" => GetString(envelope.Payload, "final_message"),
            _ => string.Empty,
        };

        return string.IsNullOrWhiteSpace(detail)
            ? $"[{envelope.SequenceId}] {envelope.EventType}"
            : $"[{envelope.SequenceId}] {envelope.EventType} · {Limit(detail, 240)}";
    }

    private static string FormatToolOutput(byte[] payload)
    {
        var tool = GetString(payload, "tool_name");
        var output = GetString(payload, "output");
        return string.IsNullOrWhiteSpace(tool) ? output : $"{tool}: {output}";
    }

    private static string GetString(byte[] payload, string property)
    {
        try
        {
            using var document = JsonDocument.Parse(payload);
            return document.RootElement.TryGetProperty(property, out var value)
                ? value.GetString() ?? string.Empty
                : string.Empty;
        }
        catch (JsonException)
        {
            return Encoding.UTF8.GetString(payload);
        }
    }

    private static string Limit(string value, int maxLength) =>
        value.Length <= maxLength ? value : value[..(maxLength - 1)] + "…";
}
