namespace DlFovFixer.Core.GameInfo;

/// <summary>
/// The result of merging into cfg/video.txt. <see cref="Text"/> is null only when the file did not
/// exist and there was nothing to write. Nothing should be written unless <see cref="Changed"/>.
/// </summary>
public sealed record VideoConfigOutcome(string? Text, bool Changed, bool Created, IReadOnlyList<KeyMerge> Keys);
