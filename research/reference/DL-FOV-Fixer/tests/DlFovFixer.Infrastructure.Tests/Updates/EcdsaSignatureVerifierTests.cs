using System.Security.Cryptography;
using System.Text;
using DlFovFixer.Infrastructure.Updates;

namespace DlFovFixer.Infrastructure.Tests.Updates;

/// <summary>
/// The fixture in Fixtures/ManifestSigning was signed by tools/openssl.ps1 with a throwaway key,
/// exactly as the release signs manifest.json, so these tests prove the two formats agree.
/// </summary>
public sealed class EcdsaSignatureVerifierTests
{
    private static readonly string Fixtures = Path.Combine(AppContext.BaseDirectory, "Fixtures", "ManifestSigning");

    private static byte[] Manifest => File.ReadAllBytes(Path.Combine(Fixtures, "manifest.json"));

    private static string Signature => File.ReadAllText(Path.Combine(Fixtures, "manifest.sig"));

    private static string PublicKey => File.ReadAllText(Path.Combine(Fixtures, "test-public.b64")).Trim();

    [Fact]
    public void Verify_ManifestSignedByOpenSsl_IsValid()
    {
        using var verifier = new EcdsaSignatureVerifier(PublicKey);

        Assert.True(verifier.Verify(Manifest, Signature));
    }

    [Fact]
    public void Verify_OneByteChanged_IsRefused()
    {
        var tampered = Manifest;
        var index = Encoding.ASCII.GetString(tampered).IndexOf("1234", StringComparison.Ordinal);
        tampered[index] = (byte)'9';
        using var verifier = new EcdsaSignatureVerifier(PublicKey);

        Assert.False(verifier.Verify(tampered, Signature));
    }

    [Fact]
    public void Verify_SignedWithAnotherKey_IsRefused()
    {
        using var other = ECDsa.Create(ECCurve.NamedCurves.nistP256);
        var otherSignature = Convert.ToBase64String(other.SignData(Manifest, HashAlgorithmName.SHA256, DSASignatureFormat.Rfc3279DerSequence));
        using var verifier = new EcdsaSignatureVerifier(PublicKey);

        Assert.False(verifier.Verify(Manifest, otherSignature));
    }

    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    [InlineData("not base64!")]
    [InlineData("AAAA")]
    public void Verify_MalformedSignature_IsRefusedWithoutThrowing(string signature)
    {
        using var verifier = new EcdsaSignatureVerifier(PublicKey);

        Assert.False(verifier.Verify(Manifest, signature));
    }

    [Fact]
    public void RepositoryKey_IsAP256PublicKey()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null && !File.Exists(Path.Combine(directory.FullName, "version.properties")))
        {
            directory = directory.Parent;
        }

        Assert.NotNull(directory);
        var key = File.ReadAllText(Path.Combine(directory.FullName, "contracts", "keys", "release-manifest-public.b64")).Trim();
        using var ecdsa = ECDsa.Create();
        ecdsa.ImportSubjectPublicKeyInfo(Convert.FromBase64String(key), out _);

        Assert.Equal(256, ecdsa.KeySize);
    }

    [Fact]
    public void NoSignatureVerifier_TrustsNothing() =>
        Assert.False(new NoSignatureVerifier().Verify(Manifest, Signature));
}
