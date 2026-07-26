using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using Linerule.Settings.Models;
using Windows.System;

namespace Linerule.Settings.ViewModels;

public sealed partial class MainPageViewModel : ObservableObject
{
    public ObservableCollection<ShortcutItemViewModel> Shortcuts { get; }

    [ObservableProperty]
    public partial bool IsMessageOpen { get; set; }

    [ObservableProperty]
    public partial string MessageTitle { get; set; } = "";

    [ObservableProperty]
    public partial string Message { get; set; } = "";

    [ObservableProperty]
    public partial MessageKind StatusKind { get; set; } = MessageKind.Informational;

    public string? HighlightedAutomationId { get; }

    public MainPageViewModel(SettingsRequest request)
    {
        Shortcuts = new(ShortcutDefinitions.CommandOrder.Select(command =>
        {
            var isHighlighted = request.Highlight == command;
            return new ShortcutItemViewModel(
                command,
                ShortcutDefinitions.Label(command),
                ShortcutDefinitions.Description(command),
                request.Hotkeys.GetValueOrDefault(
                    command,
                    ShortcutDefinitions.Defaults[command]),
                isHighlighted ? request.Error : null,
                isHighlighted);
        }));

        if (request.Error is { Length: > 0 } error)
        {
            ShowMessage(Strings.Get("RegistrationFailedTitle"), error, MessageKind.Error);
        }
        if (request.Highlight is { } highlight)
        {
            HighlightedAutomationId = ShortcutDefinitions.AutomationId(highlight);
        }
    }

    public void BeginRecording(ShortcutItemViewModel item)
    {
        foreach (var shortcut in Shortcuts)
        {
            shortcut.IsRecording = ReferenceEquals(shortcut, item);
        }
        item.Error = null;
        IsMessageOpen = false;
    }

    public void Record(ShortcutItemViewModel item, ShortcutModifiers modifiers, VirtualKey key)
    {
        if (!ShortcutChord.TryCreate(modifiers, key, out var chord, out var error))
        {
            item.Error = error;
            item.IsRecording = false;
            ShowMessage(Strings.Get("ShortcutNotAcceptedTitle"), error, MessageKind.Warning);
            return;
        }

        item.Chord = chord;
        item.Error = null;
        item.IsRecording = false;
    }

    public void CancelRecording(ShortcutItemViewModel item)
    {
        item.Error = null;
        item.IsRecording = false;
    }

    public void Reset()
    {
        var defaults = SettingsRequest.Defaults().Hotkeys;
        foreach (var item in Shortcuts)
        {
            item.Chord = defaults[item.Command];
            item.Error = null;
            item.IsRecording = false;
        }
        ShowMessage(
            Strings.Get("DefaultsRestoredTitle"),
            Strings.Get("DefaultsRestoredMessage"),
            MessageKind.Success);
    }

    public bool TryCreateResponse(
        out SettingsResponse response,
        out ShortcutItemViewModel? firstInvalid)
    {
        firstInvalid = null;
        var occupied = new Dictionary<string, ShortcutItemViewModel>(StringComparer.OrdinalIgnoreCase);
        foreach (var item in Shortcuts)
        {
            item.Error = null;
            if (!ShortcutChord.TryParse(item.Chord, out var canonical, out var error))
            {
                item.Error = error;
                firstInvalid ??= item;
                continue;
            }
            item.Chord = canonical;
            if (occupied.TryGetValue(canonical, out var first))
            {
                first.Error = Strings.Format("DuplicateFormat", item.Label);
                item.Error = Strings.Format("DuplicateFormat", first.Label);
                firstInvalid ??= first;
            }
            else
            {
                occupied.Add(canonical, item);
            }
        }

        if (firstInvalid is not null)
        {
            ShowMessage(
                Strings.Get("ResolveConflictsTitle"),
                Strings.Get("ResolveConflictsMessage"),
                MessageKind.Error);
            response = SettingsResponse.Empty;
            return false;
        }

        response = new SettingsResponse(
            Shortcuts.ToDictionary(item => item.Command, item => item.Chord));
        return true;
    }

    public void ShowError(string title, string message)
    {
        ShowMessage(title, message, MessageKind.Error);
    }

    private void ShowMessage(string title, string message, MessageKind kind)
    {
        MessageTitle = title;
        Message = message;
        StatusKind = kind;
        IsMessageOpen = true;
    }
}

public enum MessageKind
{
    Informational,
    Success,
    Warning,
    Error,
}

public sealed partial class ShortcutItemViewModel : ObservableObject
{
    public string Command { get; }
    public string Label { get; }
    public string Description { get; }
    public string AutomationId => ShortcutDefinitions.AutomationId(Command);
    public string CardAutomationId => $"Card_{Command}";
    public string AccessibleName => Strings.Format("ShortcutAccessibleName", Label, DisplayChord);
    public bool IsHighlighted { get; }
    public Microsoft.UI.Xaml.Thickness HighlightBorderThickness =>
        IsHighlighted
            ? new Microsoft.UI.Xaml.Thickness(2)
            : new Microsoft.UI.Xaml.Thickness(0);

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(DisplayChord))]
    [NotifyPropertyChangedFor(nameof(AccessibleName))]
    public partial string Chord { get; set; }

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(DisplayChord))]
    [NotifyPropertyChangedFor(nameof(AccessibleName))]
    [NotifyPropertyChangedFor(nameof(StatusText))]
    public partial bool IsRecording { get; set; }

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(StatusText))]
    public partial string? Error { get; set; }

    public string DisplayChord => IsRecording ? Strings.Get("RecordingPrompt") : Chord;
    public string StatusText =>
        IsRecording ? Strings.Get("RecordingPrompt") : Error ?? Description;

    public ShortcutItemViewModel(
        string command,
        string label,
        string description,
        string chord,
        string? error = null,
        bool isHighlighted = false)
    {
        Command = command;
        Label = label;
        Description = description;
        Chord = chord;
        Error = error;
        IsHighlighted = isHighlighted;
    }
}
