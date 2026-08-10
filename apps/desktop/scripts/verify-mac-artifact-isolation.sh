#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <default.app> <acceptance.app>" >&2
  exit 2
fi

default_app="$1"
acceptance_app="$2"
default_identifier="com.paperfloat.translator"
acceptance_identifier="com.paperfloat.translator.acceptance"
default_name="Paper Float Translator"
acceptance_name="Paper Float Translator Acceptance"
expected_executable="paper-float-translator"

if [[ "$default_app" == "$acceptance_app" ]]; then
  echo "artifact isolation rejected identical default and acceptance paths" >&2
  exit 1
fi

scratch_dir="$(/usr/bin/mktemp -d /private/tmp/paper-float-artifact-isolation.XXXXXX)"
cleanup() {
  /bin/rm -rf "$scratch_dir"
}
trap cleanup EXIT

capture_stdout_or_reject() {
  local output_path="$1"
  local description="$2"
  shift 2

  if ! "$@" >"$output_path"; then
    echo "artifact isolation rejected $description tool failure" >&2
    exit 1
  fi
}

capture_codesign_metadata_or_reject() {
  local app="$1"
  local output_path="$2"
  local description="$3"

  if ! /usr/bin/codesign -d --verbose=4 "$app" > /dev/null 2>"$output_path"; then
    echo "artifact isolation rejected $description codesign metadata failure" >&2
    exit 1
  fi
}

capture_designated_requirement_or_reject() {
  local app="$1"
  local output_path="$2"
  local description="$3"

  if ! /usr/bin/codesign -d -r- "$app" >"$output_path" 2>&1; then
    echo "artifact isolation rejected $description designated requirement failure" >&2
    exit 1
  fi
}

for app in "$default_app" "$acceptance_app"; do
  if [[ ! -d "$app/Contents" ]]; then
    echo "artifact isolation rejected missing app bundle: $app" >&2
    exit 1
  fi
  /usr/bin/codesign --verify --deep --strict "$app"
done

read_plist_value() {
  /usr/bin/plutil -extract "$2" raw -o - "$1/Contents/Info.plist"
}

read_metadata_value() {
  /usr/bin/awk -F= -v key="$2" '$1 == key {print substr($0, index($0, "=") + 1); exit}' "$1"
}

read_designated_requirement() {
  /usr/bin/awk '/designated =>/ {sub(/^.*designated => /, ""); print; exit}' "$1"
}

assert_equal() {
  if [[ "$1" != "$2" ]]; then
    echo "artifact isolation rejected $3: expected '$2', got '$1'" >&2
    exit 1
  fi
}

assert_equal "$(read_plist_value "$default_app" CFBundleIdentifier)" "$default_identifier" "default bundle identifier"
assert_equal "$(read_plist_value "$acceptance_app" CFBundleIdentifier)" "$acceptance_identifier" "acceptance bundle identifier"
assert_equal "$(read_plist_value "$default_app" CFBundleName)" "$default_name" "default bundle name"
assert_equal "$(read_plist_value "$acceptance_app" CFBundleName)" "$acceptance_name" "acceptance bundle name"
assert_equal "$(read_plist_value "$default_app" CFBundleDisplayName)" "$default_name" "default display name"
assert_equal "$(read_plist_value "$acceptance_app" CFBundleDisplayName)" "$acceptance_name" "acceptance display name"
assert_equal "$(read_plist_value "$default_app" CFBundleExecutable)" "$expected_executable" "default executable name"
assert_equal "$(read_plist_value "$acceptance_app" CFBundleExecutable)" "$expected_executable" "acceptance executable name"

default_codesign_metadata="$scratch_dir/default.codesign-metadata"
acceptance_codesign_metadata="$scratch_dir/acceptance.codesign-metadata"
default_requirement_output="$scratch_dir/default.designated-requirement"
acceptance_requirement_output="$scratch_dir/acceptance.designated-requirement"

capture_codesign_metadata_or_reject "$default_app" "$default_codesign_metadata" "default"
capture_codesign_metadata_or_reject "$acceptance_app" "$acceptance_codesign_metadata" "acceptance"
capture_designated_requirement_or_reject "$default_app" "$default_requirement_output" "default"
capture_designated_requirement_or_reject "$acceptance_app" "$acceptance_requirement_output" "acceptance"

default_codesign_identifier="$(read_metadata_value "$default_codesign_metadata" Identifier)"
acceptance_codesign_identifier="$(read_metadata_value "$acceptance_codesign_metadata" Identifier)"
default_cdhash="$(read_metadata_value "$default_codesign_metadata" CDHash)"
acceptance_cdhash="$(read_metadata_value "$acceptance_codesign_metadata" CDHash)"
default_requirement="$(read_designated_requirement "$default_requirement_output")"
acceptance_requirement="$(read_designated_requirement "$acceptance_requirement_output")"

assert_equal "$default_codesign_identifier" "$default_identifier" "default codesign identifier"
assert_equal "$acceptance_codesign_identifier" "$acceptance_identifier" "acceptance codesign identifier"

if [[ ! "$default_cdhash" =~ ^[[:xdigit:]]{40}$ ]]; then
  echo "artifact isolation rejected invalid default CDHash: '$default_cdhash'" >&2
  exit 1
fi
if [[ ! "$acceptance_cdhash" =~ ^[[:xdigit:]]{40}$ ]]; then
  echo "artifact isolation rejected invalid acceptance CDHash: '$acceptance_cdhash'" >&2
  exit 1
fi
if [[ "$default_cdhash" == "$acceptance_cdhash" ]]; then
  echo "artifact isolation rejected identical default and acceptance CDHashes" >&2
  exit 1
fi
if [[ -z "$default_requirement" ]] ||
  [[ "$default_requirement" != *"identifier \"$default_identifier\""* && "$default_requirement" != *"cdhash H\"$default_cdhash\""* ]]; then
  echo "artifact isolation rejected default designated requirement not bound to its identifier or CDHash" >&2
  exit 1
fi
if [[ -z "$acceptance_requirement" ]] ||
  [[ "$acceptance_requirement" != *"identifier \"$acceptance_identifier\""* && "$acceptance_requirement" != *"cdhash H\"$acceptance_cdhash\""* ]]; then
  echo "artifact isolation rejected acceptance designated requirement not bound to its identifier or CDHash" >&2
  exit 1
fi

default_binary="$default_app/Contents/MacOS/$expected_executable"
acceptance_binary="$acceptance_app/Contents/MacOS/$expected_executable"

for binary in "$default_binary" "$acceptance_binary"; do
  if [[ ! -x "$binary" ]]; then
    echo "artifact isolation rejected missing executable: $binary" >&2
    exit 1
  fi
done

default_symbols="$scratch_dir/default.nm"
acceptance_symbols="$scratch_dir/acceptance.nm"
default_strings="$scratch_dir/default.strings"
acceptance_strings="$scratch_dir/acceptance.strings"
default_sha_output="$scratch_dir/default.sha256"
acceptance_sha_output="$scratch_dir/acceptance.sha256"

capture_stdout_or_reject "$default_symbols" "default nm" /usr/bin/nm "$default_binary"
capture_stdout_or_reject "$acceptance_symbols" "acceptance nm" /usr/bin/nm "$acceptance_binary"
capture_stdout_or_reject "$default_strings" "default strings" /usr/bin/strings -a "$default_binary"
capture_stdout_or_reject "$acceptance_strings" "acceptance strings" /usr/bin/strings -a "$acceptance_binary"
capture_stdout_or_reject "$default_sha_output" "default SHA-256" /usr/bin/shasum -a 256 "$default_binary"
capture_stdout_or_reject "$acceptance_sha_output" "acceptance SHA-256" /usr/bin/shasum -a 256 "$acceptance_binary"

default_binary_sha256="$(/usr/bin/awk '{print $1; exit}' "$default_sha_output")"
acceptance_binary_sha256="$(/usr/bin/awk '{print $1; exit}' "$acceptance_sha_output")"
if [[ ! "$default_binary_sha256" =~ ^[[:xdigit:]]{64}$ ]]; then
  echo "artifact isolation rejected invalid default binary SHA-256: '$default_binary_sha256'" >&2
  exit 1
fi
if [[ ! "$acceptance_binary_sha256" =~ ^[[:xdigit:]]{64}$ ]]; then
  echo "artifact isolation rejected invalid acceptance binary SHA-256: '$acceptance_binary_sha256'" >&2
  exit 1
fi
if [[ "$default_binary_sha256" == "$acceptance_binary_sha256" ]]; then
  echo "artifact isolation rejected identical default and acceptance binaries" >&2
  exit 1
fi

default_symbol_match_code=0
/usr/bin/grep -E 'paper_float_test_|inject_acceptance_' "$default_symbols" >/dev/null || default_symbol_match_code=$?
if [[ $default_symbol_match_code -eq 0 ]]; then
  echo "artifact isolation rejected default binary with acceptance native symbols" >&2
  exit 1
fi
if [[ $default_symbol_match_code -ne 1 ]]; then
  echo "artifact isolation rejected default symbol inspection failure" >&2
  exit 1
fi

acceptance_symbol_match_code=0
/usr/bin/grep 'paper_float_test_inject_tap_disabled' "$acceptance_symbols" >/dev/null || acceptance_symbol_match_code=$?
if [[ $acceptance_symbol_match_code -eq 1 ]]; then
  echo "artifact isolation rejected acceptance binary without tap-disabled injection symbol" >&2
  exit 1
fi
if [[ $acceptance_symbol_match_code -ne 0 ]]; then
  echo "artifact isolation rejected acceptance symbol inspection failure" >&2
  exit 1
fi

for forbidden in \
  "$acceptance_identifier" \
  "$acceptance_name" \
  "deepseek-api-key-acceptance"; do
  default_string_match_code=0
  /usr/bin/grep -F "$forbidden" "$default_strings" >/dev/null || default_string_match_code=$?
  if [[ $default_string_match_code -eq 0 ]]; then
    echo "artifact isolation rejected default binary containing acceptance namespace: $forbidden" >&2
    exit 1
  fi
  if [[ $default_string_match_code -ne 1 ]]; then
    echo "artifact isolation rejected default string inspection failure: $forbidden" >&2
    exit 1
  fi

  acceptance_string_match_code=0
  /usr/bin/grep -F "$forbidden" "$acceptance_strings" >/dev/null || acceptance_string_match_code=$?
  if [[ $acceptance_string_match_code -eq 1 ]]; then
    echo "artifact isolation rejected acceptance binary missing namespace: $forbidden" >&2
    exit 1
  fi
  if [[ $acceptance_string_match_code -ne 0 ]]; then
    echo "artifact isolation rejected acceptance string inspection failure: $forbidden" >&2
    exit 1
  fi
done

echo "artifactIsolation=true"
echo "defaultIdentifier=$default_identifier"
echo "acceptanceIdentifier=$acceptance_identifier"
echo "defaultCDHash=$default_cdhash"
echo "acceptanceCDHash=$acceptance_cdhash"
echo "defaultDesignatedRequirement=$default_requirement"
echo "acceptanceDesignatedRequirement=$acceptance_requirement"
echo "defaultBinarySha256=$default_binary_sha256"
echo "acceptanceBinarySha256=$acceptance_binary_sha256"
