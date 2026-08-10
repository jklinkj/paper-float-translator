#import <Foundation/Foundation.h>
#import <ApplicationServices/ApplicationServices.h>

#include <math.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

extern int paper_float_test_may_read_frozen_selection_node(
  int pidValid,
  int ancestryValid
);
extern int paper_float_test_resolve_selection_anchor(
  double anchorX,
  double anchorY,
  const double *candidateBounds,
  const int *boundsAvailable,
  size_t candidateCount,
  long *selectedIndex
);
extern int paper_float_test_resolve_selection_anchor_groups(
  double anchorX,
  double anchorY,
  const double *candidateBounds,
  const int *boundsAvailable,
  const size_t *candidateGroupIndices,
  size_t candidateCount,
  size_t groupCount,
  long *selectedGroupIndex
);
extern int paper_float_test_combine_selected_range_texts(
  const char *const *parts,
  const int *partsAvailable,
  size_t partCount,
  char *output,
  size_t outputCapacity
);
extern int paper_float_test_preferred_parent_fallback_allowed(
  int rawAXError,
  int parentHasText
);
extern int paper_float_test_stable_range_read_can_emit(
  int identityValidBeforeRead,
  int allRangeTextRead,
  int identityValidAfterRead
);
extern int paper_float_test_preferred_direct_read_decision(
  int firstStatus,
  int secondStatus,
  int textsEqual
);
extern int paper_float_test_anchored_focused_direct_read_status(
  int selectedRangesStable,
  int anchorMatchesSelectedRangeBounds,
  int firstStatus,
  int secondStatus,
  int textsEqual
);
extern int paper_float_test_mouse_primary_requires_anchor_proof(
  int anchorElementValid,
  int focusedElementSafe,
  int focusedElementCoveredByPrimaryAncestry
);
extern int paper_float_test_direct_selected_text_may_emit(
  int decision,
  int requiresSelectionChangeEvidence,
  int selectionChangeObserved,
  int baselineComparable,
  int textMatchesBaseline
);
extern double paper_float_test_selection_baseline_maximum_ax_budget_seconds(void);
extern double paper_float_test_selection_baseline_operation_timeout(double remainingSeconds);
extern int paper_float_test_selection_baseline_supplemental_focus_allowed(double remainingSeconds);
extern int paper_float_test_selection_baseline_maximum_depth(void);
extern int paper_float_test_selection_baseline_may_keep_partial(
  int allowPartial,
  unsigned long previouslyCapturedCount,
  int currentValueComplete,
  int budgetExhausted
);
extern int paper_float_test_selection_baseline_timeout_was_accepted(int error);
extern int paper_float_test_anchor_identity_revalidation(
  int withinFrozenWindow,
  int frozenAnchorAvailable,
  int exactElementMatch,
  int nearbyAncestryMatch
);
extern int paper_float_test_focused_preferred_element_can_be_used(
  int belongsToTargetPID,
  int isWithinFrozenWindow,
  int isFrozenWindow,
  int duplicatesPrimaryAncestry,
  int sharesControlSubtree,
  int frameContainsAnchor
);
extern int paper_float_test_mouse_selection_reason(
  int clickCount,
  int isShiftSelection,
  char *output,
  size_t outputCapacity
);
extern int paper_float_test_visible_work_area_for_anchor(
  double anchorX,
  double anchorY,
  const uint32_t *displayIDs,
  const double *displayBounds,
  size_t displayCount,
  const uint32_t *screenDisplayIDs,
  const double *screenFrames,
  const double *screenVisibleFrames,
  size_t screenCount,
  uint32_t *selectedDisplayID,
  double *visibleWorkArea
);

enum {
  SelectionAnchorSuccess = 0,
  SelectionAnchorNoCandidate = 1,
  SelectionAnchorAmbiguous = 2,
  SelectionAnchorBoundsUnavailable = 3,
  SelectionAnchorInvalidInput = 4
};

enum {
  VisibleWorkAreaSuccess = 0,
  VisibleWorkAreaInvalidInput = 1,
  VisibleWorkAreaDisplayNotFound = 2,
  VisibleWorkAreaDisplayAmbiguous = 3,
  VisibleWorkAreaScreenNotFound = 4,
  VisibleWorkAreaScreenAmbiguous = 5,
  VisibleWorkAreaGeometryInvalid = 6
};

enum {
  DirectReadSafeAbsence = 0,
  DirectReadText = 1,
  DirectReadFailure = 2
};

enum {
  PreferredDirectTryRanges = 0,
  PreferredDirectEmit = 1,
  PreferredDirectFailClosed = 2
};

typedef struct {
  int checks;
  int failures;
} SmokeResult;

static void SmokeExpect(SmokeResult *result, bool condition, const char *label) {
  result->checks += 1;
  if (!condition) {
    result->failures += 1;
    fprintf(stderr, "selection/work-area check failed: %s\n", label);
  }
}

static bool NearlyEqual(double lhs, double rhs) {
  return isfinite(lhs) && isfinite(rhs) && fabs(lhs - rhs) < 0.001;
}

static bool WorkAreaEquals(
  const double *actual,
  double x,
  double y,
  double width,
  double height
) {
  return NearlyEqual(actual[0], x) &&
    NearlyEqual(actual[1], y) &&
    NearlyEqual(actual[2], width) &&
    NearlyEqual(actual[3], height);
}

int main(void) {
  @autoreleasepool {
    SmokeResult result = {0, 0};

    const double twoControlBounds[] = {
      40, 40, 260, 90,
      40, 220, 260, 90
    };
    const int twoAvailable[] = {1, 1};
    long selectedIndex = -1;
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        120, 250, twoControlBounds, twoAvailable, 2, &selectedIndex
      ) == SelectionAnchorSuccess && selectedIndex == 1,
      "same-window old selection is ignored in favor of the uniquely anchor-owned control"
    );
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        120, 80, twoControlBounds, twoAvailable, 2, &selectedIndex
      ) == SelectionAnchorSuccess && selectedIndex == 0,
      "the first control remains selectable when it owns the frozen anchor"
    );
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        500, 500, twoControlBounds, twoAvailable, 2, &selectedIndex
      ) == SelectionAnchorSuccess && selectedIndex == 1,
      "non-intersecting candidates resolve only through a strictly unique nearest distance"
    );

    const double overlappingBounds[] = {
      20, 20, 200, 100,
      100, 40, 200, 100
    };
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        150, 70, overlappingBounds, twoAvailable, 2, &selectedIndex
      ) == SelectionAnchorAmbiguous && selectedIndex == -1,
      "multiple bounds containing the anchor are ambiguous"
    );

    const int oneUnavailable[] = {1, 0};
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        120, 80, twoControlBounds, oneUnavailable, 2, &selectedIndex
      ) == SelectionAnchorBoundsUnavailable && selectedIndex == -1,
      "one bounds-less selected candidate makes the entire fallback fail closed"
    );
    const int unavailable[] = {0};
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        120, 80, twoControlBounds, unavailable, 1, &selectedIndex
      ) == SelectionAnchorBoundsUnavailable && selectedIndex == -1,
      "fallback fails closed when no selected candidate has usable bounds"
    );

    const double negativeSizeBounds[] = {20, 20, -10, 30};
    const int available[] = {1};
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        20, 20, negativeSizeBounds, available, 1, &selectedIndex
      ) == SelectionAnchorInvalidInput,
      "negative selection width fails closed"
    );
    const double zeroHeightBounds[] = {20, 20, 30, 0};
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        20, 20, zeroHeightBounds, available, 1, &selectedIndex
      ) == SelectionAnchorInvalidInput,
      "zero selection height fails closed"
    );
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        NAN, 20, twoControlBounds, twoAvailable, 2, &selectedIndex
      ) == SelectionAnchorInvalidInput,
      "non-finite anchor fails closed"
    );

    const double negativeOriginBounds[] = {-1420, 40, 320, 100};
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        -1200, 80, negativeOriginBounds, available, 1, &selectedIndex
      ) == SelectionAnchorSuccess && selectedIndex == 0,
      "valid selection bounds on a negative-origin display remain selectable"
    );
    const double multilineUnionBounds[] = {80, 100, 420, 96};
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        470, 188, multilineUnionBounds, available, 1, &selectedIndex
      ) == SelectionAnchorSuccess && selectedIndex == 0,
      "multi-line range union containing the endpoint owns the anchor"
    );

    const double equalDistanceBounds[] = {
      20, 100, 70, 40,
      110, 100, 70, 40
    };
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        100, 120, equalDistanceBounds, twoAvailable, 2, &selectedIndex
      ) == SelectionAnchorAmbiguous && selectedIndex == -1,
      "equidistant nearby candidates fail closed"
    );
    const double uniqueNearestBounds[] = {
      20, 100, 70, 40,
      150, 100, 70, 40
    };
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        100, 120, uniqueNearestBounds, twoAvailable, 2, &selectedIndex
      ) == SelectionAnchorSuccess && selectedIndex == 0,
      "a uniquely nearest candidate inside the explicit 24-point tolerance is selected"
    );
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        400, 400, uniqueNearestBounds, twoAvailable, 2, &selectedIndex
      ) == SelectionAnchorSuccess && selectedIndex == 1,
      "the strictly unique nearest candidate is deterministic even without an intersection"
    );
    const double chainedEpsilonTieBounds[] = {
      0, 100, 90, 40,
      109.80, 100, 70, 40,
      109.55, 100, 70, 40
    };
    const int threeAvailable[] = {1, 1, 1};
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        100, 120, chainedEpsilonTieBounds, threeAvailable, 3, &selectedIndex
      ) == SelectionAnchorAmbiguous && selectedIndex == -1,
      "a candidate within epsilon of the true minimum remains an ambiguity"
    );
    const double sharedEdgeBounds[] = {
      20, 100, 80, 40,
      100, 100, 80, 40
    };
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        100, 120, sharedEdgeBounds, twoAvailable, 2, &selectedIndex
      ) == SelectionAnchorAmbiguous,
      "a shared-edge anchor is ambiguous rather than order-dependent"
    );
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        10, 10, NULL, NULL, 0, &selectedIndex
      ) == SelectionAnchorNoCandidate,
      "empty candidate set fails closed"
    );
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor(
        10, 10, NULL, available, 1, &selectedIndex
      ) == SelectionAnchorInvalidInput,
      "missing candidate storage is rejected"
    );
    SmokeExpect(
      &result,
      paper_float_test_may_read_frozen_selection_node(1, 1) == 1,
      "content is eligible only after PID and full ancestry validation"
    );
    SmokeExpect(
      &result,
      paper_float_test_may_read_frozen_selection_node(0, 1) == 0,
      "cross-PID candidate content is never read"
    );
    SmokeExpect(
      &result,
      paper_float_test_may_read_frozen_selection_node(1, 0) == 0,
      "cross-window candidate content is never read"
    );

    const double multiRangeGroupBounds[] = {
      10, 10, 60, 24,
      10, 90, 100, 24
    };
    const size_t sameGroup[] = {0, 0};
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor_groups(
        80, 100,
        multiRangeGroupBounds, twoAvailable, sameGroup, 2, 1, &selectedIndex
      ) == SelectionAnchorSuccess && selectedIndex == 0,
      "an anchor in the second range selects the complete multi-range element group"
    );
    const double overlappingSameGroupBounds[] = {
      20, 20, 100, 60,
      60, 40, 100, 60
    };
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor_groups(
        80, 60,
        overlappingSameGroupBounds, twoAvailable, sameGroup, 2, 1, &selectedIndex
      ) == SelectionAnchorSuccess && selectedIndex == 0,
      "overlapping ranges in one element are one candidate rather than an ambiguity"
    );
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor_groups(
        80, 100,
        multiRangeGroupBounds, oneUnavailable, sameGroup, 2, 1, &selectedIndex
      ) == SelectionAnchorBoundsUnavailable && selectedIndex == -1,
      "one missing bound in a multi-range group fails the whole fallback closed"
    );
    const double invalidMultiRangeGroupBounds[] = {
      10, 10, 60, 24,
      10, 90, -100, 24
    };
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor_groups(
        80, 100,
        invalidMultiRangeGroupBounds, twoAvailable, sameGroup, 2, 1, &selectedIndex
      ) == SelectionAnchorInvalidInput && selectedIndex == -1,
      "one invalid bound in a multi-range group fails the whole fallback closed"
    );
    const size_t differentGroups[] = {0, 1};
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor_groups(
        80, 60,
        overlappingSameGroupBounds, twoAvailable, differentGroups, 2, 2, &selectedIndex
      ) == SelectionAnchorAmbiguous && selectedIndex == -1,
      "overlapping ranges belonging to different elements remain ambiguous"
    );
    const double groupedNearestBounds[] = {
      0, 100, 20, 20,
      120, 100, 20, 20,
      135, 100, 20, 20
    };
    const size_t groupedNearestGroups[] = {0, 0, 1};
    SmokeExpect(
      &result,
      paper_float_test_resolve_selection_anchor_groups(
        100, 110,
        groupedNearestBounds, threeAvailable, groupedNearestGroups, 3, 2,
        &selectedIndex
      ) == SelectionAnchorSuccess && selectedIndex == 0,
      "nearest distance is the minimum across every range in an element group"
    );

    const char *orderedParts[] = {"first range", "second range"};
    const int orderedPartsAvailable[] = {1, 1};
    char combinedText[64] = {0};
    SmokeExpect(
      &result,
      paper_float_test_combine_selected_range_texts(
        orderedParts, orderedPartsAvailable, 2, combinedText, sizeof(combinedText)
      ) == 1 && strcmp(combinedText, "first range\nsecond range") == 0,
      "all selected-range text is combined once in original AX range order"
    );
    const int onePartUnavailable[] = {1, 0};
    SmokeExpect(
      &result,
      paper_float_test_combine_selected_range_texts(
        orderedParts, onePartUnavailable, 2, combinedText, sizeof(combinedText)
      ) == 0 && combinedText[0] == '\0',
      "one unreadable selected range prevents partial text emission"
    );
    const char *blankParts[] = {"first range", "   "};
    SmokeExpect(
      &result,
      paper_float_test_combine_selected_range_texts(
        blankParts, orderedPartsAvailable, 2, combinedText, sizeof(combinedText)
      ) == 0 && combinedText[0] == '\0',
      "one blank selected range prevents partial text emission"
    );
    SmokeExpect(
      &result,
      paper_float_test_combine_selected_range_texts(
        orderedParts, orderedPartsAvailable, 2, combinedText, 4
      ) == 0 && combinedText[0] == '\0',
      "insufficient output storage fails without returning truncated text"
    );
    SmokeExpect(
      &result,
      paper_float_test_preferred_parent_fallback_allowed(
        kAXErrorInvalidUIElement, 1
      ) == 0,
      "an invalid anchor element cannot fall through to stale parent text"
    );
    SmokeExpect(
      &result,
      paper_float_test_preferred_parent_fallback_allowed(kAXErrorFailure, 1) == 0,
      "a generic AX failure cannot fall through to stale parent text"
    );
    SmokeExpect(
      &result,
      paper_float_test_preferred_parent_fallback_allowed(
        kAXErrorCannotComplete, 1
      ) == 0,
      "an AX timeout cannot fall through to stale parent text"
    );
    SmokeExpect(
      &result,
      paper_float_test_preferred_parent_fallback_allowed(
        kAXErrorAttributeUnsupported, 1
      ) == 1,
      "an unsupported selection attribute may continue to a capable parent"
    );
    SmokeExpect(
      &result,
      paper_float_test_preferred_parent_fallback_allowed(kAXErrorNoValue, 1) == 1,
      "a safely absent selection value may continue to a capable parent"
    );
    SmokeExpect(
      &result,
      paper_float_test_stable_range_read_can_emit(1, 1, 1) == 1,
      "stable range identity with every text part read may emit"
    );
    SmokeExpect(
      &result,
      paper_float_test_stable_range_read_can_emit(0, 1, 1) == 0,
      "range identity must match before any selected text is read"
    );
    SmokeExpect(
      &result,
      paper_float_test_stable_range_read_can_emit(1, 0, 1) == 0,
      "partial selected-range text can never emit"
    );
    SmokeExpect(
      &result,
      paper_float_test_stable_range_read_can_emit(1, 1, 0) == 0,
      "range identity must remain stable after selected text is read"
    );
    SmokeExpect(
      &result,
      paper_float_test_preferred_direct_read_decision(
        DirectReadText, DirectReadText, 1
      ) == PreferredDirectEmit,
      "stable direct selected text emits before incomplete range capabilities"
    );
    SmokeExpect(
      &result,
      paper_float_test_preferred_direct_read_decision(
        DirectReadSafeAbsence, DirectReadSafeAbsence, 0
      ) == PreferredDirectTryRanges,
      "safe direct-text absence falls back to selected ranges"
    );
    SmokeExpect(
      &result,
      paper_float_test_preferred_direct_read_decision(
        DirectReadText, DirectReadText, 0
      ) == PreferredDirectFailClosed,
      "changing direct selected text fails closed"
    );
    SmokeExpect(
      &result,
      paper_float_test_preferred_direct_read_decision(
        DirectReadText, DirectReadSafeAbsence, 0
      ) == PreferredDirectFailClosed,
      "a selection disappearing between direct reads cannot emit or fall through"
    );
    SmokeExpect(
      &result,
      paper_float_test_preferred_direct_read_decision(
        DirectReadFailure, DirectReadSafeAbsence, 0
      ) == PreferredDirectFailClosed,
      "a hard direct-read error cannot fall through to stale parent or range text"
    );
    SmokeExpect(
      &result,
      paper_float_test_anchored_focused_direct_read_status(
        1, 1, DirectReadText, DirectReadText, 1
      ) == DirectReadText,
      "focused direct text emits only with stable ranges anchored to this gesture"
    );
    SmokeExpect(
      &result,
      paper_float_test_anchored_focused_direct_read_status(
        1, 0, DirectReadText, DirectReadText, 1
      ) == DirectReadSafeAbsence,
      "stable text from a prior selection is rejected when this gesture anchor misses its bounds"
    );
    SmokeExpect(
      &result,
      paper_float_test_anchored_focused_direct_read_status(
        0, 1, DirectReadText, DirectReadText, 1
      ) == DirectReadFailure,
      "changed selected ranges fail closed instead of continuing to a parent"
    );
    SmokeExpect(
      &result,
      paper_float_test_anchored_focused_direct_read_status(
        0, 1, DirectReadSafeAbsence, DirectReadSafeAbsence, 0
      ) == DirectReadFailure,
      "changed ranges still fail closed when direct selected text disappears"
    );
    SmokeExpect(
      &result,
      paper_float_test_anchored_focused_direct_read_status(
        1, 1, DirectReadText, DirectReadText, 0
      ) == DirectReadFailure,
      "focused text is rejected when repeated reads do not match"
    );
    SmokeExpect(
      &result,
      paper_float_test_anchored_focused_direct_read_status(
        1, 1, DirectReadSafeAbsence, DirectReadSafeAbsence, 0
      ) == DirectReadSafeAbsence,
      "stable anchored ranges without direct text may continue to an anchored parent"
    );
    SmokeExpect(
      &result,
      paper_float_test_anchored_focused_direct_read_status(
        1, 1, DirectReadFailure, DirectReadSafeAbsence, 0
      ) == DirectReadFailure,
      "an anchored focused direct-read failure always fails closed"
    );
    SmokeExpect(
      &result,
      paper_float_test_mouse_primary_requires_anchor_proof(1, 0, 1) == 0,
      "a hit-tested ancestry keeps stable direct-only Preview reads reachable"
    );
    SmokeExpect(
      &result,
      paper_float_test_mouse_primary_requires_anchor_proof(0, 1, 0) == 1,
      "a focused fallback promoted to primary keeps anchor proof"
    );
    SmokeExpect(
      &result,
      paper_float_test_mouse_primary_requires_anchor_proof(1, 1, 0) == 0,
      "a distinct hit-tested primary remains on the ordinary anchored hit-test path"
    );
    SmokeExpect(
      &result,
      paper_float_test_mouse_primary_requires_anchor_proof(0, 0, 0) == 0,
      "an unsafe focused element cannot create an anchored primary read"
    );
    SmokeExpect(
      &result,
      paper_float_test_direct_selected_text_may_emit(
        PreferredDirectEmit, 1, 0, 1, 1
      ) == 0,
      "stable direct text matching the mouse-down baseline cannot emit as a new drag"
    );
    SmokeExpect(
      &result,
      paper_float_test_direct_selected_text_may_emit(
        PreferredDirectEmit, 1, 0, 1, 0
      ) == 1,
      "Preview direct-only text may emit when it differs from the mouse-down baseline"
    );
    SmokeExpect(
      &result,
      paper_float_test_direct_selected_text_may_emit(
        PreferredDirectEmit, 1, 1, 0, 0
      ) == 1,
      "an AX selected-text notification is sufficient current-gesture evidence when available"
    );
    SmokeExpect(
      &result,
      paper_float_test_direct_selected_text_may_emit(
        PreferredDirectEmit, 0, 0, 0, 0
      ) == 1,
      "an independent AX observer trigger supplies its own direct-text change evidence"
    );
    SmokeExpect(
      &result,
      fabs(paper_float_test_selection_baseline_maximum_ax_budget_seconds() - 0.045) < 0.0001,
      "the active event-tap baseline uses one 45 ms wall-clock deadline"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_baseline_maximum_depth() == 16,
      "the mouse-down anchor baseline covers the same bounded ancestry depth as mouse-up"
    );
    SmokeExpect(
      &result,
      fabs(paper_float_test_selection_baseline_operation_timeout(0.020) - 0.003) < 0.0001,
      "a baseline AX step is capped at three milliseconds when budget remains"
    );
    SmokeExpect(
      &result,
      fabs(paper_float_test_selection_baseline_operation_timeout(0.001) - 0.001) < 0.0001,
      "the final AX step is capped by the smaller remaining deadline budget"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_baseline_operation_timeout(0.0) == 0.0,
      "no new AX step starts after the baseline deadline"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_baseline_supplemental_focus_allowed(0.009) == 1,
      "supplemental focus may run only with its full reserved budget"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_baseline_supplemental_focus_allowed(0.0089) == 0,
      "supplemental focus is skipped when it could consume the anchor deadline"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_baseline_may_keep_partial(1, 1, 0, 1) == 1,
      "deadline exhaustion preserves already completed anchor ancestry values"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_baseline_may_keep_partial(1, 0, 1, 1) == 1,
      "a completed current anchor value survives a parent-step deadline"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_baseline_may_keep_partial(1, 0, 0, 1) == 0,
      "deadline exhaustion before any completed anchor value fails closed"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_baseline_may_keep_partial(1, 2, 0, 0) == 0,
      "non-budget AX failures cannot be hidden by a partial anchor snapshot"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_baseline_timeout_was_accepted(kAXErrorSuccess) == 1,
      "a baseline AX operation may proceed only after the short timeout is accepted"
    );
    SmokeExpect(
      &result,
      paper_float_test_selection_baseline_timeout_was_accepted(kAXErrorIllegalArgument) == 0,
      "a rejected short timeout fails the pre-delivery baseline closed"
    );
    SmokeExpect(
      &result,
      paper_float_test_anchor_identity_revalidation(1, 1, 1, 0) == 1,
      "an unchanged hit-tested anchor element survives commit validation"
    );
    SmokeExpect(
      &result,
      paper_float_test_anchor_identity_revalidation(1, 1, 0, 1) == 1,
      "a changed AX leaf may commit only while its immediate frozen control identity remains"
    );
    SmokeExpect(
      &result,
      paper_float_test_anchor_identity_revalidation(1, 1, 0, 0) == 0,
      "same-window scroll or overlay retargeting cannot commit text from the old anchor control"
    );
    SmokeExpect(
      &result,
      paper_float_test_anchor_identity_revalidation(0, 1, 1, 1) == 0,
      "an anchor outside the frozen window always fails commit validation"
    );
    SmokeExpect(
      &result,
      paper_float_test_focused_preferred_element_can_be_used(1, 1, 0, 0, 1, 0) == 1,
      "a focused Preview element in the anchored control subtree may be a secondary seed"
    );
    SmokeExpect(
      &result,
      paper_float_test_focused_preferred_element_can_be_used(1, 1, 0, 0, 0, 1) == 1,
      "a focused Preview element whose frozen frame contains the anchor may be a secondary seed"
    );
    SmokeExpect(
      &result,
      paper_float_test_focused_preferred_element_can_be_used(0, 1, 0, 0, 1, 1) == 0,
      "a focused element from another PID is rejected"
    );
    SmokeExpect(
      &result,
      paper_float_test_focused_preferred_element_can_be_used(1, 0, 0, 0, 1, 1) == 0,
      "a focused element outside the frozen window is rejected"
    );
    SmokeExpect(
      &result,
      paper_float_test_focused_preferred_element_can_be_used(1, 1, 1, 0, 1, 1) == 0,
      "the frozen window root cannot become a direct-text seed"
    );
    SmokeExpect(
      &result,
      paper_float_test_focused_preferred_element_can_be_used(1, 1, 0, 1, 1, 1) == 0,
      "a focused element already covered by primary ancestry is not read twice"
    );
    SmokeExpect(
      &result,
      paper_float_test_focused_preferred_element_can_be_used(1, 1, 0, 0, 0, 0) == 0,
      "an unanchored focused element cannot expose stale selected text"
    );

    char mouseReason[64] = {0};
    SmokeExpect(
      &result,
      paper_float_test_mouse_selection_reason(
        2, 0, mouseReason, sizeof(mouseReason)
      ) == 1 && strcmp(mouseReason, "mouse_up_double_click") == 0,
      "a double click has its own acceptance diagnostic reason"
    );
    SmokeExpect(
      &result,
      paper_float_test_mouse_selection_reason(
        3, 0, mouseReason, sizeof(mouseReason)
      ) == 1 && strcmp(mouseReason, "mouse_up_triple_click") == 0,
      "a triple click has its own acceptance diagnostic reason"
    );
    SmokeExpect(
      &result,
      paper_float_test_mouse_selection_reason(
        7, 0, mouseReason, sizeof(mouseReason)
      ) == 1 && strcmp(mouseReason, "mouse_up_triple_click") == 0,
      "click counts above three remain in the triple-click acceptance bucket"
    );
    SmokeExpect(
      &result,
      paper_float_test_mouse_selection_reason(
        2, 1, mouseReason, sizeof(mouseReason)
      ) == 1 && strcmp(mouseReason, "mouse_up_double_click") == 0,
      "double-click classification retains precedence over Shift metadata"
    );
    SmokeExpect(
      &result,
      paper_float_test_mouse_selection_reason(
        1, 1, mouseReason, sizeof(mouseReason)
      ) == 1 && strcmp(mouseReason, "mouse_up_shift") == 0,
      "non-multiclick Shift selection keeps its existing diagnostic reason"
    );
    SmokeExpect(
      &result,
      paper_float_test_mouse_selection_reason(
        1, 0, mouseReason, sizeof(mouseReason)
      ) == 1 && strcmp(mouseReason, "mouse_up_drag") == 0,
      "non-multiclick drag selection keeps its existing diagnostic reason"
    );

    const uint32_t primaryID[] = {1};
    const double primaryBounds[] = {0, 0, 1440, 900};
    const uint32_t primaryScreenID[] = {1};
    const double primaryFrame[] = {0, 0, 1440, 900};
    double workArea[4] = {0, 0, 0, 0};
    uint32_t selectedDisplayID = 0;

    const double menuVisible[] = {0, 0, 1440, 875};
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        100, 100,
        primaryID, primaryBounds, 1,
        primaryScreenID, primaryFrame, menuVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaSuccess && selectedDisplayID == 1 &&
        WorkAreaEquals(workArea, 0, 25, 1440, 875),
      "AppKit top inset maps the menu bar into CG top-left coordinates"
    );

    const double bottomDockVisible[] = {0, 70, 1440, 805};
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        100, 100,
        primaryID, primaryBounds, 1,
        primaryScreenID, primaryFrame, bottomDockVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaSuccess && WorkAreaEquals(workArea, 0, 25, 1440, 805),
      "bottom Dock inset reduces height without flipping the CG origin"
    );

    const double leftDockVisible[] = {80, 0, 1360, 875};
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        100, 100,
        primaryID, primaryBounds, 1,
        primaryScreenID, primaryFrame, leftDockVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaSuccess && WorkAreaEquals(workArea, 80, 25, 1360, 875),
      "left Dock inset maps from local AppKit geometry"
    );

    const double rightDockVisible[] = {0, 0, 1360, 875};
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        100, 100,
        primaryID, primaryBounds, 1,
        primaryScreenID, primaryFrame, rightDockVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaSuccess && WorkAreaEquals(workArea, 0, 25, 1360, 875),
      "right Dock inset maps from local AppKit geometry"
    );

    const uint32_t twoDisplayIDs[] = {1, 2};
    const double twoDisplayBounds[] = {
      0, 0, 1440, 900,
      -1440, 0, 1440, 900
    };
    const uint32_t reversedScreenIDs[] = {2, 1};
    const double reversedFrames[] = {
      -1440, 0, 1440, 900,
      0, 0, 1440, 900
    };
    const double reversedVisible[] = {
      -1440, 0, 1440, 875,
      0, 0, 1440, 875
    };
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        -500, 100,
        twoDisplayIDs, twoDisplayBounds, 2,
        reversedScreenIDs, reversedFrames, reversedVisible, 2,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaSuccess && selectedDisplayID == 2 &&
        WorkAreaEquals(workArea, -1440, 25, 1440, 875),
      "negative CG display origin maps by display ID, independent of screen ordering"
    );

    const uint32_t verticalID[] = {3};
    const double verticalCGBounds[] = {0, -1200, 1920, 1200};
    const double verticalAppKitFrame[] = {0, 900, 1920, 1200};
    const double verticalVisible[] = {0, 900, 1920, 1175};
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        500, -500,
        verticalID, verticalCGBounds, 1,
        verticalID, verticalAppKitFrame, verticalVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaSuccess &&
        WorkAreaEquals(workArea, 0, -1175, 1920, 1175),
      "vertical AppKit origin is not reused as CG top-left Y"
    );

    const uint32_t retinaID[] = {4};
    const double retinaCGBounds[] = {1440, 0, 1440, 900};
    const double retinaFrame[] = {1440, 0, 1440, 900};
    const double retinaVisible[] = {1440, 0, 1440, 875};
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        1800, 200,
        retinaID, retinaCGBounds, 1,
        retinaID, retinaFrame, retinaVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaSuccess &&
        WorkAreaEquals(workArea, 1440, 25, 1440, 875),
      "Retina and mixed-scale displays remain in logical coordinates"
    );

    const uint32_t mixedScaleIDs[] = {10, 11};
    const double mixedScaleCGBounds[] = {
      0, 0, 1920, 1080,
      1920, 0, 1440, 900
    };
    const double mixedScaleFrames[] = {
      0, 0, 1920, 1080,
      1920, 0, 1440, 900
    };
    const double mixedScaleVisible[] = {
      0, 0, 1920, 1055,
      1920, 0, 1440, 875
    };
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        2500, 200,
        mixedScaleIDs, mixedScaleCGBounds, 2,
        mixedScaleIDs, mixedScaleFrames, mixedScaleVisible, 2,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaSuccess && selectedDisplayID == 11 &&
        WorkAreaEquals(workArea, 1920, 25, 1440, 875),
      "mixed logical display sizes do not introduce backing-pixel scaling"
    );

    const uint32_t belowID[] = {12};
    const double belowCGBounds[] = {0, 900, 1280, 1024};
    const double belowFrame[] = {0, -1024, 1280, 1024};
    const double belowVisible[] = {0, -1024, 1280, 1000};
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        500, 1200,
        belowID, belowCGBounds, 1,
        belowID, belowFrame, belowVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaSuccess &&
        WorkAreaEquals(workArea, 0, 924, 1280, 1000),
      "a display below the primary maps AppKit top inset to increasing CG Y"
    );

    const uint32_t duplicateDisplayIDs[] = {1, 1};
    const double duplicateDisplayBounds[] = {
      0, 0, 1440, 900,
      2000, 0, 1440, 900
    };
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        100, 100,
        duplicateDisplayIDs, duplicateDisplayBounds, 2,
        primaryScreenID, primaryFrame, menuVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaDisplayAmbiguous,
      "duplicate CG display identifiers fail the unique display-to-screen mapping"
    );

    const uint32_t duplicateScreenIDs[] = {1, 1};
    const double duplicateFrames[] = {
      0, 0, 1440, 900,
      0, 0, 1440, 900
    };
    const double duplicateVisible[] = {
      0, 0, 1440, 875,
      0, 0, 1440, 875
    };
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        100, 100,
        primaryID, primaryBounds, 1,
        duplicateScreenIDs, duplicateFrames, duplicateVisible, 2,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaScreenAmbiguous,
      "duplicate NSScreen mapping fails closed"
    );

    const uint32_t missingScreenID[] = {9};
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        100, 100,
        primaryID, primaryBounds, 1,
        missingScreenID, primaryFrame, menuVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaScreenNotFound,
      "missing NSScreen mapping fails closed"
    );

    const double adjacentBounds[] = {
      0, 0, 1440, 900,
      1440, 0, 1440, 900
    };
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        1440, 100,
        twoDisplayIDs, adjacentBounds, 2,
        reversedScreenIDs, reversedFrames, reversedVisible, 2,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaDisplayAmbiguous,
      "display seam anchor fails closed instead of choosing by array order"
    );
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        NAN, 100,
        primaryID, primaryBounds, 1,
        primaryScreenID, primaryFrame, menuVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaInvalidInput,
      "non-finite work-area anchor fails closed"
    );

    const double invalidDisplayBounds[] = {0, 0, -1440, 900};
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        100, 100,
        primaryID, invalidDisplayBounds, 1,
        primaryScreenID, primaryFrame, menuVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaGeometryInvalid,
      "invalid CG display bounds fail closed"
    );
    const double outsideVisible[] = {-10, 0, 1450, 875};
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        100, 100,
        primaryID, primaryBounds, 1,
        primaryScreenID, primaryFrame, outsideVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaGeometryInvalid,
      "visibleFrame outside frame fails closed"
    );
    const double backingPixelFrame[] = {0, 0, 2880, 1800};
    const double backingPixelVisible[] = {0, 0, 2880, 1750};
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        100, 100,
        primaryID, primaryBounds, 1,
        primaryScreenID, backingPixelFrame, backingPixelVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaGeometryInvalid,
      "backing-pixel scaling is rejected instead of mixed into logical coordinates"
    );
    const double zeroVisible[] = {0, 0, 1440, 0};
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        100, 100,
        primaryID, primaryBounds, 1,
        primaryScreenID, primaryFrame, zeroVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaGeometryInvalid,
      "zero-sized visibleFrame fails closed"
    );
    SmokeExpect(
      &result,
      paper_float_test_visible_work_area_for_anchor(
        4000, 4000,
        primaryID, primaryBounds, 1,
        primaryScreenID, primaryFrame, menuVisible, 1,
        &selectedDisplayID, workArea
      ) == VisibleWorkAreaDisplayNotFound,
      "anchor outside every display fails closed"
    );

    bool passed = result.failures == 0;
    printf(
      "NATIVE_SELECTION_WORK_AREA_SMOKE %s checks=%d failures=%d\n",
      passed ? "PASS" : "FAIL",
      result.checks,
      result.failures
    );
    return passed ? 0 : 1;
  }
}
