# WinGet manifests

Source of truth for the QoreDB WinGet package. The Microsoft WinGet community repository expects all three files (`installer`, `locale.en-US`, `version`) under `manifests/q/QoreDB/QoreDB/<version>/`.

## Releasing a new version

1. Publish the Windows `.exe` (NSIS-based Tauri installer) on GitHub Releases.
2. Compute its SHA-256 in PowerShell:
   ```powershell
   Get-FileHash -Algorithm SHA256 QoreDB_<version>_x64-setup.exe
   ```
3. Update the three manifest YAMLs with the new `PackageVersion`, `InstallerUrl`, `InstallerSha256`, and `ReleaseDate`.
4. Validate locally:
   ```powershell
   winget validate --manifest .\packaging\winget
   ```
5. Fork [`microsoft/winget-pkgs`](https://github.com/microsoft/winget-pkgs), copy the three YAMLs into `manifests/q/QoreDB/QoreDB/<version>/`, and open a PR. Title format: `QoreDB.QoreDB version 0.1.Y`.
6. The Microsoft CI pipeline (Smoke Test) runs an automated install against a clean Windows VM. Fix any feedback in the same PR.

## Notes

- Scope is `user` (per-user install). Tauri NSIS installers default to user scope and don't require admin rights.
- The `InstallerType` is `nullsoft` since the Tauri Windows build uses NSIS.
- After the first manifest is merged, future updates can be automated with [wingetcreate](https://github.com/microsoft/winget-create) running in the release workflow.
