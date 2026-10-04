using System.Globalization;
using System.Text.RegularExpressions;

namespace DlFovFixer.Core.Updates;

/// <summary>
/// A version like <c>2.0.0</c> or <c>2.0.0-dev</c>. Every rule is covered by
/// contracts/vectors/semantic-version.json.
/// </summary>
public sealed partial record SemanticVersion(int Major, int Minor, int Patch, string? Prerelease) : IComparable<SemanticVersion>
{
    /// <summary>A local build. Directory.Build.props marks every build that is not a release this way.</summary>
    public bool IsDevelopmentBuild => Prerelease == "dev";

    public bool IsPrerelease => Prerelease is not null;

    /// <summary>The version, or null unless the text is exactly a semantic version without build metadata.</summary>
    public static SemanticVersion? Parse(string? text)
    {
        if (text is null || Pattern().Match(text) is not { Success: true } match)
        {
            return null;
        }

        static int? Number(Group group) =>
            int.TryParse(group.Value, NumberStyles.None, CultureInfo.InvariantCulture, out var number) ? number : null;

        return Number(match.Groups["major"]) is { } major
            && Number(match.Groups["minor"]) is { } minor
            && Number(match.Groups["patch"]) is { } patch
            ? new SemanticVersion(major, minor, patch, match.Groups["pre"].Success ? match.Groups["pre"].Value : null)
            : null;
    }

    public int CompareTo(SemanticVersion? other)
    {
        if (other is null)
        {
            return 1;
        }

        var core = (Major, Minor, Patch).CompareTo((other.Major, other.Minor, other.Patch));
        if (core != 0)
        {
            return core;
        }

        // A release sorts after any of its pre-releases.
        return (Prerelease, other.Prerelease) switch
        {
            (null, null) => 0,
            (null, _) => 1,
            (_, null) => -1,
            _ => ComparePrerelease(Prerelease!, other.Prerelease!),
        };
    }

    public static bool operator >(SemanticVersion left, SemanticVersion right) => left.CompareTo(right) > 0;

    public static bool operator <(SemanticVersion left, SemanticVersion right) => left.CompareTo(right) < 0;

    public static bool operator >=(SemanticVersion left, SemanticVersion right) => left.CompareTo(right) >= 0;

    public static bool operator <=(SemanticVersion left, SemanticVersion right) => left.CompareTo(right) <= 0;

    public override string ToString() =>
        Prerelease is null ? $"{Major}.{Minor}.{Patch}" : $"{Major}.{Minor}.{Patch}-{Prerelease}";

    // Identifiers compare one by one: numbers by value and below words, words by ordinal.
    private static int ComparePrerelease(string left, string right)
    {
        var a = left.Split('.');
        var b = right.Split('.');
        for (var i = 0; i < Math.Min(a.Length, b.Length); i++)
        {
            var leftIsNumber = long.TryParse(a[i], NumberStyles.None, CultureInfo.InvariantCulture, out var x);
            var rightIsNumber = long.TryParse(b[i], NumberStyles.None, CultureInfo.InvariantCulture, out var y);
            var order = (leftIsNumber, rightIsNumber) switch
            {
                (true, true) => x.CompareTo(y),
                (true, false) => -1,
                (false, true) => 1,
                _ => string.CompareOrdinal(a[i], b[i]),
            };
            if (order != 0)
            {
                return Math.Sign(order);
            }
        }

        return a.Length.CompareTo(b.Length);
    }

    // \z, not $: in .NET, $ also matches before a trailing newline.
    [GeneratedRegex(
        @"^(?<major>0|[1-9][0-9]*)\.(?<minor>0|[1-9][0-9]*)\.(?<patch>0|[1-9][0-9]*)(?:-(?<pre>[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?\z",
        RegexOptions.CultureInvariant)]
    private static partial Regex Pattern();
}
