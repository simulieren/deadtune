using System.Windows.Media;
using DlFovFixer.App.Theming;
using DotNetLib.Tray;

namespace DlFovFixer.App.Tests.Theming;

/// <summary>
/// The app's warm palettes replace the kit's neutral ones, so they must meet the same bar: readable
/// text (WCAG AA, 4.5 to 1) and status and focus colors that stand out (3 to 1).
/// </summary>
public sealed class WarmPalettesTests
{
    public static TheoryData<string> Palettes() => ["Light", "Dark"];

    [Theory]
    [MemberData(nameof(Palettes))]
    public void Text_IsReadableOnEverySurface(string name)
    {
        var palette = Palette(name);

        foreach (var surface in new[] { palette.Background, palette.Surface, palette.SurfaceRaised })
        {
            Assert.True(Contrast(palette.TextPrimary, surface) >= 4.5, $"{name} TextPrimary on {surface}");
            Assert.True(Contrast(palette.TextSecondary, surface) >= 4.5, $"{name} TextSecondary on {surface}");
        }
    }

    [Theory]
    [MemberData(nameof(Palettes))]
    public void OnPrimary_IsReadableOnPrimary(string name)
    {
        var palette = Palette(name);

        Assert.True(Contrast(palette.OnPrimary, palette.Primary) >= 4.5);
    }

    [Theory]
    [MemberData(nameof(Palettes))]
    public void StatusAndFocus_StandOutFromTheBackground(string name)
    {
        var palette = Palette(name);

        // WCAG's 3 to 1 for graphics and focus indicators.
        foreach (var color in new[] { palette.Danger, palette.Success, palette.Warning, palette.Focus })
        {
            Assert.True(Contrast(color, palette.Background) >= 3, $"{name} {color}");
        }
    }

    private static TrayPalette Palette(string name) => name == "Dark" ? WarmPalettes.Dark : WarmPalettes.Light;

    private static double Contrast(Color a, Color b)
    {
        var (lighter, darker) = (Luminance(a), Luminance(b)) is var (x, y) && x > y ? (x, y) : (y, x);
        return (lighter + 0.05) / (darker + 0.05);
    }

    private static double Luminance(Color color)
    {
        static double Channel(byte value)
        {
            var c = value / 255.0;
            return c <= 0.03928 ? c / 12.92 : Math.Pow((c + 0.055) / 1.055, 2.4);
        }

        return (0.2126 * Channel(color.R)) + (0.7152 * Channel(color.G)) + (0.0722 * Channel(color.B));
    }
}
