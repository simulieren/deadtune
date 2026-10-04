using System.Text.RegularExpressions;

namespace DlFovFixer.Core.GameInfo;

/// <summary>
/// Applies the chosen <c>r_aspectratio</c> and the stored tweaks to the text of gameinfo.gi. The
/// value always wins over a pasted <c>r_aspectratio</c>, and applying twice changes nothing.
/// </summary>
public static partial class GameInfoMerge
{
    /// <summary>The <c>r_aspectratio</c> value in the ConVars block, or anywhere if there is no block; null if unset.</summary>
    public static string? ReadValue(string text)
    {
        var span = KeyValuesText.FindBlock(text, "ConVars");
        var block = span is { } s ? text[s.Open..(s.Close + 1)] : text;
        var match = AspectRatioEntry().Match(block);
        return match.Success ? match.Groups[1].Value : null;
    }

    /// <summary>Merges the value into ConVars and, when tweaks are on, the tweaks into ConVars and SceneSystem.</summary>
    public static GameInfoMergeOutcome Apply(
        string text,
        string? value,
        IReadOnlyList<TweakEntry> conVars,
        IReadOnlyList<TweakEntry> sceneSystem,
        bool applyTweaks)
    {
        var previous = ReadValue(text);

        List<TweakEntry> conVarEntries = [new(AspectRatio.ConVarName, AspectRatio.Normalize(value) ?? AspectRatio.DefaultValue)];
        if (applyTweaks)
        {
            conVarEntries.AddRange(conVars.Where(entry => entry.Key != AspectRatio.ConVarName));
        }

        var conVarMerge = BlockMerge.Merge(text, "ConVars", conVarEntries, quoted: true, create: true);
        var merged = conVarMerge.Text;

        IReadOnlyList<KeyMerge> sceneResults = [];
        if (applyTweaks && sceneSystem.Count > 0)
        {
            var sceneMerge = BlockMerge.Merge(merged, "SceneSystem", sceneSystem, quoted: false, create: true);
            merged = sceneMerge.Text;
            sceneResults = sceneMerge.Keys;
        }

        return new GameInfoMergeOutcome(merged, !string.Equals(merged, text, StringComparison.Ordinal), previous, conVarMerge.Keys, sceneResults);
    }

    [GeneratedRegex("\"r_aspectratio\"[ \\t]*\"([^\"]*)\"", RegexOptions.CultureInvariant)]
    private static partial Regex AspectRatioEntry();
}
