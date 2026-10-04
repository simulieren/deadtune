using System.Security.Cryptography;
using DlFovFixer.Core.Updates;

namespace DlFovFixer.Infrastructure.Updates;

/// <summary>
/// ECDSA P-256 with SHA-256 over the exact manifest bytes. The signature is DER, base64-encoded, as
/// <c>openssl dgst -sha256 -sign</c> writes it (ADR 0006). Uses only the platform crypto.
/// </summary>
public sealed class EcdsaSignatureVerifier : ISignatureVerifier, IDisposable
{
    private readonly ECDsa _key;

    /// <param name="publicKeyBase64">The public key as base64 DER SubjectPublicKeyInfo, from contracts/keys.</param>
    public EcdsaSignatureVerifier(string publicKeyBase64)
    {
        _key = ECDsa.Create();
        _key.ImportSubjectPublicKeyInfo(Convert.FromBase64String(publicKeyBase64), out _);
    }

    public bool Verify(ReadOnlySpan<byte> data, string signatureBase64)
    {
        byte[] signature;
        try
        {
            signature = Convert.FromBase64String(signatureBase64.Trim());
        }
        catch (FormatException)
        {
            return false;
        }

        if (signature.Length == 0)
        {
            return false;
        }

        try
        {
            return _key.VerifyData(data, signature, HashAlgorithmName.SHA256, DSASignatureFormat.Rfc3279DerSequence);
        }
        catch (CryptographicException)
        {
            return false;
        }
    }

    public void Dispose() => _key.Dispose();
}
