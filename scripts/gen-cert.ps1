# Create a self-signed code-signing certificate in the CurrentUser store and
# print its thumbprint. Tauri signs by thumbprint (signtool reads the cert from
# the store), so no PFX export and no password are needed.
$ErrorActionPreference = "Stop"

$cert = New-SelfSignedCertificate `
    -Type CodeSigningCert `
    -Subject "CN=Ark Nuwa Desktop (Self-Signed)" `
    -KeyAlgorithm RSA -KeyLength 2048 `
    -CertStoreLocation Cert:\CurrentUser\My `
    -NotAfter (Get-Date).AddYears(5) `
    -HashAlgorithm SHA256

Write-Output ("THUMBPRINT=" + $cert.Thumbprint)
