#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <Paper Float Translator.app> [Paper Float Translator.dmg]" >&2
  exit 64
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
# shellcheck source=mac-release-signature-policy.sh
source "$SCRIPT_DIR/mac-release-signature-policy.sh"

EXPECTED_APP_IDENTIFIER="com.paperfloat.translator"

resolve_path() {
  local candidate="$1"

  if [[ -e "$candidate" ]]; then
    printf '%s\n' "$candidate"
    return
  fi

  if [[ -e "$REPO_ROOT/$candidate" ]]; then
    printf '%s\n' "$REPO_ROOT/$candidate"
    return
  fi

  printf '%s\n' "$candidate"
}

APP_PATH="$(resolve_path "$1")"
DMG_PATH="${2:-}"
if [[ -n "$DMG_PATH" ]]; then
  DMG_PATH="$(resolve_path "$DMG_PATH")"
fi

if [[ ! -d "$APP_PATH" ]]; then
  echo "app bundle not found: $APP_PATH" >&2
  exit 66
fi

APP_EXECUTABLE="$APP_PATH/Contents/MacOS/paper-float-translator"
LOCAL_API_KEY_ARTIFACT_MARKER="PAPER_FLOAT_ARTIFACT_CLASS_LOCAL_API_KEY_FILE_V1"
LEGACY_LOCAL_API_KEY_ARTIFACT_MARKER="api-key.local"
if [[ ! -f "$APP_EXECUTABLE" ]]; then
  echo "release verification rejected app: expected executable is missing" >&2
  exit 66
fi
if [[ "${ALLOW_ADHOC_MAC_VERIFY:-}" != "1" ]]; then
  set +e
  LC_ALL=C /usr/bin/grep -a -F -q \
    -e "$LOCAL_API_KEY_ARTIFACT_MARKER" \
    -e "$LEGACY_LOCAL_API_KEY_ARTIFACT_MARKER" \
    "$APP_EXECUTABLE"
  LOCAL_API_KEY_MARKER_STATUS=$?
  set -e
  if [[ $LOCAL_API_KEY_MARKER_STATUS -eq 0 ]]; then
    echo "release verification rejected app: local-api-key-file artifact marker is present" >&2
    exit 65
  fi
  if [[ $LOCAL_API_KEY_MARKER_STATUS -ne 1 ]]; then
    echo "release verification rejected app: unable to inspect executable for local-api-key-file marker" >&2
    exit 65
  fi

  set +e
  LC_ALL=C /usr/bin/grep -a -q "paper_float_test_" "$APP_EXECUTABLE"
  ACCEPTANCE_SYMBOL_STATUS=$?
  set -e
  if [[ $ACCEPTANCE_SYMBOL_STATUS -eq 0 ]]; then
    echo "release verification rejected app: acceptance-testing native symbols are present" >&2
    exit 65
  fi
  if [[ $ACCEPTANCE_SYMBOL_STATUS -ne 1 ]]; then
    echo "release verification rejected app: unable to inspect executable for acceptance-testing symbols" >&2
    exit 65
  fi
fi

verify_developer_id_signature() {
  local artifact_path="$1"
  local artifact_label="$2"
  local signature_metadata="$3"
  local required_identifier="${4:-}"
  local signature_kind team_identifier signed_identifier requirement_output strict_requirement

  signature_kind="$(paper_float_classify_signature "$signature_metadata")"
  printf 'signature policy (%s): %s\n' "$artifact_label" "$signature_kind"
  if [[ "$signature_kind" != "developer_id_application" ]]; then
    echo "release verification rejected $artifact_label: expected Authority=Developer ID Application, got $signature_kind" >&2
    return 65
  fi

  team_identifier="$(paper_float_extract_team_identifier "$signature_metadata")"
  if ! [[ "$team_identifier" =~ ^[A-Z0-9]{10}$ ]]; then
    echo "release verification rejected $artifact_label: a stable TeamIdentifier is required" >&2
    return 65
  fi

  signed_identifier="$(paper_float_extract_identifier "$signature_metadata")"
  if ! [[ "$signed_identifier" =~ ^[A-Za-z0-9.-]+$ ]]; then
    echo "release verification rejected $artifact_label: a safe signed Identifier is required" >&2
    return 65
  fi
  if [[ -n "$required_identifier" && "$signed_identifier" != "$required_identifier" ]]; then
    echo "release verification rejected $artifact_label: expected Identifier=$required_identifier, got $signed_identifier" >&2
    return 65
  fi

  if ! requirement_output="$(codesign -d -r- "$artifact_path" 2>&1)"; then
    echo "release verification rejected $artifact_label: unable to read its designated requirement" >&2
    printf '%s\n' "$requirement_output" >&2
    return 65
  fi
  printf '%s\n' "$requirement_output"

  if ! paper_float_is_developer_id_designated_requirement "$requirement_output" "$team_identifier" "$signed_identifier"; then
    echo "release verification rejected $artifact_label: designated requirement is not strictly bound to its identifier, Developer ID certificate chain, and team" >&2
    return 65
  fi

  if ! strict_requirement="$(paper_float_build_developer_id_requirement "$signed_identifier" "$team_identifier")"; then
    echo "release verification rejected $artifact_label: unable to construct the independent release requirement" >&2
    return 65
  fi
  if ! codesign --verify --deep --strict --verbose=2 -R="$strict_requirement" "$artifact_path"; then
    echo "release verification rejected $artifact_label: artifact does not satisfy the independently constructed Developer ID requirement" >&2
    return 65
  fi
}

echo "== codesign app =="
if [[ "${ALLOW_ADHOC_MAC_VERIFY:-}" == "1" ]]; then
  echo "LOCAL DIAGNOSTIC ONLY: Developer ID release policy is not being evaluated."
  codesign --verify --deep --verbose=2 "$APP_PATH" || echo "non-strict app verification failed for local ad-hoc diagnostics"
else
  codesign --verify --deep --strict --verbose=2 "$APP_PATH"
fi
APP_SIGNATURE="$(codesign -dvvv --entitlements :- "$APP_PATH" 2>&1)"
printf '%s\n' "$APP_SIGNATURE"

if [[ "${ALLOW_ADHOC_MAC_VERIFY:-}" != "1" ]]; then
  verify_developer_id_signature "$APP_PATH" "app" "$APP_SIGNATURE" "$EXPECTED_APP_IDENTIFIER"
fi

echo "== gatekeeper app =="
if [[ "${ALLOW_ADHOC_MAC_VERIFY:-}" == "1" ]]; then
  echo "skipped for local ad-hoc diagnostics"
else
  spctl --assess --type execute --verbose "$APP_PATH"
fi

if [[ -n "$DMG_PATH" ]]; then
  if [[ ! -f "$DMG_PATH" ]]; then
    echo "dmg not found: $DMG_PATH" >&2
    exit 66
  fi

  echo "== codesign dmg =="
  if [[ "${ALLOW_ADHOC_MAC_VERIFY:-}" == "1" ]]; then
    codesign --verify --verbose=2 "$DMG_PATH" || echo "non-strict dmg verification failed for local ad-hoc diagnostics"
  else
    codesign --verify --strict --verbose=2 "$DMG_PATH"
    DMG_SIGNATURE="$(codesign -dvvv "$DMG_PATH" 2>&1)"
    printf '%s\n' "$DMG_SIGNATURE"
    verify_developer_id_signature "$DMG_PATH" "dmg" "$DMG_SIGNATURE"
  fi

  echo "== gatekeeper dmg =="
  if [[ "${ALLOW_ADHOC_MAC_VERIFY:-}" == "1" ]]; then
    echo "skipped for local ad-hoc diagnostics"
  else
    spctl --assess --type open --verbose "$DMG_PATH"
  fi

  echo "== stapler dmg =="
  if [[ "${ALLOW_ADHOC_MAC_VERIFY:-}" == "1" ]]; then
    echo "skipped for local ad-hoc diagnostics"
  else
    xcrun stapler validate "$DMG_PATH"
  fi
fi
