# Omavault User Manual

Omavault is a safe for your files, right in the Omarchy bar. Files stored in it are encrypted on your disk (both contents and file names) and are only readable while the vault is unlocked. When you are done, it locks itself.

This manual is for everyday use. For the security model and development details, see the [README](README.md).

## Table of contents

- [Creating your vault](#creating-your-vault)
- [The recovery key](#the-recovery-key)
- [Unlocking and using the vault](#unlocking-and-using-the-vault)
- [Automatic locking](#automatic-locking)
- [If you forgot your passphrase](#if-you-forgot-your-passphrase)
- [Files dropped in while the vault was locked](#files-dropped-in-while-the-vault-was-locked)
- [Keyboard navigation](#keyboard-navigation)
- [Changing settings](#changing-settings)
- [Removing Omavault](#removing-omavault)

## Creating your vault

The first time you open the panel (click the shield icon in the bar), you will create the vault:

1. Choose a passphrase with at least 8 characters. A long phrase you can remember is better than a short complicated word.
2. Type it again to confirm and press **Create vault**.

The vault folder is created in `~/.local/share/omavault/`. Everything you put in it is encrypted automatically; there is no extra step.

## The recovery key

Right after creating the vault, the panel shows a **recovery key**: a long code like `6f2f38e6-93a3f5ac-…`.

- **Save it somewhere safe and offline** (password manager printed in your safe, a piece of paper). Use the **Copy** button and paste it — the key is written directly to `wl-copy` stdin, never appearing in argv or `/proc`.
- This is the only way to open the vault if you ever forget the passphrase.
- It is shown **only once**. Click the card after saving it.
- If you lose both the passphrase and the recovery key, the files are gone. There is no back door.

## Unlocking and using the vault

1. Click the shield icon, type your passphrase and press **Unlock** (or hit `Enter`).
2. The countdown at the top shows when the vault will lock itself.
3. **Open folder** opens the decrypted folder in your file manager. Drag files in, open them, edit them: encryption happens transparently.
4. The panel lists the most recent files; click one to reveal it in the file manager.
5. Lock it yourself anytime: the toggle in the panel header, **middle-click** on the bar icon, or the `l` key.

While the vault is unlocked, any app running as your user can read its files. That is normal for any encrypted folder on a running system; lock the vault when you step away.

## Automatic locking

The vault locks itself after the configured delay (default 10 minutes). The countdown turns red in the last minute; **Postpone lock** restarts the timer.

If apps still hold files open when the vault locks, it detaches immediately but finishes the lock when those apps close. The panel shows which apps are holding things up under "Open in apps".

## If you forgot your passphrase

Use the recovery key you saved when creating the vault:

1. Open the panel and click **Use recovery key** under the unlock form.
2. Paste the recovery key and unlock.

After unlocking with the recovery key, the panel shows **New passphrase**: set a new one so you can unlock normally again. The recovery key keeps working: it never expires or changes.

You can only do this while the vault stays unlocked; if it locks before you set a new passphrase, just unlock with the recovery key again.

## Files dropped in while the vault was locked

Sometimes a stale file manager window lets you drop files into the vault folder while it is locked. Those files are **not encrypted**. On the next unlock, Omavault moves them to a private `recovered` folder (permissions 0700, readable only by your user) and the panel offers two choices:

- **Move to vault**: re-inserts them into the vault, encrypted again
- **Delete**: removes them permanently

Check this section before locking again: recovered files sit unencrypted until you decide.

## Keyboard navigation

Everything works without a mouse:

- **Arrow keys** or **Tab / Shift+Tab** move through the panel elements: header, buttons, form fields (with `+` to reveal what you typed), file list
- **Enter** activates the highlighted element
- `r` refreshes the status, `l` locks
- **Esc** closes the panel
- At the first/last element, **Tab** moves on to the next plugin in the bar

## Changing settings

Right-click the bar icon to open the Omarchy plugin settings, or use the bar's widget settings. You can adjust:

- **Auto-lock delay** (1 to 480 minutes, default 10)
- **Recent files to list** (5 to 50, default 10)

## Removing Omavault

If you no longer want the vault:

1. Take your files out first: after removal they stay encrypted and unreadable.
2. Run the uninstall script from the project folder and follow the prompts:
   `./uninstall.sh --remove-packages --remove-data`
3. Or do it manually:
   - Remove the plugin: `omarchy plugin remove skvggor.omavault`
   - Optionally remove the encryption tool: `omarchy pkg remove gocryptfs`
   - Delete the leftover data: `rm -rf ~/.local/share/omavault`

Never remove `fuse3`; other parts of the system depend on it.
