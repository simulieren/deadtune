using System.Globalization;
using System.Text.RegularExpressions;

namespace DlFovFixer.Core.GameInfo;

/// <summary>
/// The <c>r_aspectratio</c> ConVar that sets Deadlock's field of view. A higher value is a wider
/// view.
/// </summary>
public static partial class AspectRatio
{
    /// <summary>The ConVar this app exists to keep in gameinfo.gi.</summary>
    public const string ConVarName = "r_aspectratio";

    /// <summary>The value applied when the stored one is missing or invalid.</summary>
    public const string DefaultValue = "2";

    // Degrees are close to linear in the value, fitted to the community's data points
    // (1.75 is 80, 2.15 is 90, 2.49 is 100, 3.00 is 115).
    private const double DegreesPerUnit = 28.0;
    private const double DegreesAtZero = 31.0;

    /// <summary>
    /// Cleans a typed value, keeping the user's own form such as "2" or "2.15". A comma counts as a
    /// decimal point. Returns null unless it is a plain number from 0.5 to 6.
    /// </summary>
    public static string? Normalize(string? raw)
    {
        if (raw is null)
        {
            return null;
        }

        var text = PlainText.Strip(raw).Replace(',', '.');
        if (TryParse(text) is not { } number || number < 0.5 || number > 6.0)
        {
            return null;
        }

        return text;
    }

    /// <summary>The approximate horizontal field of view in degrees, or null if it is not a number.</summary>
    public static int? ToFovDegrees(string value) =>
        TryParse(PlainText.Strip(value)) is { } number
            ? (int)Math.Round(DegreesPerUnit * number + DegreesAtZero, MidpointRounding.ToEven)
            : null;

    private static double? TryParse(string text) =>
        PlainNumber().IsMatch(text) && double.TryParse(text, NumberStyles.Float, CultureInfo.InvariantCulture, out var number)
            ? number
            : null;

    [GeneratedRegex(@"^[+-]?(?:[0-9]+\.?[0-9]*|\.[0-9]+)(?:[eE][+-]?[0-9]+)?\z", RegexOptions.CultureInvariant)]
    private static partial Regex PlainNumber();
}
