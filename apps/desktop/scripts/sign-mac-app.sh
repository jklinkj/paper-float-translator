#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEFAULT_APP="$APP_ROOT/src-tauri/target/release/bundle/macos/Paper Float Translator.app"
APP_PATH="${1:-$DEFAULT_APP}"
SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:-${CODESIGN_IDENTITY:--}}"
ENTITLEMENTS="$APP_ROOT/src-tauri/entitlements.plist"
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/paper-float-translator"
LOCAL_API_KEY_ARTIFACT_MARKER="PAPER_FLOAT_ARTIFACT_CLASS_LOCAL_API_KEY_FILE_V1"
LEGACY_LOCAL_API_KEY_ARTIFACT_MARKER="api-key.local"

if [[ ! -d "$APP_PATH" ]]; then
  echo "app bundle not found: $APP_PATH" >&2
  exit 66
fi

if [[ ! -f "$ENTITLEMENTS" ]]; then
  echo "entitlements file not found: $ENTITLEMENTS" >&2
  exit 66
fi

if [[ ! -f "$APP_EXECUTABLE" ]]; then
  echo "app executable not found: $APP_EXECUTABLE" >&2
  exit 66
fi

if [[ "$SIGNING_IDENTITY" != "-" ]]; then
  set +e
  LC_ALL=C /usr/bin/grep -a -F -q \
    -e "$LOCAL_API_KEY_ARTIFACT_MARKER" \
    -e "$LEGACY_LOCAL_API_KEY_ARTIFACT_MARKER" \
    "$APP_EXECUTABLE"
  LOCAL_API_KEY_MARKER_STATUS=$?
  set -e
  if [[ $LOCAL_API_KEY_MARKER_STATUS -eq 0 ]]; then
    echo "signing rejected app: local-api-key-file diagnostics cannot receive a release identity" >&2
    exit 65
  fi
  if [[ $LOCAL_API_KEY_MARKER_STATUS -ne 1 ]]; then
    echo "signing rejected app: unable to inspect local-api-key-file artifact marker" >&2
    exit 65
  fi
fi

echo "Signing app bundle: $APP_PATH"
if [[ "$SIGNING_IDENTITY" == "-" ]]; then
  echo "Using local ad-hoc identity. This is for diagnostics only, not public release."
else
  echo "Using signing identity: $SIGNING_IDENTITY"
fi

codesign \
  --force \
  --deep \
  --options runtime \
  --entitlements "$ENTITLEMENTS" \
  --sign "$SIGNING_IDENTITY" \
  "$APP_PATH"

codesign --verify --deep --verbose=2 "$APP_PATH"
codesign -dvvv --entitlements :- "$APP_PATH" 2>&1
