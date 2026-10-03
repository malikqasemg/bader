#!/bin/sh
# Signs the built Bader.app with a stable local identity ("Bader Dev" by default),
# so macOS keeps "Always Allow" for the keychain across rebuilds.
#   scripts/sign-mac.sh [identity]
set -e
ID="${1:-Bader Dev}"
APP="$(dirname "$0")/../target/release/bundle/macos/Bader.app"
HASH=$(security find-identity -p codesigning | grep "\"$ID\"" | head -1 | awk '{print $2}')
[ -n "$HASH" ] || { echo "no signing identity called $ID"; exit 1; }
codesign --force --deep --sign "$HASH" "$APP"
codesign -dv "$APP" 2>&1 | grep -E "Authority|Identifier|Signature" | head -4
codesign -d -r- "$APP" 2>&1 | tail -1
