#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
IDENTITY="$("$SCRIPT_DIR/ensure-local-codesign-identity.sh")"
LOCAL_TARGET_DIR="$APP_ROOT/src-tauri/target/local-api-key-file"

echo "Building local signed app with identity: $IDENTITY"
echo "Local diagnostic target: $LOCAL_TARGET_DIR"
cd "$APP_ROOT"
CARGO_TARGET_DIR="$LOCAL_TARGET_DIR" \
  tauri build --bundles app --features local-api-key-file --config "{\"bundle\":{\"macOS\":{\"signingIdentity\":\"$IDENTITY\"}}}"
