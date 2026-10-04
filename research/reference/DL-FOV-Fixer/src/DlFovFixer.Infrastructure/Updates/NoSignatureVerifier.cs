using DlFovFixer.Core.Updates;

namespace DlFovFixer.Infrastructure.Updates;

/// <summary>For a build without a manifest key: no signature is ever valid, so nothing is trusted.</summary>
public sealed class NoSignatureVerifier : ISignatureVerifier
{
    public bool Verify(ReadOnlySpan<byte> data, string signatureBase64) => false;
}
