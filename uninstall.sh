#!/usr/bin/env bash
set -euo pipefail

PLUGIN_ID="skvggor.omavault"
PLUGIN_DIR="$HOME/.config/omarchy/plugins/$PLUGIN_ID"
DATA_DIR="${OMAVAULT_ROOT:-$HOME/.local/share/omavault}"
MOUNT_DIR="$DATA_DIR/Protected Files"

REMOVE_PACKAGES=false
REMOVE_DATA=false

usage() {
  cat <<'EOF'
usage: uninstall.sh [-y] [--remove-packages] [--remove-data]

Removes the omavault plugin from Omarchy.

Options:
  -y                 Skip the confirmation prompt
  --remove-packages  Also uninstall gocryptfs (omarchy-pkg-remove).
                     fuse3 is never removed: the system needs it.
  --remove-data      Also delete the vault data in ~/.local/share/omavault.
                     Unrecoverable: take your files out of the vault first.
  -h, --help         Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -y) ASSUME_YES=true ;;
    --remove-packages) REMOVE_PACKAGES=true ;;
    --remove-data) REMOVE_DATA=true ;;
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

if [[ "${ASSUME_YES:-false}" != true ]]; then
  read -r -p "Remove the omavault plugin? [y/N] " answer
  [[ "$answer" == "y" || "$answer" == "Y" ]] || {
    echo "Aborted."
    exit 1
  }
fi

if [[ "$REMOVE_DATA" == true && -d "$DATA_DIR" ]]; then
  if [[ "${ASSUME_YES:-false}" != true ]]; then
    read -r -p "Also DELETE all vault data in $DATA_DIR? This cannot be undone. [y/N] " answer
    [[ "$answer" == "y" || "$answer" == "Y" ]] || {
      echo "Keeping vault data."
      REMOVE_DATA=false
    }
  fi
fi

if [[ "$REMOVE_DATA" == true ]] && findmnt "$MOUNT_DIR" >/dev/null 2>&1; then
  echo "Unmounting $MOUNT_DIR…"
  fusermount3 -u "$MOUNT_DIR" || fusermount -u "$MOUNT_DIR" || umount "$MOUNT_DIR"
fi

if [[ "$REMOVE_DATA" == true && -d "$DATA_DIR" ]]; then
  rm -rf "$DATA_DIR"
  echo "Deleted $DATA_DIR."
fi

if command -v omarchy >/dev/null 2>&1; then
  omarchy plugin remove "$PLUGIN_ID" --yes || true
elif [[ -d "$PLUGIN_DIR" ]]; then
  rm -rf "$PLUGIN_DIR"
  omarchy-shell shell rescanPlugins || true
fi
echo "Removed the omavault plugin."

if [[ "$REMOVE_PACKAGES" == true ]]; then
  echo "Removing gocryptfs package…"
  omarchy-pkg-remove gocryptfs
fi

if [[ "$REMOVE_DATA" != true && -d "$DATA_DIR" ]]; then
  echo "Note: vault data kept in $DATA_DIR (encrypted, unreadable without gocryptfs)."
fi
