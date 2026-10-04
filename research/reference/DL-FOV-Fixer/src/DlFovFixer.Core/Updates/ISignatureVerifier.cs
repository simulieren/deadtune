namespace DlFovFixer.Core.Updates;

/// <summary>Checks a detached signature over exact bytes against the key built into the app (ADR 0006).</summary>
public interface ISignatureVerifier
{
    bool Verify(ReadOnlySpan<byte> data, string signatureBase64);
}
