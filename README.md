# Omavault (Omarchy plugin)

<img width="474" height="495" alt="image" src="https://github.com/user-attachments/assets/7db4c2ec-2dac-4bd8-9c8c-559559c827ad" />


A secret vault for your files in the Omarchy bar, backed by [gocryptfs](https://github.com/rfjakob/gocryptfs). Files are encrypted at rest (content **and** file names); a decrypted mount exists only while the vault is unlocked, and auto-locks after a configurable delay.

- Bar widget with vault state, unlock form, recent files, and an auto-lock countdown
- Vault data lives in `~/.local/share/omavault/` (`vault` = encrypted, `mount` = decrypted view)
- Passphrase travels over stdin, never in argv or on disk
- A recovery key (gocryptfs master key) is shown once at creation; there is no other recovery path

## Dependencies

- `gocryptfs` and `fuse3`: install manually (`omarchy pkg add gocryptfs fuse3`) or use the "Install gocryptfs" button in the panel (runs `omarchy-pkg-add` in a terminal, sudo prompted there, same mechanism as Omarchy's first-party service installers)
- `util-linux` (the `script` tool, present on any normal Arch install), required at vault creation so gocryptfs prints the master key

No Rust toolchain needed for a normal install: `./install.sh` builds the helper if `cargo` is available, otherwise downloads the prebuilt binary matching this version from GitHub Releases and verifies its SHA-256 checksum. To build from source instead, run `omarchy pkg add rust`.

## Install

```sh
./install.sh
omarchy plugin enable skvggor.omavault
```

Or the native Omarchy flow — the panel then offers an "Install helper" button on first use, which downloads the prebuilt binary matching this version:

```sh
omarchy plugin add https://github.com/skvggor/omavault-plugin.git --enable
```

## Update

The plugin files and the helper binary update separately — the binary is downloaded from the releases, not tracked in git:

```sh
omarchy plugin update skvggor.omavault
bash ~/.config/omarchy/plugins/skvggor.omavault/setup-helper.sh
```

If you skip the second command, the panel detects the stale helper version and shows the "Install helper" button again; clicking it downloads the matching binary (v0.1.2 or newer).

## Usage

- Click the shield icon to open the panel
- First run: create the vault with a passphrase (min. 8 characters) and **save the recovery key** (the "Copy" button puts it in your clipboard; paste it somewhere offline)
- Unlock with the passphrase (or switch to "Use recovery key" in the panel if you forgot it); the decrypted folder opens via "Open vault folder"
- After unlocking with the recovery key, the panel offers to set a new passphrase (re-wraps the gocryptfs master key; the recovery key stays valid), otherwise you would need the recovery key forever
- The vault locks automatically after the configured delay, or immediately via the toggle / middle-click on the bar icon
- Files dropped into the mount point while locked are kept in `recovered`; after unlocking, the panel offers to move them back into the vault (encrypted) or delete them

## Security model

Be honest about what this protects:

- **Protected**: data at rest (stolen disk, backups, cloud sync of `$HOME`), and any access while the vault is locked
- **Not protected**: anything running as your user while the vault is unlocked, or a compromised session
- **Caveats**:
  - Files dropped into the mount point while the vault is locked are moved, **unencrypted**, to `~/.local/share/omavault/recovered/` on the next unlock. The panel then offers to move them back into the vault or delete them; until you act, they sit in plaintext
  - The passphrase and recovery key live in the panel's memory (QML strings cannot be securely zeroized) while the panel process runs
  - Unlocking with the recovery key passes it to gocryptfs as a command-line argument (its only supported interface), so the key is briefly visible in `/proc/<pid>/cmdline` to processes running as your user, the same exposure class the threat model already excludes

Forgotten passphrase + lost recovery key = unrecoverable data, by design.

## Uninstall

The Omarchy plugin manifest has no dependency hook, so packages are managed separately:

```sh
./uninstall.sh                                    # removes the plugin
./uninstall.sh --remove-packages --remove-data    # full teardown
```

Or manually:

```sh
omarchy plugin remove skvggor.omavault          # removes the plugin
omarchy pkg remove gocryptfs                    # optional: also drop the dependency
```

Do **not** remove `fuse3`: the rest of the system (gvfs, qemu, xdg-desktop-portal, …) needs it. The vault data in `~/.local/share/omavault/` stays behind, encrypted and unreadable without gocryptfs; delete the folder if you no longer want it.

## Development

```sh
cargo test          # helper unit tests
cargo llvm-cov      # coverage
npm test            # Model.js tests
qmllint -I /usr/share/omarchy/shell *.qml
tools/eject.sh      # factory-reset: unmounts, wipes vault data, reinstalls
                    # --remove-packages also uninstalls gocryptfs
```

End-user documentation lives in [MANUAL.md](MANUAL.md).

The helper binary (`omavault-helper`) is a thin Rust wrapper around gocryptfs; the QML layer only orchestrates it and never handles cryptography.

## Release

Bump `version` in `manifest.json` and `Cargo.toml` to the same value, then tag:

```sh
git tag v0.1.1 && git push origin v0.1.1
```

CI refuses the tag if versions disagree, builds a static musl helper, runs the tests, and publishes binary + SHA-256 as release assets.
