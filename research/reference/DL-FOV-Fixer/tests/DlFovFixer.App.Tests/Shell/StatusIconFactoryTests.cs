using System.IO;
using DlFovFixer.App.Shell;
using DlFovFixer.App.Tests.Support;
using DotNetLib.Tray;

namespace DlFovFixer.App.Tests.Shell;

public sealed class StatusIconFactoryTests
{
    [Theory]
    [InlineData(StatusColor.Green)]
    [InlineData(StatusColor.Amber)]
    [InlineData(StatusColor.Red)]
    public void CreateIcon_IsAnIcoFileWindowsCanLoad(StatusColor color)
    {
        var bytes = Sta.Run(() => StatusIconFactory.CreateIcon(color));

        Assert.Equal(IconFile.TraySizes.Count, BitConverter.ToInt16(bytes, 4));
        using var stream = new MemoryStream(bytes);
        using var icon = new System.Drawing.Icon(stream, 32, 32);
        Assert.Equal(32, icon.Width);
    }

    [Fact]
    public void Render_ColorsTheConeByStatus()
    {
        // The cone's middle, straight above the lens, carries the bright status color.
        static (byte R, byte G, byte B) Middle(StatusColor color)
        {
            const int Size = 64;
            var pixels = Sta.Run(() => StatusIconFactory.Render(Size, color));
            var i = ((int)(Size * 0.45) * Size + (Size / 2) + 4) * 4;
            return (pixels[i + 2], pixels[i + 1], pixels[i]);
        }

        Assert.Equal((74, 200, 110), Middle(StatusColor.Green));
        Assert.Equal((240, 170, 60), Middle(StatusColor.Amber));
        Assert.Equal((235, 72, 72), Middle(StatusColor.Red));
    }

    [Fact]
    public void Render_LeavesTheCornersClear()
    {
        var pixels = Sta.Run(() => StatusIconFactory.Render(32, StatusColor.Green));

        Assert.Equal(0, pixels[3]);
    }
}
