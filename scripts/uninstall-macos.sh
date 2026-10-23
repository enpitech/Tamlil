#!/bin/sh
# Remove Tamlil and everything it stored on this Mac.
#   sh scripts/uninstall-macos.sh            # remove app + all data
#   sh scripts/uninstall-macos.sh --keep-models
# Same as Settings → Preferences → Uninstall, for when the app is already gone.
set -u
ID="com.enpitech.tamlil"
KEEP_MODELS=0; [ "${1:-}" = "--keep-models" ] && KEEP_MODELS=1
echo "Quitting Tamlil..."; pkill -x Tamlil 2>/dev/null; sleep 1
for v in /Volumes/Tamlil*; do [ -d "$v" ] && hdiutil detach "$v" -quiet 2>/dev/null; done
DATA="$HOME/Library/Application Support/$ID"
if [ "$KEEP_MODELS" = 1 ] && [ -d "$DATA/models" ]; then
  echo "Keeping $DATA/models"
  find "$DATA" -mindepth 1 -maxdepth 1 ! -name models -exec rm -rf {} +
else
  rm -rf "$DATA"
fi
rm -rf "$HOME/Library/WebKit/$ID" "$HOME/Library/Caches/$ID" "$HOME/Library/HTTPStorages/$ID" \
       "$HOME/Library/Saved Application State/$ID.savedState" "$HOME/Library/Logs/Tamlil"
rm -f "$HOME/Library/Preferences/$ID.plist"; defaults delete "$ID" >/dev/null 2>&1
for app in "/Applications/Tamlil.app" "$HOME/Applications/Tamlil.app"; do
  [ -d "$app" ] && { echo "Removing $app"; rm -rf "$app"; }
done
echo "Done. Tamlil and its data are gone."
