namespace DlFovFixer.Core.GameInfo;

/// <summary>
/// Whitespace and line rules that match the Python 1.x app, so pasted text splits and trims the way
/// it always did. .NET's own Trim does not treat U+001C to U+001F as whitespace, and Python does.
/// </summary>
internal static class PlainText
{
    private const char LineSeparator = (char)0x2028;
    private const char ParagraphSeparator = (char)0x2029;

    public static bool IsSpace(char c) => char.IsWhiteSpace(c) || c is >= '\u001c' and <= '\u001f';

    public static string Strip(string text) => StripChars(text, IsSpace);

    public static string StripChars(string text, Func<char, bool> strip)
    {
        var start = 0;
        var end = text.Length;
        while (start < end && strip(text[start]))
        {
            start++;
        }

        while (end > start && strip(text[end - 1]))
        {
            end--;
        }

        return text[start..end];
    }

    /// <summary>Splits on every line break Python's <c>str.splitlines</c> knows, with no trailing empty line.</summary>
    public static IEnumerable<string> Lines(string text)
    {
        var start = 0;
        var i = 0;
        while (i < text.Length)
        {
            var c = text[i];
            if (c is '\n' or '\r' or '\v' or '\f' or '\u001c' or '\u001d' or '\u001e' or '\u0085' or LineSeparator or ParagraphSeparator)
            {
                yield return text[start..i];
                i += c == '\r' && i + 1 < text.Length && text[i + 1] == '\n' ? 2 : 1;
                start = i;
            }
            else
            {
                i++;
            }
        }

        if (start < text.Length)
        {
            yield return text[start..];
        }
    }

    /// <summary>The text up to its first whitespace, after any leading whitespace.</summary>
    public static string FirstWord(string text)
    {
        var start = 0;
        while (start < text.Length && IsSpace(text[start]))
        {
            start++;
        }

        var end = start;
        while (end < text.Length && !IsSpace(text[end]))
        {
            end++;
        }

        return text[start..end];
    }

    /// <summary>The first word and the rest after the whitespace that follows it, or null if there is no rest.</summary>
    public static (string First, string Remainder)? SplitOnce(string text)
    {
        var start = 0;
        while (start < text.Length && IsSpace(text[start]))
        {
            start++;
        }

        var end = start;
        while (end < text.Length && !IsSpace(text[end]))
        {
            end++;
        }

        var rest = end;
        while (rest < text.Length && IsSpace(text[rest]))
        {
            rest++;
        }

        return end > start && rest < text.Length ? (text[start..end], text[rest..]) : null;
    }
}
