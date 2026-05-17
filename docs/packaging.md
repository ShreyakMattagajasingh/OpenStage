# Packaging — Windows MSI installer (Phase 15)

## What ships

`tools/build_installer.ps1` produces a single per-user MSI:

```
target/wix/avatar_desktop-0.1.0-x86_64.msi   (~10 MB)
```

Contents:

| File | Install location |
| ---- | ---------------- |
| `avatar_desktop.exe`  | `%LOCALAPPDATA%\Programs\AvatarStudio\` |
| `asset_builder.exe`   | `%LOCALAPPDATA%\Programs\AvatarStudio\` |
| `assets/processed/**` | `%LOCALAPPDATA%\Programs\AvatarStudio\assets\processed\` |
| `assets/icon/avatar_studio.ico` | `%LOCALAPPDATA%\Programs\AvatarStudio\assets\icon\` |
| `LICENSE.rtf`         | `%LOCALAPPDATA%\Programs\AvatarStudio\` |
| Start Menu shortcut   | `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Avatar Studio\Avatar Studio.lnk` |

User-writable runtime data is **never** in the install dir. It lives at:

```
%LOCALAPPDATA%\AvatarStudio\
├── settings.json
├── asset_catalog.sqlite
├── characters\
├── exports\
├── debug_screenshots\
├── perf\
└── (per-character thumbnails)
```

Uninstall removes the install dir but leaves `%LOCALAPPDATA%\AvatarStudio\`
untouched, preserving saves.

## Runtime path resolution

The binary auto-detects which layout it runs in (see
`crates/engine_core/src/paths.rs`):

| Mode        | When                                                              | Assets root          | User data root |
| ----------- | ----------------------------------------------------------------- | -------------------- | -------------- |
| Workspace   | exe is under `target/debug` or `target/release` (i.e. `cargo run`) | `./assets`           | `./user_data` |
| Portable    | `AVATAR_STUDIO_PORTABLE=1` or `avatar_studio.portable` marker next to exe | `<exe_dir>/assets` | `<exe_dir>/user_data` |
| Installed   | anything else                                                     | `<exe_dir>/assets`   | `%LOCALAPPDATA%\AvatarStudio` |

Env overrides win over everything:
- `AVATAR_STUDIO_ASSETS=<path>`
- `AVATAR_STUDIO_USER_DATA=<path>`

## Prerequisites (one-time setup)

1. Rust toolchain (already installed for dev work).
2. `cargo-wix`:
   ```
   cargo install cargo-wix --locked
   ```
3. WiX Toolset 3.11 binaries (portable, no admin needed):
   ```powershell
   $url = "https://github.com/wixtoolset/wix3/releases/download/wix3112rtm/wix311-binaries.zip"
   Invoke-WebRequest -Uri $url -OutFile "$env:USERPROFILE\.avatar_studio\wix311.zip"
   Expand-Archive -Path "$env:USERPROFILE\.avatar_studio\wix311.zip" `
                  -DestinationPath "$env:USERPROFILE\.avatar_studio\wix311"
   ```
4. The dev signing cert (one-time):
   ```powershell
   $env:AVATAR_STUDIO_PFX_PASS = "<choose a password>"
   pwsh tools\dev_codesign.ps1
   ```
   Writes `%USERPROFILE%\.avatar_studio\dev-cert.pfx` with a self-signed
   code-signing cert valid for 3 years. The cert subject is
   `CN=Avatar Studio Dev`.

## Building the MSI

```powershell
$env:AVATAR_STUDIO_PFX_PASS = "<the password from above>"
pwsh tools\build_installer.ps1
```

Steps:
1. `cargo build --release --bin avatar_desktop --bin asset_builder`.
2. `heat.exe` harvests `assets/processed/` into `wix/assets.wxs`.
3. `cargo wix` (candle + light) emits `target/wix/avatar_desktop-0.1.0-x86_64.msi`.
4. `Set-AuthenticodeSignature` signs the MSI with the dev cert and the
   public DigiCert RFC-3161 timestamp server, so the signature stays
   valid after the cert expires.
5. SHA-256 of the signed MSI is printed.

Pass `-NoSign` to skip step 4.

### Linker validation suppressions

`build_installer.ps1` passes `-sice:ICE38 -sice:ICE43 -sice:ICE57 -sice:ICE64
-sice:ICE91`. These ICE validators are designed for per-machine
installs (HKLM keys, machine-wide directories). Our per-user MSI puts
everything under `LocalAppDataFolder` by design, which trips those
validators. The MSI installs cleanly on real Windows.

## SmartScreen and the dev cert

A self-signed cert is **not** in the Windows Trusted Root store, so:

- The first time the MSI runs, SmartScreen blocks it with
  "Windows protected your PC". Click **More info → Run anyway**.
- `Get-AuthenticodeSignature` reports `Status: UnknownError` until
  the issuer is trusted.

To silence SmartScreen on a development machine (do **not** do this on
production user boxes), install the cert into your Trusted Root store
once:

```powershell
$pwd = ConvertTo-SecureString -String $env:AVATAR_STUDIO_PFX_PASS `
                              -Force -AsPlainText
Import-PfxCertificate -FilePath "$env:USERPROFILE\.avatar_studio\dev-cert.pfx" `
                      -Password $pwd `
                      -CertStoreLocation Cert:\CurrentUser\Root
```

After that, `Get-AuthenticodeSignature` reports `Valid` and SmartScreen
stops complaining for binaries signed by that cert.

## Swapping in a real OV / EV cert

In production, replace the dev PFX with one from a real CA (e.g. SSL.com,
DigiCert, Sectigo) and re-run the build:

```powershell
$env:AVATAR_STUDIO_PFX_PASS = "<real cert password>"
pwsh tools\build_installer.ps1 `
    -PfxPath "C:\path\to\real-cert.pfx"
```

For Azure Trusted Signing (cloud-based, Microsoft's modern code-signing
service: $10/month, US/Canada orgs only as of late 2025), swap
`Set-AuthenticodeSignature` for `AzureSignTool`:

```
dotnet tool install -g AzureSignTool
AzureSignTool sign `
    -kvu https://<vault>.vault.azure.net `
    -kvc <cert-name> `
    -kvm `
    -tr http://timestamp.digicert.com `
    -td sha256 `
    target\wix\avatar_desktop-0.1.0-x86_64.msi
```

## Smoke-testing the MSI on a clean account

1. Copy the MSI to a Windows user account that doesn't have the dev
   cert in Trusted Root.
2. Double-click → SmartScreen warns → **Run anyway**.
3. The WiX Minimal UI shows, click through, install completes (no UAC).
4. Verify:
   - Start Menu has "Avatar Studio".
   - Launching the shortcut opens the app.
   - Default body loads; asset list populates from the bundled
     `assets/processed/` directory.
   - Save a character; the JSON appears under
     `%LOCALAPPDATA%\AvatarStudio\characters\`.
5. Uninstall via Programs & Features:
   - Install dir is removed.
   - `%LOCALAPPDATA%\AvatarStudio\` is preserved.

## Common pitfalls

- **`cargo wix` says "no Product or Bundle"** — `wix/main.wxs` not on the
  include list. The script always passes `--include wix\main.wxs`.
- **light.exe LGHT0091 duplicate symbol** — cargo-wix already links
  `WixUIExtension` when it sees `<UIRef Id="WixUI_Minimal" />`. Don't
  pass `-ext WixUIExtension` again.
- **`Get-PfxCertificate -Password` fails** — that parameter is PS 7+.
  We build the X509Certificate2 directly via `New-Object` in
  `build_installer.ps1` so PS 5.1 works.

## Out of scope

- Multi-resolution ICO (single 256×256 source for now).
- MSIX / Windows Store submission.
- Per-machine install path (would need admin + UAC).
- macOS `.dmg` / Linux `.deb` / AppImage.
- Auto-update.
