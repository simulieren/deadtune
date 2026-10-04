namespace DlFovFixer.Core.Updates;

/// <summary>What the update channel publishes right now, not yet checked: manifest.json and manifest.sig.</summary>
public sealed record ChannelSnapshot(byte[] ManifestBytes, string SignatureBase64);
