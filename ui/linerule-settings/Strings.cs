using System.Globalization;
using Microsoft.Windows.ApplicationModel.Resources;

namespace Linerule.Settings;

internal static class Strings
{
    private static readonly Lazy<ResourceLoader> Loader = new(() =>
        new ResourceLoader(ResourceLoader.GetDefaultResourceFilePath(), "Resources"));

    public static string Get(string id)
    {
        var value = Loader.Value.GetString(id);
        return string.IsNullOrEmpty(value) ? id : value;
    }

    public static string Format(string id, params object[] values) =>
        string.Format(CultureInfo.CurrentCulture, Get(id), values);
}
