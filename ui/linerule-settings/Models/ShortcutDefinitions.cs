using Windows.System;

namespace Linerule.Settings.Models;

[Flags]
public enum ShortcutModifiers
{
    None = 0,
    Control = 1,
    Alt = 2,
    Shift = 4,
    Meta = 8,
}

public static class ShortcutDefinitions
{
    public static readonly IReadOnlyList<string> CommandOrder =
    [
        "cycle_mode",
        "cycle_effect",
        "toggle_on_off",
        "thicker",
        "thinner",
        "more_opaque",
        "less_opaque",
        "toggle_guide",
        "quit",
    ];

    public static readonly IReadOnlyDictionary<string, string> Defaults =
        new Dictionary<string, string>
        {
            ["cycle_mode"] = "Ctrl+Alt+R",
            ["cycle_effect"] = "Ctrl+Alt+E",
            ["toggle_on_off"] = "Ctrl+Alt+H",
            ["thicker"] = "Ctrl+Alt+Up",
            ["thinner"] = "Ctrl+Alt+Down",
            ["more_opaque"] = "Ctrl+Alt+Right",
            ["less_opaque"] = "Ctrl+Alt+Left",
            ["toggle_guide"] = "Ctrl+Alt+K",
            ["quit"] = "Ctrl+Alt+Q",
        };

    public static string AutomationId(string command) => $"Shortcut_{command}";

    public static string Label(string command) => command switch
    {
        "cycle_mode" => Strings.Get("LabelCycleMode"),
        "cycle_effect" => Strings.Get("LabelCycleEffect"),
        "toggle_on_off" => Strings.Get("LabelToggleOnOff"),
        "thicker" => Strings.Get("LabelThicker"),
        "thinner" => Strings.Get("LabelThinner"),
        "more_opaque" => Strings.Get("LabelMoreOpaque"),
        "less_opaque" => Strings.Get("LabelLessOpaque"),
        "toggle_guide" => Strings.Get("LabelToggleGuide"),
        "quit" => Strings.Get("LabelQuit"),
        _ => command,
    };

    public static string Description(string command) => command switch
    {
        "toggle_on_off" => Strings.Get("DescriptionToggleOnOff"),
        "toggle_guide" => Strings.Get("DescriptionToggleGuide"),
        "quit" => Strings.Get("DescriptionQuit"),
        _ => Strings.Get("DescriptionDefault"),
    };
}

public static class ShortcutChord
{
    public static bool IsModifier(VirtualKey key) => key is
        VirtualKey.Control or VirtualKey.Menu or VirtualKey.Shift
        or VirtualKey.LeftWindows or VirtualKey.RightWindows;

    public static bool TryCreate(
        ShortcutModifiers modifiers,
        VirtualKey key,
        out string chord,
        out string error)
    {
        if (modifiers == ShortcutModifiers.None)
        {
            chord = "";
            error = Strings.Get("ModifierRequired");
            return false;
        }

        var keyName = KeyName(key);
        if (keyName is null)
        {
            chord = "";
            error = Strings.Get("UnsupportedKey");
            return false;
        }

        chord = Format(modifiers, keyName);
        error = "";
        return true;
    }

    public static bool TryParse(string value, out string canonical, out string error)
    {
        var parts = value.Split('+', StringSplitOptions.TrimEntries);
        var modifiers = ShortcutModifiers.None;
        string? key = null;
        foreach (var part in parts)
        {
            if (part.Equals("Ctrl", StringComparison.OrdinalIgnoreCase)
                || part.Equals("Control", StringComparison.OrdinalIgnoreCase))
            {
                modifiers |= ShortcutModifiers.Control;
            }
            else if (part.Equals("Alt", StringComparison.OrdinalIgnoreCase))
            {
                modifiers |= ShortcutModifiers.Alt;
            }
            else if (part.Equals("Shift", StringComparison.OrdinalIgnoreCase))
            {
                modifiers |= ShortcutModifiers.Shift;
            }
            else if (part.Equals("Meta", StringComparison.OrdinalIgnoreCase)
                || part.Equals("Win", StringComparison.OrdinalIgnoreCase)
                || part.Equals("Windows", StringComparison.OrdinalIgnoreCase))
            {
                modifiers |= ShortcutModifiers.Meta;
            }
            else if (key is null && NormalizeKey(part) is { } normalized)
            {
                key = normalized;
            }
            else
            {
                canonical = "";
                error = Strings.Get("InvalidChord");
                return false;
            }
        }

        if (modifiers == ShortcutModifiers.None)
        {
            canonical = "";
            error = Strings.Get("ModifierRequired");
            return false;
        }
        if (key is null)
        {
            canonical = "";
            error = Strings.Get("ChooseKey");
            return false;
        }

        canonical = Format(modifiers, key);
        error = "";
        return true;
    }

    private static string Format(ShortcutModifiers modifiers, string key)
    {
        var parts = new List<string>(5);
        if (modifiers.HasFlag(ShortcutModifiers.Control))
        {
            parts.Add("Ctrl");
        }
        if (modifiers.HasFlag(ShortcutModifiers.Alt))
        {
            parts.Add("Alt");
        }
        if (modifiers.HasFlag(ShortcutModifiers.Shift))
        {
            parts.Add("Shift");
        }
        if (modifiers.HasFlag(ShortcutModifiers.Meta))
        {
            parts.Add("Meta");
        }
        parts.Add(key);
        return string.Join('+', parts);
    }

    private static string? NormalizeKey(string key)
    {
        if (key.Length == 1 && char.IsAsciiLetter(key[0]))
        {
            return char.ToUpperInvariant(key[0]).ToString();
        }
        return key.ToUpperInvariant() switch
        {
            "UP" => "Up",
            "DOWN" => "Down",
            "LEFT" => "Left",
            "RIGHT" => "Right",
            "[" or "]" or "-" or "=" => key,
            _ => null,
        };
    }

    private static string? KeyName(VirtualKey key)
    {
        if (key is >= VirtualKey.A and <= VirtualKey.Z)
        {
            return key.ToString();
        }
        return key switch
        {
            VirtualKey.Up => "Up",
            VirtualKey.Down => "Down",
            VirtualKey.Left => "Left",
            VirtualKey.Right => "Right",
            (VirtualKey)219 => "[",
            (VirtualKey)221 => "]",
            (VirtualKey)189 => "-",
            (VirtualKey)187 => "=",
            _ => null,
        };
    }
}
