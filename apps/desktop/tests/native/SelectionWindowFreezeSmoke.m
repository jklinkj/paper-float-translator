#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>

#include <stdbool.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>

extern uint32_t paper_float_test_receiving_window_id(CGEventRef event);
extern int paper_float_test_may_read_frozen_selection_node(
  int pidValid,
  int ancestryValid
);
extern int paper_float_test_choose_gesture_window_id(
  uint32_t mouseDownWindowID,
  uint32_t mouseUpWindowID,
  uint32_t *selectedWindowID
);
extern int paper_float_test_choose_gesture_target_pid(
  int mouseDownPID,
  int mouseUpPID,
  int selfPID,
  int *selectedPID
);
extern int paper_float_test_choose_anchored_gesture_target_pid(
  int mouseDownPID,
  int mouseUpPID,
  int selfPID,
  int *selectedPID
);
extern int paper_float_test_choose_selection_window_resolution_strategy(
  int pidStatus,
  int windowStatus,
  int *failureStatus
);
extern int paper_float_test_selection_commit_validation(
  int stateCurrent,
  int requiresAnchorWindowRevalidation,
  int anchoredWindowIdentityCurrent
);
extern int paper_float_test_selection_window_freeze(
  uint32_t frozenWindowID,
  int targetPID,
  int selfPID,
  const uint32_t *windowIDs,
  const int *ownerPIDs,
  const int *layers,
  const int *onscreen,
  const double *windowBounds,
  size_t windowCount,
  const double *axBounds,
  size_t axCount,
  long *matchedAXIndex
);
extern int paper_float_test_observer_context_matches(
  long long frozenGeneration,
  long long activeGeneration,
  uint32_t contextWindowID,
  uint32_t lastMouseWindowID,
  int contextPID,
  int observerPID,
  int lastMousePID,
  int contextInvalid,
  int elementWithinFrozenWindow
);
extern int paper_float_test_observer_mouse_correlation_decision(
  int recentMouseGesture,
  int mouseGenerationCompleted,
  int mouseGestureInvalid,
  int samePID
);
extern int paper_float_test_completed_mouse_echo_needs_text_check(
  int recentMouseGesture,
  int mouseGenerationCompleted,
  int samePID,
  int completedMouseTextAvailable
);
extern int paper_float_test_generation_is_current(
  long long frozenGeneration,
  long long activeGeneration,
  long long frozenLifecycle,
  long long activeLifecycle
);
extern int paper_float_test_frozen_state_cleanup(int stopWatchers);
extern int paper_float_test_pending_frozen_activation_decision(
  int completed,
  int invalid,
  int windowIDAvailable,
  double elapsedSeconds,
  int activatedPID
);
extern int paper_float_test_frozen_context_attempt_decision(
  int attempt,
  int stateCurrent,
  int contextAvailable
);
enum {
  FreezeSuccess = 0,
  FreezeMissingWindowID = 1,
  FreezeWindowIDMismatch = 2,
  FreezeInvalidTargetPID = 3,
  FreezeSelfTarget = 4,
  FreezeMetadataMissing = 5,
  FreezeMetadataAmbiguous = 6,
  FreezeMetadataPIDMismatch = 7,
  FreezeMetadataOffscreen = 8,
  FreezeMetadataLayerMismatch = 9,
  FreezeMetadataBoundsInvalid = 10,
  FreezeAXWindowMissing = 11,
  FreezeAXWindowAmbiguous = 12,
  FreezeGesturePIDMismatch = 13
};

enum {
  WindowResolutionReject = 0,
  WindowResolutionFrozenCGWindow = 1,
  WindowResolutionAnchoredAXWindow = 2
};

enum {
  FrozenContextCancel = 0,
  FrozenContextRetry = 1,
  FrozenContextStartReads = 2,
  FrozenContextTerminalFailure = 3
};

typedef struct {
  int checks;
  int failures;
} SmokeResult;

static void SmokeExpect(SmokeResult *result, bool condition, const char *label) {
  result->checks += 1;
  if (!condition) {
    result->failures += 1;
    fprintf(stderr, "selection-window-freeze check failed: %s\n", label);
  }
}

static int Resolve(
  uint32_t frozenWindowID,
  int targetPID,
  int selfPID,
  const uint32_t *windowIDs,
  const int *ownerPIDs,
  const int *layers,
  const int *onscreen,
  const double *windowBounds,
  size_t windowCount,
  const double *axBounds,
  size_t axCount,
  long *matchedAXIndex
) {
  return paper_float_test_selection_window_freeze(
    frozenWindowID,
    targetPID,
    selfPID,
    windowIDs,
    ownerPIDs,
    layers,
    onscreen,
    windowBounds,
    windowCount,
    axBounds,
    axCount,
    matchedAXIndex
  );
}

int main(void) {
  @autoreleasepool {
    SmokeResult result = {0, 0};

    CGEventRef event = CGEventCreateMouseEvent(
      NULL,
      kCGEventLeftMouseDown,
      CGPointMake(-25, 40),
      kCGMouseButtonLeft
    );
    SmokeExpect(&result, event != NULL, "synthetic CGEvent is available");
    if (event != NULL) {
      CGEventSetIntegerValueField(
        event,
        kCGMouseEventWindowUnderMousePointerThatCanHandleThisEvent,
        101
      );
      CGEventSetIntegerValueField(event, kCGMouseEventWindowUnderMousePointer, 202);
      SmokeExpect(
        &result,
        paper_float_test_receiving_window_id(event) == 101,
        "the handleable public CGEvent window field has priority"
      );
      CGEventSetIntegerValueField(
        event,
        kCGMouseEventWindowUnderMousePointerThatCanHandleThisEvent,
        0
      );
      SmokeExpect(
        &result,
        paper_float_test_receiving_window_id(event) == 202,
        "a missing handleable field uses the public under-pointer candidate"
      );
      CGEventSetIntegerValueField(event, kCGMouseEventWindowUnderMousePointer, 0);
      SmokeExpect(
        &result,
        paper_float_test_receiving_window_id(event) == 0,
        "missing values in both public window fields fail closed"
      );
      CFRelease(event);
    }

    SmokeExpect(
      &result,
      paper_float_test_may_read_frozen_selection_node(1, 1) == 1,
      "selected text may be read only after PID and ancestry validation"
    );
    SmokeExpect(
      &result,
      paper_float_test_may_read_frozen_selection_node(0, 1) == 0,
      "cross-PID child is rejected before selected-text access"
    );
    SmokeExpect(
      &result,
      paper_float_test_may_read_frozen_selection_node(1, 0) == 0,
      "cross-window child is rejected before selected-text access"
    );

    uint32_t selectedWindowID = 0;
    SmokeExpect(
      &result,
      paper_float_test_choose_gesture_window_id(101, 0, &selectedWindowID) == FreezeSuccess &&
        selectedWindowID == 101,
      "mouse-down window ID has priority"
    );
    SmokeExpect(
      &result,
      paper_float_test_choose_gesture_window_id(0, 101, &selectedWindowID) == FreezeSuccess &&
        selectedWindowID == 101,
      "mouse-up window ID is used only when mouse-down has none"
    );
    SmokeExpect(
      &result,
      paper_float_test_choose_gesture_window_id(101, 202, &selectedWindowID) ==
        FreezeWindowIDMismatch,
      "nonzero down/up window mismatch fails closed"
    );
    SmokeExpect(
      &result,
      paper_float_test_choose_gesture_window_id(0, 0, &selectedWindowID) ==
        FreezeMissingWindowID,
      "missing down/up IDs fail closed"
    );

    int selectedPID = 0;
    SmokeExpect(
      &result,
      paper_float_test_choose_gesture_target_pid(400, 0, 999, &selectedPID) == FreezeSuccess &&
        selectedPID == 400,
      "mouse-down PID has priority"
    );
    SmokeExpect(
      &result,
      paper_float_test_choose_gesture_target_pid(400, 401, 999, &selectedPID) ==
        FreezeGesturePIDMismatch,
      "nonzero down/up PID mismatch fails closed"
    );
    SmokeExpect(
      &result,
      paper_float_test_choose_anchored_gesture_target_pid(400, 401, 999, &selectedPID) ==
          FreezeSuccess &&
        selectedPID == 400,
      "an under-pointer AX owner outranks a mismatching public event PID"
    );
    SmokeExpect(
      &result,
      paper_float_test_choose_anchored_gesture_target_pid(0, 401, 999, &selectedPID) ==
        FreezeInvalidTargetPID,
      "an anchored gesture cannot claim an unavailable mouse-down AX owner"
    );
    SmokeExpect(
      &result,
      paper_float_test_choose_anchored_gesture_target_pid(999, 401, 999, &selectedPID) ==
        FreezeSelfTarget,
      "an anchored self-owned mouse-down remains rejected"
    );
    SmokeExpect(
      &result,
      paper_float_test_choose_gesture_target_pid(999, 999, 999, &selectedPID) ==
        FreezeSelfTarget,
      "self process is rejected"
    );
    SmokeExpect(
      &result,
      paper_float_test_choose_gesture_target_pid(0, 0, 999, &selectedPID) ==
        FreezeInvalidTargetPID,
      "missing target PID is rejected"
    );
    int windowResolutionFailure = -1;
    SmokeExpect(
      &result,
      paper_float_test_choose_selection_window_resolution_strategy(
        FreezeSuccess,
        FreezeSuccess,
        &windowResolutionFailure
      ) == WindowResolutionFrozenCGWindow &&
        windowResolutionFailure == FreezeSuccess,
      "a public CGWindow ID keeps the strict frozen-window path"
    );
    SmokeExpect(
      &result,
      paper_float_test_choose_selection_window_resolution_strategy(
        FreezeSuccess,
        FreezeMissingWindowID,
        &windowResolutionFailure
      ) == WindowResolutionAnchoredAXWindow &&
        windowResolutionFailure == FreezeSuccess,
      "missing public CGWindow IDs use PID-and-anchor AX resolution"
    );
    SmokeExpect(
      &result,
      paper_float_test_choose_selection_window_resolution_strategy(
        FreezeGesturePIDMismatch,
        FreezeMissingWindowID,
        &windowResolutionFailure
      ) == WindowResolutionReject &&
        windowResolutionFailure == FreezeGesturePIDMismatch,
      "an unstable event PID is never rescued by AX focus"
    );
    SmokeExpect(
      &result,
      paper_float_test_choose_selection_window_resolution_strategy(
        FreezeSuccess,
        FreezeWindowIDMismatch,
        &windowResolutionFailure
      ) == WindowResolutionReject &&
        windowResolutionFailure == FreezeWindowIDMismatch,
      "conflicting nonzero window IDs remain fail closed"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_commit_validation(1, 1, 1) == 1,
      "windowless text commits only while state and anchored AX identity remain current"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_commit_validation(0, 1, 1) == 0,
      "a stale generation or watcher token cannot commit windowless text"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_commit_validation(1, 1, 0) == 0,
      "a changed frontmost or anchored AX window cannot commit windowless text"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_commit_validation(1, 0, 0) == 1,
      "a strict frozen-CGWindow context does not require the windowless identity branch"
    );
    SmokeExpect(
      &result,
      paper_float_test_frozen_context_attempt_decision(1, 1, 0) ==
        FrozenContextRetry,
      "the first transient frozen-context miss is retried"
    );
    SmokeExpect(
      &result,
      paper_float_test_frozen_context_attempt_decision(2, 1, 0) ==
        FrozenContextRetry,
      "the second transient frozen-context miss is retried"
    );
    SmokeExpect(
      &result,
      paper_float_test_frozen_context_attempt_decision(3, 1, 0) ==
        FrozenContextTerminalFailure,
      "the third frozen-context miss becomes one terminal failure"
    );
    SmokeExpect(
      &result,
      paper_float_test_frozen_context_attempt_decision(2, 1, 1) ==
        FrozenContextStartReads,
      "a later successful context build starts the text-read chain"
    );
    SmokeExpect(
      &result,
      paper_float_test_frozen_context_attempt_decision(2, 0, 1) ==
        FrozenContextCancel,
      "a stale lifecycle or generation cancels even a successful context build"
    );
    SmokeExpect(
      &result,
      paper_float_test_frozen_context_attempt_decision(0, 1, 0) ==
        FrozenContextCancel,
      "an invalid context attempt is rejected"
    );
    const uint32_t orderedWindowIDs[] = {202, 101};
    const int samePIDs[] = {400, 400};
    const int normalLayers[] = {0, 0};
    const int onscreen[] = {1, 1};
    const double orderedWindowBounds[] = {
      600, 40, 700, 500,
      -1440, 32, 800, 600
    };
    const double orderedAXBounds[] = {
      600, 40, 700, 500,
      -1440, 32, 800, 600
    };
    long matchedAXIndex = -1;
    SmokeExpect(
      &result,
      Resolve(
        101,
        400,
        999,
        orderedWindowIDs,
        samePIDs,
        normalLayers,
        onscreen,
        orderedWindowBounds,
        2,
        orderedAXBounds,
        2,
        &matchedAXIndex
      ) == FreezeSuccess && matchedAXIndex == 1,
      "same-PID B/A metadata order resolves frozen A on negative coordinates"
    );

    const uint32_t retinaWindowID[] = {303};
    const int retinaPID[] = {400};
    const int retinaLayer[] = {0};
    const int retinaOnscreen[] = {1};
    const double retinaWindowBounds[] = {100, 120, 500, 400};
    const double retinaPointAXBounds[] = {100, 120, 500, 400};
    const double incorrectlyScaledAXBounds[] = {200, 240, 1000, 800};
    SmokeExpect(
      &result,
      Resolve(
        303,
        400,
        999,
        retinaWindowID,
        retinaPID,
        retinaLayer,
        retinaOnscreen,
        retinaWindowBounds,
        1,
        retinaPointAXBounds,
        1,
        &matchedAXIndex
      ) == FreezeSuccess,
      "Retina CGWindow and AX bounds compare in points without scaling"
    );
    const double previewInsetAXBounds[] = {101.5, 118.5, 498.5, 401.5};
    SmokeExpect(
      &result,
      Resolve(
        303,
        400,
        999,
        retinaWindowID,
        retinaPID,
        retinaLayer,
        retinaOnscreen,
        retinaWindowBounds,
        1,
        previewInsetAXBounds,
        1,
        &matchedAXIndex
      ) == FreezeSuccess && matchedAXIndex == 0,
      "a unique Preview-like AX decoration delta within two points is accepted"
    );
    const double beyondPreviewInsetAXBounds[] = {102.01, 120, 500, 400};
    SmokeExpect(
      &result,
      Resolve(
        303,
        400,
        999,
        retinaWindowID,
        retinaPID,
        retinaLayer,
        retinaOnscreen,
        retinaWindowBounds,
        1,
        beyondPreviewInsetAXBounds,
        1,
        &matchedAXIndex
      ) == FreezeAXWindowMissing,
      "an AX window delta beyond two points remains rejected"
    );
    SmokeExpect(
      &result,
      Resolve(
        303,
        400,
        999,
        retinaWindowID,
        retinaPID,
        retinaLayer,
        retinaOnscreen,
        retinaWindowBounds,
        1,
        incorrectlyScaledAXBounds,
        1,
        &matchedAXIndex
      ) == FreezeAXWindowMissing,
      "backing-pixel scaling is not accepted as an AX match"
    );

    const double duplicateAXBounds[] = {
      100, 120, 500, 400,
      100, 120, 500, 400
    };
    SmokeExpect(
      &result,
      Resolve(
        303,
        400,
        999,
        retinaWindowID,
        retinaPID,
        retinaLayer,
        retinaOnscreen,
        retinaWindowBounds,
        1,
        duplicateAXBounds,
        2,
        &matchedAXIndex
      ) == FreezeAXWindowAmbiguous,
      "same-bounds AX ambiguity fails closed"
    );
    const double nearDuplicateAXBounds[] = {
      100, 120, 500, 400,
      101.5, 118.5, 498.5, 401.5
    };
    SmokeExpect(
      &result,
      Resolve(
        303,
        400,
        999,
        retinaWindowID,
        retinaPID,
        retinaLayer,
        retinaOnscreen,
        retinaWindowBounds,
        1,
        nearDuplicateAXBounds,
        2,
        &matchedAXIndex
      ) == FreezeAXWindowAmbiguous,
      "two Preview-like AX windows inside tolerance remain ambiguous"
    );

    const uint32_t duplicateMetadataIDs[] = {303, 303};
    const int duplicateMetadataPIDs[] = {400, 400};
    SmokeExpect(
      &result,
      Resolve(
        303,
        400,
        999,
        duplicateMetadataIDs,
        duplicateMetadataPIDs,
        normalLayers,
        onscreen,
        orderedWindowBounds,
        2,
        retinaPointAXBounds,
        1,
        &matchedAXIndex
      ) == FreezeMetadataAmbiguous,
      "duplicate CG metadata identity fails closed"
    );
    SmokeExpect(
      &result,
      Resolve(
        0,
        400,
        999,
        retinaWindowID,
        retinaPID,
        retinaLayer,
        retinaOnscreen,
        retinaWindowBounds,
        1,
        retinaPointAXBounds,
        1,
        &matchedAXIndex
      ) == FreezeMissingWindowID,
      "frozen ID zero fails closed"
    );
    SmokeExpect(
      &result,
      Resolve(
        404,
        400,
        999,
        retinaWindowID,
        retinaPID,
        retinaLayer,
        retinaOnscreen,
        retinaWindowBounds,
        1,
        retinaPointAXBounds,
        1,
        &matchedAXIndex
      ) == FreezeMetadataMissing,
      "disappeared frozen window fails closed"
    );

    const int offscreen[] = {0};
    SmokeExpect(
      &result,
      Resolve(
        303,
        400,
        999,
        retinaWindowID,
        retinaPID,
        retinaLayer,
        offscreen,
        retinaWindowBounds,
        1,
        retinaPointAXBounds,
        1,
        &matchedAXIndex
      ) == FreezeMetadataOffscreen,
      "offscreen frozen window fails closed"
    );

    const int wrongPID[] = {401};
    SmokeExpect(
      &result,
      Resolve(
        303,
        400,
        999,
        retinaWindowID,
        wrongPID,
        retinaLayer,
        retinaOnscreen,
        retinaWindowBounds,
        1,
        retinaPointAXBounds,
        1,
        &matchedAXIndex
      ) == FreezeMetadataPIDMismatch,
      "CG owner PID mismatch fails closed"
    );
    SmokeExpect(
      &result,
      Resolve(
        303,
        999,
        999,
        retinaWindowID,
        retinaPID,
        retinaLayer,
        retinaOnscreen,
        retinaWindowBounds,
        1,
        retinaPointAXBounds,
        1,
        &matchedAXIndex
      ) == FreezeSelfTarget,
      "resolver rejects a self-owned target"
    );

    const int elevatedLayer[] = {3};
    SmokeExpect(
      &result,
      Resolve(
        303,
        400,
        999,
        retinaWindowID,
        retinaPID,
        elevatedLayer,
        retinaOnscreen,
        retinaWindowBounds,
        1,
        retinaPointAXBounds,
        1,
        &matchedAXIndex
      ) == FreezeMetadataLayerMismatch,
      "nonzero window layer fails closed"
    );

    SmokeExpect(
      &result,
      paper_float_test_observer_context_matches(
        12, 12, 101, 101, 400, 400, 400, 0, 1
      ) == 1,
      "observer in frozen window may reuse the frozen context"
    );
    SmokeExpect(
      &result,
      paper_float_test_observer_context_matches(
        12, 12, 0, 0, 400, 400, 400, 0, 1
      ) == 1,
      "observer may reuse a PID-and-anchor AX context with no public window ID"
    );
    SmokeExpect(
      &result,
      paper_float_test_observer_context_matches(
        12, 12, 101, 101, 400, 400, 400, 0, 0
      ) == 0,
      "same-PID observer from another window is rejected"
    );
    SmokeExpect(
      &result,
      paper_float_test_observer_context_matches(
        12, 12, 101, 101, 400, 400, 400, 1, 1
      ) == 0,
      "observer cannot revive an invalid frozen context"
    );
    SmokeExpect(
      &result,
      paper_float_test_observer_mouse_correlation_decision(1, 0, 0, 1) == 1,
      "a same-PID observer notification reuses only an unfinished valid mouse generation"
    );
    SmokeExpect(
      &result,
      paper_float_test_observer_mouse_correlation_decision(1, 1, 0, 1) == 0,
      "a notification after mouse completion starts an independent observer generation"
    );
    SmokeExpect(
      &result,
      paper_float_test_observer_mouse_correlation_decision(1, 0, 1, 1) == 0,
      "an invalid mouse gesture cannot absorb a later observer selection"
    );
    SmokeExpect(
      &result,
      paper_float_test_observer_mouse_correlation_decision(1, 0, 0, 0) == 0,
      "a different-PID observer selection cannot be absorbed by the recent mouse window"
    );
    SmokeExpect(
      &result,
      paper_float_test_completed_mouse_echo_needs_text_check(1, 1, 1, 1) == 1,
      "a completed mouse notification compares its text before suppressing a duplicate echo"
    );
    SmokeExpect(
      &result,
      paper_float_test_completed_mouse_echo_needs_text_check(1, 0, 1, 1) == 0,
      "an unfinished mouse notification is correlation evidence rather than a completed echo"
    );
    SmokeExpect(
      &result,
      paper_float_test_generation_is_current(12, 12, 7, 7) == 1,
      "current generation and lifecycle are accepted"
    );
    SmokeExpect(
      &result,
      paper_float_test_generation_is_current(11, 12, 7, 7) == 0,
      "old generation is rejected"
    );
    SmokeExpect(
      &result,
      paper_float_test_generation_is_current(12, 12, 6, 7) == 0,
      "old watcher lifecycle is rejected"
    );
    SmokeExpect(
      &result,
      paper_float_test_pending_frozen_activation_decision(0, 0, 1, 0.5, 400) == 1,
      "same-application activation preserves a live frozen mouse retry"
    );
    SmokeExpect(
      &result,
      paper_float_test_pending_frozen_activation_decision(0, 0, 0, 0.5, 400) == 1,
      "same-application activation preserves a live PID-and-anchor AX retry"
    );
    SmokeExpect(
      &result,
      paper_float_test_pending_frozen_activation_decision(0, 0, 1, 0.5, 401) == 0,
      "activation of another application invalidates the old frozen mouse retry"
    );
    SmokeExpect(
      &result,
      paper_float_test_pending_frozen_activation_decision(1, 0, 1, 0.5, 400) == 0,
      "external activation does not preserve a completed generation"
    );
    SmokeExpect(
      &result,
      paper_float_test_pending_frozen_activation_decision(0, 1, 1, 0.5, 400) == 0,
      "external activation does not preserve an invalid frozen context"
    );
    SmokeExpect(
      &result,
      paper_float_test_pending_frozen_activation_decision(0, 0, 1, 2.0, 400) == 0,
      "external activation does not preserve an expired retry"
    );
    SmokeExpect(
      &result,
      paper_float_test_frozen_state_cleanup(0) == 1,
      "new selection generation clears the prior frozen context"
    );
    SmokeExpect(
      &result,
      paper_float_test_frozen_state_cleanup(1) == 1,
      "watcher stop clears frozen and in-flight gesture state"
    );

    bool passed = result.failures == 0;
    printf(
      "SELECTION_WINDOW_FREEZE_SMOKE %s checks=%d failures=%d\n",
      passed ? "PASS" : "FAIL",
      result.checks,
      result.failures
    );
    return passed ? 0 : 1;
  }
}
