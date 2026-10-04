using System.Windows;
using System.Windows.Media;
using DotNetLib.Tray;

namespace DlFovFixer.App.Shell;

/// <summary>
/// Draws the tray icon: a field-of-view cone opening from a lens, on a dark rounded tile. The shape
/// never changes and the color shows the status, as in 1.0. <see cref="IconFile"/> turns the drawing
/// into a whole .ico file with one frame per tray size, so Windows picks the sharpest one.
/// </summary>
public static class StatusIconFactory
{
    private static readonly Color Tile = Color.FromRgb(26, 22, 17);
    private static readonly Color Lens = Color.FromRgb(18, 15, 11);

    // Coordinates are fractions of the icon's size, fitted to 1.0's Pillow drawing.
    private const double Margin = 0.05;
    private const double CornerRadius = 0.26;
    private const double BorderWidth = 0.06;
    private const double ApexX = 0.5;
    private const double ApexY = 0.82;
    private const double ConeRadius = 0.60;
    private const double HalfAngle = 45;
    private const double RayWidth = 0.013;
    private const double ArcWidth = 0.022;
    private const double LensRadius = 0.08;

    /// <summary>The bytes of a .ico file for <paramref name="color"/>. Must run on an STA thread.</summary>
    public static byte[] CreateIcon(StatusColor color) =>
        IconFile.Create(IconFile.TraySizes, size => Render(size, color));

    /// <summary>Premultiplied BGRA pixels, top row first.</summary>
    internal static byte[] Render(int size, StatusColor color) =>
        IconFile.Render(size, (context, pixels) => Draw(context, pixels, color));

    private static void Draw(DrawingContext context, int size, StatusColor color)
    {
        var (bright, deep) = Palette(color);
        context.PushTransform(new ScaleTransform(size, size));

        var border = new Pen(new SolidColorBrush(bright), BorderWidth);
        var inset = Margin + (BorderWidth / 2);
        context.DrawRoundedRectangle(
            new SolidColorBrush(Tile),
            border,
            new Rect(inset, inset, 1 - (2 * inset), 1 - (2 * inset)),
            CornerRadius - (BorderWidth / 2),
            CornerRadius - (BorderWidth / 2));

        var apex = new Point(ApexX, ApexY);
        context.DrawGeometry(new SolidColorBrush(bright), null, Cone(apex, ConeRadius));

        var rayPen = new Pen(new SolidColorBrush(deep), RayWidth);
        foreach (var offset in new[] { -HalfAngle + 6, -HalfAngle / 2, 0, HalfAngle / 2, HalfAngle - 6 })
        {
            context.DrawLine(rayPen, apex, PointAt(apex, ConeRadius * 0.94, offset));
        }

        context.DrawGeometry(null, new Pen(new SolidColorBrush(deep), ArcWidth), Arc(apex, ConeRadius * 0.56));
        context.DrawEllipse(new SolidColorBrush(Lens), new Pen(new SolidColorBrush(bright), ArcWidth), apex, LensRadius, LensRadius);
        context.Pop();
    }

    private static (Color Bright, Color Deep) Palette(StatusColor color) => color switch
    {
        StatusColor.Green => (Color.FromRgb(74, 200, 110), Color.FromRgb(26, 120, 60)),
        StatusColor.Red => (Color.FromRgb(235, 72, 72), Color.FromRgb(140, 30, 30)),
        _ => (Color.FromRgb(240, 170, 60), Color.FromRgb(150, 96, 20)),
    };

    /// <summary>A point <paramref name="radius"/> away from the apex, <paramref name="degrees"/> off straight up.</summary>
    private static Point PointAt(Point apex, double radius, double degrees)
    {
        var angle = (degrees - 90) * Math.PI / 180;
        return new Point(apex.X + (radius * Math.Cos(angle)), apex.Y + (radius * Math.Sin(angle)));
    }

    private static StreamGeometry Cone(Point apex, double radius)
    {
        var geometry = new StreamGeometry();
        using (var context = geometry.Open())
        {
            context.BeginFigure(apex, isFilled: true, isClosed: true);
            context.LineTo(PointAt(apex, radius, -HalfAngle), isStroked: false, isSmoothJoin: false);
            context.ArcTo(PointAt(apex, radius, HalfAngle), new Size(radius, radius), 0, false, SweepDirection.Clockwise, false, false);
        }

        geometry.Freeze();
        return geometry;
    }

    private static StreamGeometry Arc(Point apex, double radius)
    {
        var geometry = new StreamGeometry();
        using (var context = geometry.Open())
        {
            context.BeginFigure(PointAt(apex, radius, -HalfAngle), isFilled: false, isClosed: false);
            context.ArcTo(PointAt(apex, radius, HalfAngle), new Size(radius, radius), 0, false, SweepDirection.Clockwise, true, false);
        }

        geometry.Freeze();
        return geometry;
    }
}
