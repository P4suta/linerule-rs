using System.Text.Json;
using System.Text.Json.Serialization;

namespace Linerule.Settings.Models;

public sealed record SettingsRequest(
    [property: JsonPropertyName("hotkeys")] IReadOnlyDictionary<string, string> Hotkeys,
    [property: JsonPropertyName("error")] string? Error = null,
    [property: JsonPropertyName("highlight")] string? Highlight = null)
{
    public static SettingsRequest Defaults() => new(ShortcutDefinitions.Defaults);
}

public sealed record SettingsResponse(
    [property: JsonPropertyName("hotkeys")] IReadOnlyDictionary<string, string> Hotkeys)
{
    public static SettingsResponse Empty { get; } =
        new(new Dictionary<string, string>());
}

public static class SettingsProtocol
{
    public static (SettingsRequest Request, string? ResponsePath) ReadLaunch(string[] args)
    {
        string? requestPath = null;
        string? responsePath = null;
        for (var index = 1; index + 1 < args.Length; index += 2)
        {
            switch (args[index])
            {
                case "--request":
                    requestPath = args[index + 1];
                    break;
                case "--response":
                    responsePath = args[index + 1];
                    break;
            }
        }

        responsePath = responsePath is null
            ? null
            : ValidateProtocolPath(responsePath, mustExist: false);
        if (requestPath is null)
        {
            return (SettingsRequest.Defaults(), responsePath);
        }

        requestPath = ValidateProtocolPath(requestPath, mustExist: true);
        var request = JsonSerializer.Deserialize(
            File.ReadAllText(requestPath),
            SettingsJsonContext.Default.SettingsRequest);
        if (request?.Hotkeys is null)
        {
            throw new JsonException("Settings request must contain a hotkeys object.");
        }
        return (request, responsePath);
    }

    public static void WriteResponse(string path, SettingsResponse response)
    {
        path = ValidateProtocolPath(path, mustExist: false);
        var temporary = $"{path}.tmp";
        File.WriteAllText(
            temporary,
            JsonSerializer.Serialize(response, SettingsJsonContext.Default.SettingsResponse));
        File.Move(temporary, path, true);
    }

    private static string ValidateProtocolPath(string path, bool mustExist)
    {
        var fullPath = Path.GetFullPath(path);
        if (!string.Equals(Path.GetExtension(fullPath), ".json", StringComparison.OrdinalIgnoreCase))
        {
            throw new ArgumentException("Settings protocol files must use the .json extension.");
        }
        var parent = Path.GetDirectoryName(fullPath);
        if (parent is null || !Directory.Exists(parent))
        {
            throw new ArgumentException("Settings protocol directory does not exist.");
        }
        if (mustExist && !File.Exists(fullPath))
        {
            throw new FileNotFoundException("Settings request file was not found.", fullPath);
        }
        return fullPath;
    }
}

[JsonSourceGenerationOptions(
    PropertyNameCaseInsensitive = false,
    WriteIndented = true)]
[JsonSerializable(typeof(SettingsRequest))]
[JsonSerializable(typeof(SettingsResponse))]
internal sealed partial class SettingsJsonContext : JsonSerializerContext
{
}
