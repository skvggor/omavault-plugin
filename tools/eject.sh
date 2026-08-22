#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

DATA_DIR="${OMAVAULT_ROOT:-$HOME/.local/share/omavault}"
MOUNT_DIR="$DATA_DIR/Protected Files"
ASSUME_YES=false
BACKUP_DIR=""
REMOVE_PACKAGES=false

usage() {
  cat <<'EOF'
usage: tools/eject.sh [-y] [--backup DIR] [--remove-packages]

Ejects the omavault plugin back to a factory state: unmounts the vault,
wipes every trace of vault data, rebuilds the helper and reinstalls the
plugin so the full flow can be tested from scratch.

Deleted data is unrecoverable (encrypted vault, no other recovery path).

Options:
  -y                 Skip the confirmation prompt
  --backup DIR       Copy the vault data to DIR before wiping
  --remove-packages  Also uninstall gocryptfs (omarchy-pkg-remove).
                     The vault data stays encrypted and unreadable.
                     fuse3 is never removed: the system needs it.
  -h, --help         Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -y) ASSUME_YES=true ;;
    --remove-packages) REMOVE_PACKAGES=true ;;
    --backup)
      shift
      BACKUP_DIR="${1:?--backup requires a directory}"
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

FILE_COUNT=0
if [[ -d "$DATA_DIR" ]]; then
  FILE_COUNT=$(find "$DATA_DIR" -type f | wc -l)
fi

echo "Vault data : $DATA_DIR ($FILE_COUNT files)"
[[ -n "$BACKUP_DIR" ]] && echo "Backup to  : $BACKUP_DIR"

if [[ "$ASSUME_YES" != true ]]; then
  read -r -p "This wipes the vault permanently. Continue? [y/N] " answer
  [[ "$answer" == "y" || "$answer" == "Y" ]] || {
    echo "Aborted."
    exit 1
  }
fi

if findmnt "$MOUNT_DIR" >/dev/null 2>&1; then
  echo "Unmounting $MOUNT_DIR…"
  fusermount3 -u "$MOUNT_DIR" || fusermount -u "$MOUNT_DIR" || umount "$MOUNT_DIR"
fi

if [[ -n "$BACKUP_DIR" && -d "$DATA_DIR" ]]; then
  mkdir -p "$BACKUP_DIR"
  cp -a "$DATA_DIR/." "$BACKUP_DIR/"
  echo "Backup saved."
fi

if [[ -d "$DATA_DIR" ]]; then
  rm -rf "$DATA_DIR"
  echo "Wiped $DATA_DIR."
else
  echo "No vault data to wipe."
fi

./install.sh
omarchy-shell shell rescanPlugins || true

if [[ "$REMOVE_PACKAGES" == true ]]; then
  echo "Removing gocryptfs package…"
  omarchy-pkg-remove gocryptfs
fi

"$HOME/.config/omarchy/plugins/skvggor.omavault/omavault-helper" status
