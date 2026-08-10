#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "native watcher context smoke test requires macOS" >&2
  exit 69
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
NATIVE_SOURCE="$APP_ROOT/src-tauri/native/PaperFloatNativeBridge.m"
HARNESS_SOURCE="$APP_ROOT/tests/native/WatcherContextRestartSmoke.m"
FREEZE_HARNESS_SOURCE="$APP_ROOT/tests/native/SelectionWindowFreezeSmoke.m"
SELECTION_WORK_AREA_HARNESS_SOURCE="$APP_ROOT/tests/native/NativeSelectionAndWorkAreaSmoke.m"
MULTI_CLICK_HARNESS_SOURCE="$APP_ROOT/tests/native/MultiClickCoalescingSmoke.m"
TEMP_ROOT="${TMPDIR:-/tmp}"
TEMP_DIR="$(mktemp -d "$TEMP_ROOT/paper-float-native-watcher.XXXXXX")"
trap 'rm -rf "$TEMP_DIR"' EXIT

if [[ ! -f "$NATIVE_SOURCE" || ! -f "$HARNESS_SOURCE" || \
      ! -f "$FREEZE_HARNESS_SOURCE" || \
      ! -f "$SELECTION_WORK_AREA_HARNESS_SOURCE" || \
      ! -f "$MULTI_CLICK_HARNESS_SOURCE" ]]; then
  echo "native source or native smoke harness is missing" >&2
  exit 66
fi

if /usr/bin/grep -q 'localizedName' "$NATIVE_SOURCE"; then
  echo "native diagnostics must not export localized application names" >&2
  exit 1
fi

COMMON_COMPILE_FLAGS=(
  -x objective-c
  -fobjc-arc
  -fblocks
  -O2
  -mmacosx-version-min=11.0
  -Wall
  -Wextra
  -Werror
)

compile_production_object() {
  local target="$1"
  local output="$2"

  /usr/bin/xcrun clang \
    "${COMMON_COMPILE_FLAGS[@]}" \
    -target "$target" \
    -c "$NATIVE_SOURCE" \
    -o "$output"
}

echo "== strict production native compile: arm64 =="
compile_production_object \
  arm64-apple-macosx11.0 \
  "$TEMP_DIR/PaperFloatNativeBridge-arm64.o"

echo "== strict production native compile: x86_64 =="
compile_production_object \
  x86_64-apple-macosx11.0 \
  "$TEMP_DIR/PaperFloatNativeBridge-x86_64.o"

for production_object in \
  "$TEMP_DIR/PaperFloatNativeBridge-arm64.o" \
  "$TEMP_DIR/PaperFloatNativeBridge-x86_64.o"; do
  if /usr/bin/nm -g "$production_object" | /usr/bin/grep -Eq 'paper_float_test_'; then
    echo "test-only native injection symbol leaked into production object" >&2
    exit 1
  fi
done

case "$(uname -m)" in
  arm64)
    HOST_TARGET="arm64-apple-macosx11.0"
    ;;
  x86_64)
    HOST_TARGET="x86_64-apple-macosx11.0"
    ;;
  *)
    echo "unsupported macOS architecture: $(uname -m)" >&2
    exit 69
    ;;
esac

compile_watcher_harness() {
  local target="$1"
  local output="$2"

  /usr/bin/xcrun clang \
    -fobjc-arc \
    -fblocks \
    -O0 \
    -DPAPER_FLOAT_NATIVE_TESTING=1 \
    -target "$target" \
    -mmacosx-version-min=11.0 \
    -Wall \
    -Wextra \
    -Werror \
    "$NATIVE_SOURCE" \
    "$HARNESS_SOURCE" \
    -framework AppKit \
    -framework ApplicationServices \
    -framework CoreGraphics \
    -framework Foundation \
    -framework LocalAuthentication \
    -framework Security \
    -o "$output"
}

echo "== link watcher-context harness: arm64 =="
compile_watcher_harness \
  arm64-apple-macosx11.0 \
  "$TEMP_DIR/WatcherContextRestartSmoke-arm64"

echo "== link watcher-context harness: x86_64 =="
compile_watcher_harness \
  x86_64-apple-macosx11.0 \
  "$TEMP_DIR/WatcherContextRestartSmoke-x86_64"

case "$(uname -m)" in
  arm64)
    HARNESS_BINARY="$TEMP_DIR/WatcherContextRestartSmoke-arm64"
    ;;
  x86_64)
    HARNESS_BINARY="$TEMP_DIR/WatcherContextRestartSmoke-x86_64"
    ;;
esac

HARNESS_APP="$TEMP_DIR/WatcherContextRestartSmoke.app"
HARNESS_APP_EXECUTABLE="$HARNESS_APP/Contents/MacOS/WatcherContextRestartSmoke"
HARNESS_APP_INFO="$HARNESS_APP/Contents/Info.plist"
/bin/mkdir -p "$HARNESS_APP/Contents/MacOS"
/bin/cp "$HARNESS_BINARY" "$HARNESS_APP_EXECUTABLE"
/usr/bin/plutil -create xml1 "$HARNESS_APP_INFO"
/usr/bin/plutil -insert CFBundleExecutable -string WatcherContextRestartSmoke "$HARNESS_APP_INFO"
/usr/bin/plutil -insert CFBundleIdentifier -string dev.paperfloat.watcher-context-smoke "$HARNESS_APP_INFO"
/usr/bin/plutil -insert CFBundleName -string WatcherContextRestartSmoke "$HARNESS_APP_INFO"
/usr/bin/plutil -insert CFBundlePackageType -string APPL "$HARNESS_APP_INFO"
/usr/bin/plutil -insert CFBundleVersion -string 1 "$HARNESS_APP_INFO"

echo "== run 50-restart ownership smoke test =="
echo "note: event-tap, Accessibility, Input Monitoring, and signing availability are not asserted"
HARNESS_STDERR="$TEMP_DIR/harness.stderr"
if ! SMOKE_OUTPUT="$(OS_ACTIVITY_MODE=disable "$HARNESS_APP_EXECUTABLE" 2>"$HARNESS_STDERR")"; then
  /bin/cat "$HARNESS_STDERR" >&2
  printf '%s\n' "$SMOKE_OUTPUT" >&2
  exit 1
fi
if [[ -s "$HARNESS_STDERR" ]]; then
  echo "note: macOS emitted non-fatal service diagnostics; they are not interpreted as a permission result"
fi
printf '%s\n' "$SMOKE_OUTPUT"

EXPECTED_OUTPUT="NATIVE_WATCHER_CONTEXT_SMOKE PASS starts=51 restarts=50 releases=51 callbackAfterRelease=0 lifecycleMonotonic=true firstLifecycle=2 lastLifecycle=52 resourceSnapshots=52 resourceFailures=0 peakSourceSets=1 finalSources=0 toggles=50 selectionEnabledStarts=26 selectionDisabledStarts=25 disabledReadiness=25 unexpectedDisabledSelection=0 lifecycleRaces=true raceFinalSources=0 diagnostics=3 diagnosticProtocol=true tapInjection=true privacy=true"
if [[ "$SMOKE_OUTPUT" != "$EXPECTED_OUTPUT" ]]; then
  echo "unexpected watcher-context smoke output" >&2
  exit 1
fi

compile_freeze_harness() {
  local target="$1"
  local output="$2"

  /usr/bin/xcrun clang \
    -fobjc-arc \
    -fblocks \
    -O0 \
    -DPAPER_FLOAT_NATIVE_TESTING=1 \
    -target "$target" \
    -mmacosx-version-min=11.0 \
    -Wall \
    -Wextra \
    -Werror \
    "$NATIVE_SOURCE" \
    "$FREEZE_HARNESS_SOURCE" \
    -framework AppKit \
    -framework ApplicationServices \
    -framework CoreGraphics \
    -framework Foundation \
    -framework LocalAuthentication \
    -framework Security \
    -o "$output"
}

echo "== link selection-window-freeze harness: arm64 =="
compile_freeze_harness \
  arm64-apple-macosx11.0 \
  "$TEMP_DIR/SelectionWindowFreezeSmoke-arm64"

echo "== link selection-window-freeze harness: x86_64 =="
compile_freeze_harness \
  x86_64-apple-macosx11.0 \
  "$TEMP_DIR/SelectionWindowFreezeSmoke-x86_64"

case "$(uname -m)" in
  arm64)
    FREEZE_HARNESS_BINARY="$TEMP_DIR/SelectionWindowFreezeSmoke-arm64"
    ;;
  x86_64)
    FREEZE_HARNESS_BINARY="$TEMP_DIR/SelectionWindowFreezeSmoke-x86_64"
    ;;
esac

echo "== run deterministic selection-window freeze smoke test =="
echo "note: the harness uses injected public CG/AX geometry snapshots and does not require TCC"
if ! FREEZE_SMOKE_OUTPUT="$(OS_ACTIVITY_MODE=disable "$FREEZE_HARNESS_BINARY")"; then
  printf '%s\n' "$FREEZE_SMOKE_OUTPUT" >&2
  exit 1
fi
printf '%s\n' "$FREEZE_SMOKE_OUTPUT"

EXPECTED_FREEZE_OUTPUT="SELECTION_WINDOW_FREEZE_SMOKE PASS checks=67 failures=0"
if [[ "$FREEZE_SMOKE_OUTPUT" != "$EXPECTED_FREEZE_OUTPUT" ]]; then
  echo "unexpected selection-window-freeze smoke output" >&2
  exit 1
fi

compile_selection_work_area_harness() {
  local target="$1"
  local output="$2"

  /usr/bin/xcrun clang \
    -fobjc-arc \
    -fblocks \
    -O0 \
    -DPAPER_FLOAT_NATIVE_TESTING=1 \
    -target "$target" \
    -mmacosx-version-min=11.0 \
    -Wall \
    -Wextra \
    -Werror \
    "$NATIVE_SOURCE" \
    "$SELECTION_WORK_AREA_HARNESS_SOURCE" \
    -framework AppKit \
    -framework ApplicationServices \
    -framework CoreGraphics \
    -framework Foundation \
    -framework LocalAuthentication \
    -framework Security \
    -o "$output"
}

echo "== link selection-anchor/work-area harness: arm64 =="
compile_selection_work_area_harness \
  arm64-apple-macosx11.0 \
  "$TEMP_DIR/NativeSelectionAndWorkAreaSmoke-arm64"

echo "== link selection-anchor/work-area harness: x86_64 =="
compile_selection_work_area_harness \
  x86_64-apple-macosx11.0 \
  "$TEMP_DIR/NativeSelectionAndWorkAreaSmoke-x86_64"

case "$(uname -m)" in
  arm64)
    SELECTION_WORK_AREA_HARNESS_BINARY="$TEMP_DIR/NativeSelectionAndWorkAreaSmoke-arm64"
    ;;
  x86_64)
    SELECTION_WORK_AREA_HARNESS_BINARY="$TEMP_DIR/NativeSelectionAndWorkAreaSmoke-x86_64"
    ;;
esac

echo "== run deterministic selection-anchor/work-area smoke test =="
echo "note: the harness uses injected public AX and CG/AppKit geometry snapshots and does not require TCC"
if ! SELECTION_WORK_AREA_SMOKE_OUTPUT="$(OS_ACTIVITY_MODE=disable "$SELECTION_WORK_AREA_HARNESS_BINARY")"; then
  printf '%s\n' "$SELECTION_WORK_AREA_SMOKE_OUTPUT" >&2
  exit 1
fi
printf '%s\n' "$SELECTION_WORK_AREA_SMOKE_OUTPUT"

EXPECTED_SELECTION_WORK_AREA_OUTPUT="NATIVE_SELECTION_WORK_AREA_SMOKE PASS checks=109 failures=0"
if [[ "$SELECTION_WORK_AREA_SMOKE_OUTPUT" != "$EXPECTED_SELECTION_WORK_AREA_OUTPUT" ]]; then
  echo "unexpected selection-anchor/work-area smoke output" >&2
  exit 1
fi

compile_multi_click_harness() {
  local target="$1"
  local output="$2"

  /usr/bin/xcrun clang \
    -fobjc-arc \
    -fblocks \
    -O0 \
    -DPAPER_FLOAT_NATIVE_TESTING=1 \
    -target "$target" \
    -mmacosx-version-min=11.0 \
    -Wall \
    -Wextra \
    -Werror \
    "$NATIVE_SOURCE" \
    "$MULTI_CLICK_HARNESS_SOURCE" \
    -framework AppKit \
    -framework ApplicationServices \
    -framework CoreGraphics \
    -framework Foundation \
    -framework LocalAuthentication \
    -framework Security \
    -o "$output"
}

echo "== link multi-click coalescing harness: arm64 =="
compile_multi_click_harness \
  arm64-apple-macosx11.0 \
  "$TEMP_DIR/MultiClickCoalescingSmoke-arm64"

echo "== link multi-click coalescing harness: x86_64 =="
compile_multi_click_harness \
  x86_64-apple-macosx11.0 \
  "$TEMP_DIR/MultiClickCoalescingSmoke-x86_64"

case "$(uname -m)" in
  arm64)
    MULTI_CLICK_HARNESS_BINARY="$TEMP_DIR/MultiClickCoalescingSmoke-arm64"
    ;;
  x86_64)
    MULTI_CLICK_HARNESS_BINARY="$TEMP_DIR/MultiClickCoalescingSmoke-x86_64"
    ;;
esac

echo "== run real-timer multi-click coalescing smoke test =="
echo "note: main-queue timers and production invalidation are exercised without GUI or TCC"
if ! MULTI_CLICK_SMOKE_OUTPUT="$(OS_ACTIVITY_MODE=disable "$MULTI_CLICK_HARNESS_BINARY")"; then
  printf '%s\n' "$MULTI_CLICK_SMOKE_OUTPUT" >&2
  exit 1
fi
printf '%s\n' "$MULTI_CLICK_SMOKE_OUTPUT"

EXPECTED_MULTI_CLICK_OUTPUT="MULTI_CLICK_COALESCING_SMOKE PASS checks=95 failures=0"
if [[ "$MULTI_CLICK_SMOKE_OUTPUT" != "$EXPECTED_MULTI_CLICK_OUTPUT" ]]; then
  echo "unexpected multi-click coalescing smoke output" >&2
  exit 1
fi
