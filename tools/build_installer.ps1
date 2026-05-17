# Avatar Studio - build the Windows MSI installer.
#
# Pipeline:
#   1. cargo build --release  (both binaries)
#   2. heat.exe harvest assets/processed -> wix/assets.wxs
#   3. cargo wix (candle + light)        -> target/wix/avatar_studio-<v>-x86_64.msi
#   4. Set-AuthenticodeSignature with the dev cert.
#   5. Print SHA-256 of the signed MSI.
#
# Prereqs (see docs/packaging.md):
#   - cargo install cargo-wix --locked
#   - WiX Toolset 3.11 binaries unzipped to %USERPROFILE%\.avatar_studio\wix311
#   - tools/dev_codesign.ps1 has been run (or you have a real .pfx)

[CmdletBinding()]
param(
    [switch]$NoSign,
    [string]$WixBinDir = "$env:USERPROFILE\.avatar_studio\wix311",
    [string]$PfxPath   = "$env:USERPROFILE\.avatar_studio\dev-cert.pfx",
    [string]$PfxPass   = $env:AVATAR_STUDIO_PFX_PASS,
    [string]$TimestampUrl = "http://timestamp.digicert.com"
)

# Note: we intentionally do NOT set $ErrorActionPreference = "Stop".
# PowerShell 5 treats every line a native executable writes to stderr as
# a NativeCommandError, which would abort the build mid-stream. We rely on
# explicit $LASTEXITCODE checks after each native call instead.

if (-not (Test-Path "$WixBinDir\candle.exe")) {
    throw "WiX bin not found at $WixBinDir. See docs/packaging.md."
}
$env:WIX_PATH = $WixBinDir
$env:Path = "$WixBinDir;$env:Path"

$workspace = (Resolve-Path "$PSScriptRoot\..").Path
Push-Location $workspace
try {
    Write-Host "[1/4] cargo build --release"
    cargo build --release --bin avatar_desktop --bin asset_builder
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

    Write-Host "[2/4] heat: harvest assets/processed"
    $assetsWxs = Join-Path $workspace "wix\assets.wxs"
    & "$WixBinDir\heat.exe" `
        dir "assets\processed" `
        -cg HarvestedAssets `
        -gg -sfrag -srd -sreg -scom `
        -dr ProcessedDir `
        -var var.AssetsProcessedDir `
        -out $assetsWxs
    if ($LASTEXITCODE -ne 0) { throw "heat.exe failed" }

    Write-Host "[3/4] cargo wix (compile + link MSI)"
    $mainWxs = Join-Path $workspace "wix\main.wxs"
    # cargo-wix auto-detects WixUI_Minimal and links WixUIExtension itself;
    # don't pass `-ext WixUIExtension` again or light will report duplicate
    # symbol LGHT0091.
    # ICE38/ICE43/ICE57/ICE64/ICE91 are per-machine-install assumptions that
    # don't apply to our per-user MSI (LocalAppDataFolder is inherently in
    # the user profile by design).
    cargo wix `
        --package avatar_desktop `
        --no-build `
        --include $mainWxs `
        --include $assetsWxs `
        --compiler-arg "-dAssetsProcessedDir=$workspace\assets\processed" `
        --linker-arg "-sice:ICE38" `
        --linker-arg "-sice:ICE43" `
        --linker-arg "-sice:ICE57" `
        --linker-arg "-sice:ICE64" `
        --linker-arg "-sice:ICE91"
    if ($LASTEXITCODE -ne 0) { throw "cargo wix failed" }

    $msi = Get-ChildItem "target\wix\*.msi" |
           Sort-Object LastWriteTime -Descending |
           Select-Object -First 1
    if (-not $msi) { throw "no MSI produced" }
    Write-Host "Built: $($msi.FullName)"

    if (-not $NoSign) {
        if (-not $PfxPass) {
            Write-Warning "AVATAR_STUDIO_PFX_PASS not set; trying default."
            $PfxPass = "avatar-studio-dev"
        }
        if (-not (Test-Path $PfxPath)) {
            throw "No PFX at $PfxPath. Run tools/dev_codesign.ps1 first."
        }
        # Get-PfxCertificate -Password is PowerShell 7+; on PS 5.1 we build
        # the X509Certificate2 directly so the PFX can be loaded with a
        # password from CI without an interactive prompt.
        $cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2 `
            -ArgumentList @($PfxPath, $PfxPass,
                [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::PersistKeySet)
        Write-Host "[4/4] Signing with $($cert.Subject) ($($cert.Thumbprint))"
        $result = Set-AuthenticodeSignature `
            -FilePath $msi.FullName `
            -Certificate $cert `
            -TimestampServer $TimestampUrl `
            -HashAlgorithm SHA256
        Write-Host "Signature status: $($result.Status)"
        if ($result.Status -ne "Valid" -and $result.Status -ne "UnknownError") {
            Write-Warning "Signature status is not 'Valid'. Self-signed certs report 'UnknownError' until the cert is installed as Trusted Root - see docs/packaging.md."
        }
    } else {
        Write-Host "[4/4] Skipping signing (-NoSign)"
    }

    $hash = (Get-FileHash $msi.FullName -Algorithm SHA256).Hash
    Write-Host ""
    Write-Host "MSI    : $($msi.FullName)"
    Write-Host "Size   : $([Math]::Round($msi.Length / 1MB, 2)) MB"
    Write-Host "SHA-256: $hash"
} finally {
    Pop-Location
}
