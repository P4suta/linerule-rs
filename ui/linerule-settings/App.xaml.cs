using Microsoft.UI.Xaml;
using Linerule.Settings.Models;

namespace Linerule.Settings;

public partial class App : Application
{
    public static Window Window { get; private set; } = null!;
    public static SettingsRequest Request { get; private set; } = SettingsRequest.Defaults();
    public static string? ResponsePath { get; private set; }

    public App()
    {
        InitializeComponent();
    }

    protected override void OnLaunched(Microsoft.UI.Xaml.LaunchActivatedEventArgs args)
    {
        try
        {
            (Request, ResponsePath) = SettingsProtocol.ReadLaunch(Environment.GetCommandLineArgs());
        }
        catch (Exception error) when (
            error is IOException or UnauthorizedAccessException
                or ArgumentException or System.Text.Json.JsonException)
        {
            Request = new SettingsRequest(
                ShortcutDefinitions.Defaults,
                Strings.Format("RequestLoadFailedFormat", error.Message));
            ResponsePath = null;
        }
        Window = new MainWindow();
        Window.Activate();
    }
}
