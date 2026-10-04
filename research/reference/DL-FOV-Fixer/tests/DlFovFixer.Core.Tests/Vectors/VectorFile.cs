using System.Text.Json;
using DlFovFixer.Core.GameInfo;

namespace DlFovFixer.Core.Tests.Vectors;

/// <summary>
/// Reads the shared behavior vectors in contracts/vectors, which the build copies next to the
/// tests. The retired Python 1.x app was tested against the same files until 2.0.0.
/// </summary>
internal static class VectorFile
{
    public static string Directory => Path.Combine(AppContext.BaseDirectory, "vectors");

    public static JsonElement Load(string name)
    {
        using var document = JsonDocument.Parse(File.ReadAllText(Path.Combine(Directory, name)));
        return document.RootElement.Clone();
    }

    public static JsonElement Case(string file, string name, string section = "cases") =>
        Load(file).GetProperty(section).EnumerateArray().Single(c => c.GetProperty("name").GetString() == name);

    /// <summary>Every case in LF form, plus a CRLF form when the case has text input.</summary>
    public static TheoryData<string, bool> CasesWithLineEndings(string file)
    {
        var data = new TheoryData<string, bool>();
        foreach (var item in Load(file).GetProperty("cases").EnumerateArray())
        {
            var name = item.GetProperty("name").GetString()!;
            data.Add(name, false);
            if (item.GetProperty("input").ValueKind != JsonValueKind.Null)
            {
                data.Add(name, true);
            }
        }

        return data;
    }

    public static TheoryData<string> CaseNames(string file, string section = "cases")
    {
        var data = new TheoryData<string>();
        foreach (var item in Load(file).GetProperty(section).EnumerateArray())
        {
            data.Add(item.GetProperty("name").GetString()!);
        }

        return data;
    }

    /// <summary>A text field: an array of lines joined with LF, then CRLF if asked for, or null.</summary>
    public static string? Text(JsonElement lines, bool crlf = false)
    {
        if (lines.ValueKind == JsonValueKind.Null)
        {
            return null;
        }

        var text = string.Join("\n", lines.EnumerateArray().Select(line => line.GetString()));
        return crlf ? text.Replace("\n", "\r\n", StringComparison.Ordinal) : text;
    }

    public static string? String(JsonElement value) =>
        value.ValueKind == JsonValueKind.Null ? null : value.GetString();

    public static IReadOnlyList<TweakEntry> Entries(JsonElement pairs) =>
        [.. pairs.EnumerateArray().Select(pair => new TweakEntry(pair[0].GetString()!, pair[1].GetString()!))];

    /// <summary>Pairs as "key=value" text, so a failing assertion shows the whole list.</summary>
    public static string[] PairTexts(JsonElement pairs) =>
        [.. pairs.EnumerateArray().Select(pair => $"{pair[0].GetString()}={pair[1].GetString()}")];

    public static string[] PairTexts(IEnumerable<TweakEntry> entries) =>
        [.. entries.Select(entry => $"{entry.Key}={entry.Value}")];

    public static string[] PairTexts(IEnumerable<KeyMerge> results) =>
        [.. results.Select(result => $"{result.Key}={Code(result.Action)}")];

    /// <summary>The action names the vectors use, which are the Python 1.x app's.</summary>
    public static string Code(MergeAction action) => action switch
    {
        MergeAction.Updated => "updated",
        MergeAction.Added => "added",
        MergeAction.SkippedBlock => "skipped_block",
        MergeAction.NoBlock => "no_block",
        MergeAction.NoRoot => "no_root",
        _ => throw new ArgumentOutOfRangeException(nameof(action), action, null),
    };
}
