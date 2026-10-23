#!/bin/sh
# Remove Tamlil's data on Linux (and the AppImage if you pass its path).
#   sh scripts/uninstall-linux.sh [--keep-models] [/path/to/Tamlil.AppImage]
# A .deb install is removed with: sudo apt remove tamlil
set -u
ID="com.enpitech.tamlil"; KEEP_MODELS=0; APPIMAGE=""
for a in "$@"; do case "$a" in --keep-models) KEEP_MODELS=1;; *) APPIMAGE="$a";; esac; done
pkill -x tamlil 2>/dev/null; pkill -x Tamlil 2>/dev/null; sleep 1
DATA="$HOME/.local/share/$ID"
if [ "$KEEP_MODELS" = 1 ] && [ -d "$DATA/models" ]; then
  find "$DATA" -mindepth 1 -maxdepth 1 ! -name models -exec rm -rf {} +
else
  rm -rf "$DATA"
fi
rm -rf "$HOME/.config/$ID" "$HOME/.cache/$ID"
[ -n "$APPIMAGE" ] && [ -f "$APPIMAGE" ] && rm -f "$APPIMAGE" && echo "Removed $APPIMAGE"
echo "Done."
