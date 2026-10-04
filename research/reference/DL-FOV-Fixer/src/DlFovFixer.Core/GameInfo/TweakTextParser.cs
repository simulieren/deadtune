using System.Text.RegularExpressions;

namespace DlFovFixer.Core.GameInfo;

/// <summary>
/// Turns a pasted config into tweaks. Headers, line numbers and comments are dropped, and each key
/// is routed by its shape: <c>setting.*</c> goes to video.txt, a key starting with a capital to
/// SceneSystem, and anything else to ConVars.
/// </summary>
public static partial class TweakTextParser
{
    public static ParsedTweaks Parse(string text)
    {
        string? value = null;
        var conVars = new OrderedEntries();
        var sceneSystem = new OrderedEntries();
        var video = new OrderedEntries();

        foreach (var raw in PlainText.Lines(text))
        {
            var line = Clean(raw);
            if (line.Length == 0 || !AnyLetter().IsMatch(line) || SplitPair(line) is not { } entry)
            {
                continue;
            }

            if (entry.Key == AspectRatio.ConVarName)
            {
                value = entry.Value;
                continue;
            }

            var target = entry.Key.StartsWith("setting.", StringComparison.Ordinal) ? video
                : char.IsUpper(entry.Key[0]) ? sceneSystem
                : conVars;
            target.Set(entry);
        }

        return new ParsedTweaks(value, conVars.ToList(), sceneSystem.ToList(), video.ToList());
    }

    /// <summary>Merges two stored lists: incoming values win, and first-seen order is kept.</summary>
    public static IReadOnlyList<TweakEntry> MergeLists(IEnumerable<TweakEntry> existing, IEnumerable<TweakEntry> incoming)
    {
        var merged = new OrderedEntries();
        foreach (var entry in existing.Concat(incoming))
        {
            merged.Set(entry);
        }

        return merged.ToList();
    }

    private static string Clean(string line)
    {
        line = PlainText.Strip(Comment().Replace(line, string.Empty));
        line = PlainText.Strip(ListNumber().Replace(line, string.Empty));
        return PlainText.Strip(PlainText.StripChars(line, c => c is '{' or '}'));
    }

    private static TweakEntry? SplitPair(string line)
    {
        if (PlainText.SplitOnce(line) is not var (first, rest))
        {
            return null;
        }

        var key = PlainText.Strip(PlainText.StripChars(PlainText.Strip(first), c => c == '"'));
        if (!Key().IsMatch(key))
        {
            return null;
        }

        var valueText = PlainText.Strip(rest);
        string value;
        if (valueText.StartsWith('"'))
        {
            var quoted = QuotedValue().Match(valueText);
            value = quoted.Success ? quoted.Groups[1].Value : PlainText.StripChars(valueText, c => c == '"');
        }
        else
        {
            value = PlainText.FirstWord(valueText);
        }

        return new TweakEntry(key, value);
    }

    [GeneratedRegex("(//|#).*$", RegexOptions.CultureInvariant)]
    private static partial Regex Comment();

    [GeneratedRegex(@"^\s*\d+[.)]\s*", RegexOptions.CultureInvariant)]
    private static partial Regex ListNumber();

    [GeneratedRegex(@"^[A-Za-z_][\w.]*\z", RegexOptions.CultureInvariant)]
    private static partial Regex Key();

    [GeneratedRegex("^\"([^\"]*)\"", RegexOptions.CultureInvariant)]
    private static partial Regex QuotedValue();

    [GeneratedRegex("[A-Za-z]", RegexOptions.CultureInvariant)]
    private static partial Regex AnyLetter();

    /// <summary>Keys in first-seen order, where a later value for the same key wins.</summary>
    private sealed class OrderedEntries
    {
        private readonly List<string> _order = [];
        private readonly Dictionary<string, string> _values = new(StringComparer.Ordinal);

        public void Set(TweakEntry entry)
        {
            if (!_values.ContainsKey(entry.Key))
            {
                _order.Add(entry.Key);
            }

            _values[entry.Key] = entry.Value;
        }

        public List<TweakEntry> ToList() => [.. _order.Select(key => new TweakEntry(key, _values[key]))];
    }
}
