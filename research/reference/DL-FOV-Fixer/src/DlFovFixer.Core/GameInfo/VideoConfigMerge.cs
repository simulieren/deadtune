using System.Text;

namespace DlFovFixer.Core.GameInfo;

/// <summary>
/// Merges pasted <c>setting.*</c> entries into cfg/video.txt beside gameinfo.gi, the same way keys
/// are merged into a gameinfo.gi block.
/// </summary>
public static class VideoConfigMerge
{
    /// <summary>
    /// Merges <paramref name="entries"/> into <paramref name="existing"/>, the file's text, or null
    /// when the file does not exist. A new file uses CRLF. A file with no block is replaced by a
    /// fresh one, which is why the caller keeps a backup first.
    /// </summary>
    public static VideoConfigOutcome Merge(string? existing, IReadOnlyList<TweakEntry> entries)
    {
        if (entries.Count == 0)
        {
            return new VideoConfigOutcome(existing, false, false, []);
        }

        if (existing is null)
        {
            return new VideoConfigOutcome(FreshFile(entries, "\r\n"), true, true, AllAdded(entries));
        }

        var newline = KeyValuesText.NewlineOf(existing);
        if (KeyValuesText.FindRootBlock(existing) is not { } span)
        {
            return new VideoConfigOutcome(FreshFile(entries, newline), true, false, AllAdded(entries));
        }

        var inner = existing[(span.Open + 1)..span.Close];
        var results = new List<KeyMerge>(entries.Count);
        var adds = new List<TweakEntry>();
        foreach (var entry in entries)
        {
            var (merged, action) = BlockMerge.MergeOne(inner, entry, quoted: true);
            if (merged is null)
            {
                adds.Add(entry);
            }
            else
            {
                inner = merged;
            }

            results.Add(new KeyMerge(entry.Key, action));
        }

        if (adds.Count > 0)
        {
            inner = newline + Lines(adds, newline) + inner;
        }

        var text = existing[..(span.Open + 1)] + inner + existing[span.Close..];
        return new VideoConfigOutcome(text, !string.Equals(text, existing, StringComparison.Ordinal), false, results);
    }

    private static string FreshFile(IReadOnlyList<TweakEntry> entries, string newline) =>
        $"\"video.cfg\"{newline}{{{newline}{Lines(entries, newline)}}}{newline}";

    private static string Lines(IEnumerable<TweakEntry> entries, string newline)
    {
        var lines = new StringBuilder();
        foreach (var entry in entries)
        {
            lines.Append($"\t\"{entry.Key}\"\t\t\"{entry.Value}\"").Append(newline);
        }

        return lines.ToString();
    }

    private static KeyMerge[] AllAdded(IEnumerable<TweakEntry> entries) =>
        [.. entries.Select(entry => new KeyMerge(entry.Key, MergeAction.Added))];
}
