using System.Windows.Media;
using DotNetLib.Tray;

namespace DlFovFixer.App.Theming;

/// <summary>
/// The app's own warm palettes, given to DotNetLib.Tray's theme applier in place of its neutral ones.
/// The accent is amber, like the tray icon, and the status colors match the icon's.
/// </summary>
public static class WarmPalettes
{
    public static TrayPalette Light { get; } = new(
        Background: Color.FromRgb(0xF7, 0xF5, 0xF2),
        Surface: Color.FromRgb(0xFF, 0xFF, 0xFF),
        SurfaceRaised: Color.FromRgb(0xFF, 0xFF, 0xFF),
        TextPrimary: Color.FromRgb(0x1A, 0x16, 0x11),
        TextSecondary: Color.FromRgb(0x5C, 0x55, 0x4C),
        Border: Color.FromRgb(0xD6, 0xD0, 0xC7),
        Primary: Color.FromRgb(0x8A, 0x57, 0x0C),
        OnPrimary: Color.FromRgb(0xFF, 0xFF, 0xFF),
        Danger: Color.FromRgb(0xB0, 0x2A, 0x2A),
        Success: Color.FromRgb(0x1A, 0x78, 0x3C),
        Warning: Color.FromRgb(0x96, 0x60, 0x14),
        Focus: Color.FromRgb(0x8A, 0x57, 0x0C));

    public static TrayPalette Dark { get; } = new(
        Background: Color.FromRgb(0x1A, 0x16, 0x11),
        Surface: Color.FromRgb(0x24, 0x1F, 0x19),
        SurfaceRaised: Color.FromRgb(0x2E, 0x28, 0x21),
        TextPrimary: Color.FromRgb(0xF2, 0xEE, 0xE8),
        TextSecondary: Color.FromRgb(0xB8, 0xB0, 0xA5),
        Border: Color.FromRgb(0x45, 0x3D, 0x33),
        Primary: Color.FromRgb(0xF0, 0xAA, 0x3C),
        OnPrimary: Color.FromRgb(0x1A, 0x16, 0x11),
        Danger: Color.FromRgb(0xEB, 0x48, 0x48),
        Success: Color.FromRgb(0x4A, 0xC8, 0x6E),
        Warning: Color.FromRgb(0xF0, 0xAA, 0x3C),
        Focus: Color.FromRgb(0xF0, 0xAA, 0x3C));
}
