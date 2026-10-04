namespace DlFovFixer.App.Tests.Support;

/// <summary>A stored path and two small gameinfo.gi texts, one with the FOV set and one without.</summary>
internal static class GameInfoSamples
{
    public const string GameInfoPath = @"D:\Steam\steamapps\common\Deadlock\game\citadel\gameinfo.gi";

    public const string GameInfoWith249 = "\"GameInfo\"\r\n{\r\n\tConVars\r\n\t{\r\n\t\t\"r_aspectratio\" \"2.49\"\r\n\t}\r\n}\r\n";

    public const string GameInfoWithout = "\"GameInfo\"\r\n{\r\n\tConVars\r\n\t{\r\n\t}\r\n}\r\n";
}
