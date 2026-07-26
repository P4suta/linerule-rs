using Microsoft.UI.Xaml;
using Linerule.Settings.Models;

namespace Linerule.Settings;

public partial class App : Application
{
    private const string StartupFailureSuffix = ".startup-error.txt";

    public static Window Window { get; private set; } = null!;
    public static SettingsRequest Request { get; private set; } = SettingsRequest.Defaults();
    public static string? ResponsePath { get; private set; }

    public App()
    {
        UnhandledException += App_UnhandledException;
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
        try
        {
            Window = new MainWindow();
            Window.Activate();
        }
        catch (Exception error)
        {
            WriteStartupFailure(error);
            throw;
        }
    }

    private static void App_UnhandledException(
        object sender,
        Microsoft.UI.Xaml.UnhandledExceptionEventArgs args)
    {
        WriteStartupFailure(args.Exception);
    }

    private static void WriteStartupFailure(Exception error)
    {
        if (ResponsePath is not { } responsePath)
        {
            return;
        }
        try
        {
            File.WriteAllText(
                responsePath + StartupFailureSuffix,
                error.ToString());
        }
        catch (Exception writeError) when (
            writeError is IOException or UnauthorizedAccessException
                or ArgumentException or NotSupportedException)
        {
            System.Diagnostics.Debug.WriteLine(
                $"Unable to persist startup failure: {writeError}");
        }
    }
}
