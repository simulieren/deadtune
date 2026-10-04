namespace DlFovFixer.Core.GameInfo;

/// <summary>
/// Finds blocks in KeyValues text without parsing it, so everything outside the edited keys stays
/// exactly as it was (ADR 0005). Quotes and <c>//</c> line comments are honored, so a brace inside
/// a string or a comment does not change the depth.
/// </summary>
public static class KeyValuesText
{
    /// <summary>The file's own line ending: CRLF if it contains one anywhere, LF otherwise.</summary>
    public static string NewlineOf(string text) => text.Contains("\r\n", StringComparison.Ordinal) ? "\r\n" : "\n";

    /// <summary>The index of the <c>}</c> matching the <c>{</c> at <paramref name="open"/>, or -1.</summary>
    public static int MatchingBrace(string text, int open)
    {
        var depth = 0;
        var inString = false;
        var i = open;
        while (i < text.Length)
        {
            var c = text[i];
            if (inString)
            {
                if (c == '"')
                {
                    inString = false;
                }
            }
            else if (c == '"')
            {
                inString = true;
            }
            else if (c == '/' && i + 1 < text.Length && text[i + 1] == '/')
            {
                var lineEnd = text.IndexOf('\n', i);
                if (lineEnd == -1)
                {
                    return -1;
                }

                i = lineEnd;
                continue;
            }
            else if (c == '{')
            {
                depth++;
            }
            else if (c == '}')
            {
                depth--;
                if (depth == 0)
                {
                    return i;
                }
            }

            i++;
        }

        return -1;
    }

    /// <summary>
    /// The span of the first block headed by <paramref name="name"/> as a whole word, quoted or not,
    /// or null when there is none or it is unbalanced.
    /// </summary>
    public static BlockSpan? FindBlock(string text, string name)
    {
        var start = text.IndexOf(name, StringComparison.Ordinal);
        while (start != -1)
        {
            var end = start + name.Length;
            var before = start > 0 ? text[start - 1] : '\0';
            var after = end < text.Length ? text[end] : '\0';
            if (!IsWordChar(before) && !IsWordChar(after))
            {
                var j = end;
                while (j < text.Length && text[j] is ' ' or '\t' or '\r' or '\n' or '"')
                {
                    j++;
                }

                if (j < text.Length && text[j] == '{')
                {
                    var close = MatchingBrace(text, j);
                    if (close != -1)
                    {
                        return new BlockSpan(j, close);
                    }
                }
            }

            start = text.IndexOf(name, end, StringComparison.Ordinal);
        }

        return null;
    }

    /// <summary>The span of the first, outermost block in the text, or null.</summary>
    public static BlockSpan? FindRootBlock(string text)
    {
        var open = text.IndexOf('{');
        if (open == -1)
        {
            return null;
        }

        var close = MatchingBrace(text, open);
        return close == -1 ? null : new BlockSpan(open, close);
    }

    private static bool IsWordChar(char c) => char.IsLetterOrDigit(c) || char.IsNumber(c) || c == '_';
}
