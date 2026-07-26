using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Windows.Graphics;

namespace Linerule.Settings;

public sealed partial class MainWindow : Window
{
    private const double WidthDip = 760;
    private const double HeightDip = 800;

    public MainWindow()
    {
        InitializeComponent();

        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);

        RootPage.Loaded += RootPage_Loaded;
    }

    private void RootPage_Loaded(object sender, RoutedEventArgs e)
    {
        RootPage.Loaded -= RootPage_Loaded;
        var scale = RootPage.XamlRoot?.RasterizationScale ?? 1.0;
        var workArea = DisplayArea
            .GetFromWindowId(AppWindow.Id, DisplayAreaFallback.Primary)
            .WorkArea;
        var width = Math.Min(
            workArea.Width,
            checked((int)Math.Round(WidthDip * scale)));
        var height = Math.Min(
            workArea.Height,
            checked((int)Math.Round(HeightDip * scale)));
        AppWindow.Resize(new SizeInt32(width, height));
        AppWindow.Move(
            new PointInt32(
                workArea.X + (workArea.Width - width) / 2,
                workArea.Y + (workArea.Height - height) / 2));
    }
}
