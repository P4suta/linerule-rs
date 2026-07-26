using Linerule.Settings.Models;
using Linerule.Settings.ViewModels;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.System;
using Windows.UI.Core;

namespace Linerule.Settings;

public sealed partial class MainPage : Page
{
    public MainPageViewModel ViewModel { get; } = new(App.Request);

    public MainPage()
    {
        InitializeComponent();
    }

    private void Page_Loaded(object sender, RoutedEventArgs e)
    {
        if (ViewModel.HighlightedAutomationId is { } automationId)
        {
            FocusShortcut(automationId);
        }
    }

    private void ShortcutButton_Click(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: ShortcutItemViewModel item } button)
        {
            ViewModel.BeginRecording(item);
            button.Focus(FocusState.Programmatic);
        }
    }

    private void ShortcutButton_PreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (sender is not Button { Tag: ShortcutItemViewModel item } || !item.IsRecording)
        {
            return;
        }

        e.Handled = true;
        if (e.Key == VirtualKey.Escape)
        {
            ViewModel.CancelRecording(item);
            return;
        }
        if (ShortcutChord.IsModifier(e.Key))
        {
            return;
        }

        var modifiers = ShortcutModifiers.None;
        if (IsDown(VirtualKey.Control))
        {
            modifiers |= ShortcutModifiers.Control;
        }
        if (IsDown(VirtualKey.Menu))
        {
            modifiers |= ShortcutModifiers.Alt;
        }
        if (IsDown(VirtualKey.Shift))
        {
            modifiers |= ShortcutModifiers.Shift;
        }
        if (IsDown(VirtualKey.LeftWindows) || IsDown(VirtualKey.RightWindows))
        {
            modifiers |= ShortcutModifiers.Meta;
        }

        ViewModel.Record(item, modifiers, e.Key);
    }

    private async void ResetButton_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new ContentDialog
        {
            Title = Strings.Get("RestoreDialogTitle"),
            Content = Strings.Get("RestoreDialogContent"),
            PrimaryButtonText = Strings.Get("RestoreDialogPrimary"),
            CloseButtonText = Strings.Get("Cancel"),
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = XamlRoot,
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetAutomationId(
            dialog,
            "ResetConfirmation");
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            ViewModel.Reset();
        }
    }

    private void CancelButton_Click(object sender, RoutedEventArgs e)
    {
        App.Window.Close();
    }

    private void SaveButton_Click(object sender, RoutedEventArgs e)
    {
        if (!ViewModel.TryCreateResponse(out var response, out var firstInvalid))
        {
            if (firstInvalid is not null)
            {
                FocusShortcut(firstInvalid.AutomationId);
            }
            return;
        }

        if (App.ResponsePath is { } responsePath)
        {
            try
            {
                SettingsProtocol.WriteResponse(responsePath, response);
            }
            catch (Exception error) when (
                error is IOException or UnauthorizedAccessException or ArgumentException)
            {
                ViewModel.ShowError(Strings.Get("SettingsSaveFailedTitle"), error.Message);
                return;
            }
        }
        App.Window.Close();
    }

    public static InfoBarSeverity ToInfoBarSeverity(MessageKind kind) => kind switch
    {
        MessageKind.Informational => InfoBarSeverity.Informational,
        MessageKind.Success => InfoBarSeverity.Success,
        MessageKind.Warning => InfoBarSeverity.Warning,
        MessageKind.Error => InfoBarSeverity.Error,
        _ => InfoBarSeverity.Informational,
    };

    private void FocusShortcut(string automationId)
    {
        var element = FindByAutomationId(this, automationId);
        element?.Focus(FocusState.Programmatic);
    }

    private static Control? FindByAutomationId(DependencyObject root, string automationId)
    {
        var count = Microsoft.UI.Xaml.Media.VisualTreeHelper.GetChildrenCount(root);
        for (var index = 0; index < count; index++)
        {
            var child = Microsoft.UI.Xaml.Media.VisualTreeHelper.GetChild(root, index);
            if (child is Control control
                && Microsoft.UI.Xaml.Automation.AutomationProperties.GetAutomationId(control)
                    == automationId)
            {
                return control;
            }
            if (FindByAutomationId(child, automationId) is { } descendant)
            {
                return descendant;
            }
        }
        return null;
    }

    private static bool IsDown(VirtualKey key)
    {
        return InputKeyboardSource.GetKeyStateForCurrentThread(key)
            .HasFlag(CoreVirtualKeyStates.Down);
    }
}
