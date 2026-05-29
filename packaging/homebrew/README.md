# Homebrew cask

Source of truth for the QoreDB Homebrew cask. The published version lives in [`homebrew/homebrew-cask`](https://github.com/Homebrew/homebrew-cask) under `Casks/q/qoredb.rb`.

## Releasing a new version

1. Build and publish the macOS DMGs on GitHub Releases (the `pnpm tauri build` pipeline handles this).
2. Compute the SHA-256 of each DMG:
   ```bash
   shasum -a 256 QoreDB_0.1.29_x64.dmg
   shasum -a 256 QoreDB_0.1.29_aarch64.dmg
   ```
3. Update `version` and both `sha256` lines in `qoredb.rb`.
4. Fork `homebrew/homebrew-cask`, copy `qoredb.rb` to `Casks/q/qoredb.rb`, and open a PR. Title format: `Update qoredb from 0.1.X to 0.1.Y`.
5. The Homebrew CI runs `brew audit --new qoredb`, `brew style qoredb`, and verifies the URLs + checksums. Fix any audit feedback in the same PR.

## Local testing before submitting

```bash
brew tap --force-auto-update homebrew/cask
brew install --cask --no-quarantine ./qoredb.rb
brew audit --strict --online qoredb
brew style qoredb
```
