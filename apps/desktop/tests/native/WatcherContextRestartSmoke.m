#import <CoreFoundation/CoreFoundation.h>
#import <Foundation/Foundation.h>

#include <stdbool.h>
#include <stdint.h>
#include <dispatch/dispatch.h>
#include <math.h>
#include <stdio.h>
#include <string.h>

typedef void (*PaperFloatWatcherCallback)(
  void *context,
  int event_type,
  const char *text,
  double x,
  double y,
  long long delta,
  int target_pid
);

extern void paper_float_watchers_start(
  void *context,
  PaperFloatWatcherCallback callback,
  int selectionEnabled
);
extern void paper_float_watchers_stop(void);

typedef struct {
  uint32_t version;
  uint32_t selection_observer_count;
  uint32_t selection_observer_source_count;
  uint32_t key_event_tap_count;
  uint32_t key_event_tap_source_count;
  uint32_t mouse_event_tap_count;
  uint32_t mouse_event_tap_source_count;
  uint32_t pasteboard_timer_count;
  uint32_t workspace_activation_observer_count;
  uint32_t callback_count;
  uint32_t context_count;
  uint32_t effective_source_set_count;
  uint32_t total_source_count;
  long long watcher_lifecycle_token;
} PaperFloatNativeResourceSnapshot;

extern int paper_float_native_resource_snapshot(PaperFloatNativeResourceSnapshot *snapshot);
extern int paper_float_test_selection_retry_terminal(int attempt);
extern int paper_float_test_observer_empty_terminal(int correlatedWithMouseGesture);
extern void paper_float_test_emit_selection_read_diagnostic(
  int outcome,
  long long generation,
  int attempt,
  double triggerToReadMs,
  int targetPID
);
extern int paper_float_test_inject_tap_disabled(
  int tapKind,
  int timeout,
  int reenableSucceeds,
  int observerReady
);
extern void paper_float_test_set_multi_click_system_interval(double systemInterval);

enum {
  PaperFloatEventPasteboardStatus = 1,
  PaperFloatEventSelectionStatus = 2,
  PaperFloatEventWatcherContextRelease = 7,
  SmokeRestartCount = 50,
  SmokeContextCount = SmokeRestartCount + 1,
  SmokeLifecycleRaceContextCount = 5,
  SmokeDiagnosticCount = 3,
  SmokeTapStatusCapacity = 16,
  SmokePayloadCapacity = 512
};

typedef struct {
  int releaseCount;
  int callbackAfterReleaseCount;
  long long releaseLifecycle;
  bool selectionEnabled;
  int disabledReadinessCount;
  int unexpectedDisabledSelectionCallbackCount;
} SmokeContext;

static SmokeContext gContexts[SmokeContextCount];
static SmokeContext gLifecycleRaceContexts[SmokeLifecycleRaceContextCount];
static char gDiagnosticPayloads[SmokeDiagnosticCount][SmokePayloadCapacity];
static int gDiagnosticTargetPIDs[SmokeDiagnosticCount];
static int gDiagnosticCount = 0;
static char gTapStatusPayloads[SmokeTapStatusCapacity][SmokePayloadCapacity];
static int gTapStatusTargetPIDs[SmokeTapStatusCapacity];
static long long gTapStatusRecoveryMicros[SmokeTapStatusCapacity];
static int gTapStatusCount = 0;

static void SmokeCallback(
  void *rawContext,
  int eventType,
  const char *text,
  double x,
  double y,
  long long delta,
  int targetPID
) {
  (void)text;
  (void)x;
  (void)y;

  SmokeContext *context = rawContext;
  if (eventType == PaperFloatEventWatcherContextRelease) {
    context->releaseCount += 1;
    context->releaseLifecycle = delta;
  } else if (context->releaseCount > 0) {
    context->callbackAfterReleaseCount += 1;
  }

  if (!context->selectionEnabled && eventType == PaperFloatEventSelectionStatus) {
    if (text != NULL &&
        strcmp(text, "selection_disabled_by_setting") == 0 &&
        targetPID == 0 &&
        delta == 0) {
      context->disabledReadinessCount += 1;
    } else {
      context->unexpectedDisabledSelectionCallbackCount += 1;
    }
  }
  if (!context->selectionEnabled &&
      (eventType == 5 || eventType == 6)) {
    context->unexpectedDisabledSelectionCallbackCount += 1;
  }

  if (text != NULL &&
      eventType == PaperFloatEventSelectionStatus &&
      strncmp(text, "selection_read_", strlen("selection_read_")) == 0 &&
      gDiagnosticCount < SmokeDiagnosticCount) {
    (void)snprintf(
      gDiagnosticPayloads[gDiagnosticCount],
      SmokePayloadCapacity,
      "%s",
      text
    );
    gDiagnosticTargetPIDs[gDiagnosticCount] = targetPID;
    gDiagnosticCount += 1;
  }

  if (text != NULL &&
      (eventType == PaperFloatEventPasteboardStatus ||
       eventType == PaperFloatEventSelectionStatus) &&
      strstr(text, "tap_disabled_") != NULL &&
      gTapStatusCount < SmokeTapStatusCapacity) {
    (void)snprintf(
      gTapStatusPayloads[gTapStatusCount],
      SmokePayloadCapacity,
      "%s",
      text
    );
    gTapStatusTargetPIDs[gTapStatusCount] = targetPID;
    gTapStatusRecoveryMicros[gTapStatusCount] = delta;
    gTapStatusCount += 1;
  }
}

static uint32_t SmokeMax(uint32_t lhs, uint32_t rhs) {
  return lhs > rhs ? lhs : rhs;
}

static void SmokeRecordResourcePeaks(
  PaperFloatNativeResourceSnapshot *peaks,
  const PaperFloatNativeResourceSnapshot *snapshot
) {
  peaks->selection_observer_count = SmokeMax(
    peaks->selection_observer_count,
    snapshot->selection_observer_count
  );
  peaks->selection_observer_source_count = SmokeMax(
    peaks->selection_observer_source_count,
    snapshot->selection_observer_source_count
  );
  peaks->key_event_tap_count = SmokeMax(
    peaks->key_event_tap_count,
    snapshot->key_event_tap_count
  );
  peaks->key_event_tap_source_count = SmokeMax(
    peaks->key_event_tap_source_count,
    snapshot->key_event_tap_source_count
  );
  peaks->mouse_event_tap_count = SmokeMax(
    peaks->mouse_event_tap_count,
    snapshot->mouse_event_tap_count
  );
  peaks->mouse_event_tap_source_count = SmokeMax(
    peaks->mouse_event_tap_source_count,
    snapshot->mouse_event_tap_source_count
  );
  peaks->pasteboard_timer_count = SmokeMax(
    peaks->pasteboard_timer_count,
    snapshot->pasteboard_timer_count
  );
  peaks->workspace_activation_observer_count = SmokeMax(
    peaks->workspace_activation_observer_count,
    snapshot->workspace_activation_observer_count
  );
  peaks->callback_count = SmokeMax(peaks->callback_count, snapshot->callback_count);
  peaks->context_count = SmokeMax(peaks->context_count, snapshot->context_count);
  peaks->effective_source_set_count = SmokeMax(
    peaks->effective_source_set_count,
    snapshot->effective_source_set_count
  );
  peaks->total_source_count = SmokeMax(
    peaks->total_source_count,
    snapshot->total_source_count
  );
}

static bool SmokeRunningSnapshotIsValid(
  const PaperFloatNativeResourceSnapshot *snapshot,
  long long expectedLifecycle,
  bool selectionEnabled
) {
  uint32_t expectedTotal =
    snapshot->selection_observer_source_count +
    snapshot->key_event_tap_source_count +
    snapshot->mouse_event_tap_source_count +
    snapshot->pasteboard_timer_count +
    snapshot->workspace_activation_observer_count;
  bool selectionResourcesValid = selectionEnabled
    ? snapshot->workspace_activation_observer_count == 1
    : snapshot->selection_observer_count == 0 &&
      snapshot->selection_observer_source_count == 0 &&
      snapshot->mouse_event_tap_count == 0 &&
      snapshot->mouse_event_tap_source_count == 0 &&
      snapshot->workspace_activation_observer_count == 0;
  uint32_t minimumSourceCount = selectionEnabled ? 2 : 1;
  uint32_t maximumSourceCount = selectionEnabled ? 5 : 2;
  return snapshot->version == 1 &&
    snapshot->selection_observer_count <= 1 &&
    snapshot->selection_observer_source_count <= 1 &&
    snapshot->selection_observer_count == snapshot->selection_observer_source_count &&
    snapshot->key_event_tap_count <= 1 &&
    snapshot->key_event_tap_source_count <= 1 &&
    snapshot->key_event_tap_count == snapshot->key_event_tap_source_count &&
    snapshot->mouse_event_tap_count <= 1 &&
    snapshot->mouse_event_tap_source_count <= 1 &&
    snapshot->mouse_event_tap_count == snapshot->mouse_event_tap_source_count &&
    snapshot->pasteboard_timer_count == 1 &&
    selectionResourcesValid &&
    snapshot->callback_count == 1 &&
    snapshot->context_count == 1 &&
    snapshot->effective_source_set_count == 1 &&
    snapshot->total_source_count == expectedTotal &&
    snapshot->total_source_count >= minimumSourceCount &&
    snapshot->total_source_count <= maximumSourceCount &&
    snapshot->watcher_lifecycle_token == expectedLifecycle;
}

static bool SmokeStoppedSnapshotIsValid(
  const PaperFloatNativeResourceSnapshot *snapshot,
  long long expectedLifecycle
) {
  return snapshot->version == 1 &&
    snapshot->selection_observer_count == 0 &&
    snapshot->selection_observer_source_count == 0 &&
    snapshot->key_event_tap_count == 0 &&
    snapshot->key_event_tap_source_count == 0 &&
    snapshot->mouse_event_tap_count == 0 &&
    snapshot->mouse_event_tap_source_count == 0 &&
    snapshot->pasteboard_timer_count == 0 &&
    snapshot->workspace_activation_observer_count == 0 &&
    snapshot->callback_count == 0 &&
    snapshot->context_count == 0 &&
    snapshot->effective_source_set_count == 0 &&
    snapshot->total_source_count == 0 &&
    snapshot->watcher_lifecycle_token == expectedLifecycle;
}

static bool SmokeResourceSnapshotsEqual(
  const PaperFloatNativeResourceSnapshot *lhs,
  const PaperFloatNativeResourceSnapshot *rhs
) {
  return lhs->version == rhs->version &&
    lhs->selection_observer_count == rhs->selection_observer_count &&
    lhs->selection_observer_source_count == rhs->selection_observer_source_count &&
    lhs->key_event_tap_count == rhs->key_event_tap_count &&
    lhs->key_event_tap_source_count == rhs->key_event_tap_source_count &&
    lhs->mouse_event_tap_count == rhs->mouse_event_tap_count &&
    lhs->mouse_event_tap_source_count == rhs->mouse_event_tap_source_count &&
    lhs->pasteboard_timer_count == rhs->pasteboard_timer_count &&
    lhs->workspace_activation_observer_count ==
      rhs->workspace_activation_observer_count &&
    lhs->callback_count == rhs->callback_count &&
    lhs->context_count == rhs->context_count &&
    lhs->effective_source_set_count == rhs->effective_source_set_count &&
    lhs->total_source_count == rhs->total_source_count &&
    lhs->watcher_lifecycle_token == rhs->watcher_lifecycle_token;
}

static bool SmokeDiagnosticMatches(
  int index,
  NSString *status,
  NSString *reason,
  NSString *found,
  NSInteger candidateCount,
  double durationMs,
  NSString *sourceBundleIdentifier,
  NSString *axError,
  long long generation,
  NSInteger attempt,
  double triggerToReadMs,
  NSString *terminal,
  int targetPID
) {
  if (index < 0 || index >= gDiagnosticCount) {
    return false;
  }
  NSString *payload = [NSString stringWithUTF8String:gDiagnosticPayloads[index]];
  NSArray<NSString *> *fields = [payload componentsSeparatedByString:@"\t"];
  if (fields.count != 11) {
    return false;
  }
  return [fields[0] isEqualToString:status] &&
    [fields[1] isEqualToString:reason] &&
    [fields[2] isEqualToString:found] &&
    fields[3].integerValue == candidateCount &&
    fabs(fields[4].doubleValue - durationMs) < 0.05 &&
    [fields[5] isEqualToString:sourceBundleIdentifier] &&
    [fields[6] isEqualToString:axError] &&
    fields[7].longLongValue == generation &&
    fields[8].integerValue == attempt &&
    fabs(fields[9].doubleValue - triggerToReadMs) < 0.05 &&
    [fields[10] isEqualToString:terminal] &&
    gDiagnosticTargetPIDs[index] == targetPID;
}

static bool SmokeDiagnosticsPreservePrivacy(void) {
  const char *forbidden[] = {
    "TOP_SECRET_SELECTION",
    "PRIVATE_WINDOW_TITLE",
    "text_length",
    "text_hash",
    "sha256"
  };
  for (int index = 0; index < gDiagnosticCount; index += 1) {
    for (size_t forbiddenIndex = 0;
         forbiddenIndex < sizeof(forbidden) / sizeof(forbidden[0]);
         forbiddenIndex += 1) {
      if (strstr(gDiagnosticPayloads[index], forbidden[forbiddenIndex]) != NULL) {
        return false;
      }
    }
  }
  return true;
}

static bool SmokeTapStatusObserved(const char *expected) {
  for (int index = 0; index < gTapStatusCount; index += 1) {
    if (strcmp(gTapStatusPayloads[index], expected) == 0 &&
        gTapStatusTargetPIDs[index] == 0 &&
        gTapStatusRecoveryMicros[index] > 0) {
      return true;
    }
  }
  return false;
}

int main(void) {
  @autoreleasepool {
    paper_float_test_set_multi_click_system_interval(0.50);
    PaperFloatNativeResourceSnapshot resourcePeaks = {0};
    int resourceSnapshotCount = 0;
    int resourceFailures = 0;
    int selectionEnabledStartCount = 0;
    int selectionDisabledStartCount = 0;
    bool nullSnapshotRejected = paper_float_native_resource_snapshot(NULL) == 0;

    for (int index = 0; index < SmokeContextCount; index += 1) {
      bool selectionEnabled = index % 2 == 0;
      gContexts[index].selectionEnabled = selectionEnabled;
      selectionEnabledStartCount += selectionEnabled ? 1 : 0;
      selectionDisabledStartCount += selectionEnabled ? 0 : 1;
      paper_float_watchers_start(
        &gContexts[index],
        SmokeCallback,
        selectionEnabled ? 1 : 0
      );
      PaperFloatNativeResourceSnapshot snapshot = {0};
      bool snapshotRead = paper_float_native_resource_snapshot(&snapshot) == 1;
      resourceSnapshotCount += 1;
      if (!snapshotRead ||
          !SmokeRunningSnapshotIsValid(&snapshot, index + 1, selectionEnabled)) {
        resourceFailures += 1;
      }
      if (snapshotRead) {
        SmokeRecordResourcePeaks(&resourcePeaks, &snapshot);
      }
    }

    paper_float_test_emit_selection_read_diagnostic(0, 101, 1, 12.5, 501);
    paper_float_test_emit_selection_read_diagnostic(1, 102, 2, 305.0, 502);
    paper_float_test_emit_selection_read_diagnostic(2, 103, 3, 650.0, 503);

    bool fixedTapInjectionsAccepted =
      paper_float_test_inject_tap_disabled(1, 1, 1, 0) == 1 &&
      paper_float_test_inject_tap_disabled(1, 0, 0, 0) == 1 &&
      paper_float_test_inject_tap_disabled(2, 1, 1, 0) == 1 &&
      paper_float_test_inject_tap_disabled(2, 0, 0, 0) == 1 &&
      paper_float_test_inject_tap_disabled(2, 0, 0, 1) == 1;
    dispatch_semaphore_t backgroundInjectionScheduled = dispatch_semaphore_create(0);
    __block int backgroundInjectionAccepted = 0;
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
      backgroundInjectionAccepted =
        paper_float_test_inject_tap_disabled(1, 1, 0, 0);
      dispatch_semaphore_signal(backgroundInjectionScheduled);
    });
    bool backgroundTapInjectionScheduled = false;
    NSDate *injectionDeadline = [NSDate dateWithTimeIntervalSinceNow:1.0];
    while (!backgroundTapInjectionScheduled && injectionDeadline.timeIntervalSinceNow > 0) {
      backgroundTapInjectionScheduled = dispatch_semaphore_wait(
        backgroundInjectionScheduled,
        DISPATCH_TIME_NOW
      ) == 0;
      if (!backgroundTapInjectionScheduled) {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.01, true);
      }
    }
    CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, false);

    paper_float_watchers_stop();
    bool stoppedMouseTapInjectionRejected =
      paper_float_test_inject_tap_disabled(2, 1, 1, 1) == 0;
    PaperFloatNativeResourceSnapshot stoppedSnapshot = {0};
    bool stoppedSnapshotRead = paper_float_native_resource_snapshot(&stoppedSnapshot) == 1;
    resourceSnapshotCount += 1;
    if (!stoppedSnapshotRead ||
        !SmokeStoppedSnapshotIsValid(&stoppedSnapshot, SmokeContextCount + 1)) {
      resourceFailures += 1;
    }

    // Drain queued starts/stops and leave time for stale delayed callbacks to surface.
    CFRunLoopRunInMode(kCFRunLoopDefaultMode, 1.5, false);

    // Reproduce the cross-thread overtaking that a Rust-side mutex alone
    // cannot prevent: a background start queues work to main, then a main-
    // thread stop executes synchronously. The newer stop must invalidate the
    // queued start, release its detached context, and leave zero sources.
    SmokeContext *staleStartContext = &gLifecycleRaceContexts[0];
    staleStartContext->selectionEnabled = false;
    dispatch_semaphore_t backgroundStartSubmitted = dispatch_semaphore_create(0);
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
      paper_float_watchers_start(staleStartContext, SmokeCallback, 0);
      dispatch_semaphore_signal(backgroundStartSubmitted);
    });
    bool backgroundStartWasSubmitted = dispatch_semaphore_wait(
      backgroundStartSubmitted,
      dispatch_time(DISPATCH_TIME_NOW, (int64_t)NSEC_PER_SEC)
    ) == 0;
    paper_float_watchers_stop();
    CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, false);
    PaperFloatNativeResourceSnapshot startThenStopSnapshot = {0};
    bool startThenStopSnapshotRead =
      paper_float_native_resource_snapshot(&startThenStopSnapshot) == 1;
    bool startThenStopOrdered =
      backgroundStartWasSubmitted &&
      startThenStopSnapshotRead &&
      SmokeStoppedSnapshotIsValid(
        &startThenStopSnapshot,
        startThenStopSnapshot.watcher_lifecycle_token
      ) &&
      startThenStopSnapshot.watcher_lifecycle_token >
        stoppedSnapshot.watcher_lifecycle_token &&
      staleStartContext->releaseCount == 1 &&
      staleStartContext->releaseLifecycle == 0 &&
      staleStartContext->callbackAfterReleaseCount == 0;

    // Verify the reverse overtake for normal (non-exit) lifecycle semantics:
    // a background stop is queued first, then a newer main-thread start runs
    // synchronously. The stale stop must no-op and preserve the latest start.
    SmokeContext *stopThenStartBaseContext = &gLifecycleRaceContexts[1];
    SmokeContext *stopThenStartLatestContext = &gLifecycleRaceContexts[2];
    stopThenStartBaseContext->selectionEnabled = false;
    stopThenStartLatestContext->selectionEnabled = false;
    paper_float_watchers_start(stopThenStartBaseContext, SmokeCallback, 0);
    dispatch_semaphore_t backgroundStopSubmitted = dispatch_semaphore_create(0);
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
      paper_float_watchers_stop();
      dispatch_semaphore_signal(backgroundStopSubmitted);
    });
    bool backgroundStopWasSubmitted = dispatch_semaphore_wait(
      backgroundStopSubmitted,
      dispatch_time(DISPATCH_TIME_NOW, (int64_t)NSEC_PER_SEC)
    ) == 0;
    paper_float_watchers_start(stopThenStartLatestContext, SmokeCallback, 0);
    PaperFloatNativeResourceSnapshot stopThenStartBeforeDrainSnapshot = {0};
    bool stopThenStartBeforeDrainSnapshotRead =
      paper_float_native_resource_snapshot(&stopThenStartBeforeDrainSnapshot) == 1;
    CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, false);
    PaperFloatNativeResourceSnapshot stopThenStartRunningSnapshot = {0};
    bool stopThenStartRunningSnapshotRead =
      paper_float_native_resource_snapshot(&stopThenStartRunningSnapshot) == 1;
    bool latestStartSurvived =
      backgroundStopWasSubmitted &&
      stopThenStartBeforeDrainSnapshotRead &&
      stopThenStartRunningSnapshotRead &&
      SmokeRunningSnapshotIsValid(
        &stopThenStartRunningSnapshot,
        stopThenStartRunningSnapshot.watcher_lifecycle_token,
        false
      ) &&
      SmokeResourceSnapshotsEqual(
        &stopThenStartBeforeDrainSnapshot,
        &stopThenStartRunningSnapshot
      ) &&
      stopThenStartBaseContext->releaseCount == 1 &&
      stopThenStartBaseContext->callbackAfterReleaseCount == 0 &&
      stopThenStartLatestContext->releaseCount == 0 &&
      stopThenStartLatestContext->disabledReadinessCount == 1;
    paper_float_watchers_stop();
    PaperFloatNativeResourceSnapshot lifecycleRaceFinalSnapshot = {0};
    bool lifecycleRaceFinalSnapshotRead =
      paper_float_native_resource_snapshot(&lifecycleRaceFinalSnapshot) == 1;
    bool stopThenStartOrdered =
      latestStartSurvived &&
      lifecycleRaceFinalSnapshotRead &&
      SmokeStoppedSnapshotIsValid(
        &lifecycleRaceFinalSnapshot,
        lifecycleRaceFinalSnapshot.watcher_lifecycle_token
      ) &&
      lifecycleRaceFinalSnapshot.watcher_lifecycle_token >
      stopThenStartRunningSnapshot.watcher_lifecycle_token &&
      stopThenStartLatestContext->releaseCount == 1 &&
      stopThenStartLatestContext->callbackAfterReleaseCount == 0;

    // A detached stale start must release only its own context. It must not
    // advance the installed watcher's lifecycle token or disturb its sources.
    SmokeContext *staleStartBeforeLatestContext = &gLifecycleRaceContexts[3];
    SmokeContext *latestStartContext = &gLifecycleRaceContexts[4];
    staleStartBeforeLatestContext->selectionEnabled = false;
    latestStartContext->selectionEnabled = false;
    dispatch_semaphore_t staleStartSubmitted = dispatch_semaphore_create(0);
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
      paper_float_watchers_start(
        staleStartBeforeLatestContext,
        SmokeCallback,
        0
      );
      dispatch_semaphore_signal(staleStartSubmitted);
    });
    bool staleStartWasSubmitted = dispatch_semaphore_wait(
      staleStartSubmitted,
      dispatch_time(DISPATCH_TIME_NOW, (int64_t)NSEC_PER_SEC)
    ) == 0;
    paper_float_watchers_start(latestStartContext, SmokeCallback, 0);
    PaperFloatNativeResourceSnapshot beforeStaleStartDrainSnapshot = {0};
    bool beforeStaleStartDrainSnapshotRead =
      paper_float_native_resource_snapshot(&beforeStaleStartDrainSnapshot) == 1;
    CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, false);
    PaperFloatNativeResourceSnapshot afterStaleStartDrainSnapshot = {0};
    bool afterStaleStartDrainSnapshotRead =
      paper_float_native_resource_snapshot(&afterStaleStartDrainSnapshot) == 1;
    bool staleStartReleasePreservedLatest =
      staleStartWasSubmitted &&
      beforeStaleStartDrainSnapshotRead &&
      afterStaleStartDrainSnapshotRead &&
      SmokeRunningSnapshotIsValid(
        &beforeStaleStartDrainSnapshot,
        beforeStaleStartDrainSnapshot.watcher_lifecycle_token,
        false
      ) &&
      SmokeRunningSnapshotIsValid(
        &afterStaleStartDrainSnapshot,
        beforeStaleStartDrainSnapshot.watcher_lifecycle_token,
        false
      ) &&
      SmokeResourceSnapshotsEqual(
        &beforeStaleStartDrainSnapshot,
        &afterStaleStartDrainSnapshot
      ) &&
      staleStartBeforeLatestContext->releaseCount == 1 &&
      staleStartBeforeLatestContext->releaseLifecycle == 0 &&
      staleStartBeforeLatestContext->callbackAfterReleaseCount == 0 &&
      latestStartContext->releaseCount == 0 &&
      latestStartContext->disabledReadinessCount == 1;
    paper_float_watchers_stop();
    PaperFloatNativeResourceSnapshot lifecycleRaceLastFinalSnapshot = {0};
    bool lifecycleRaceLastFinalSnapshotRead =
      paper_float_native_resource_snapshot(&lifecycleRaceLastFinalSnapshot) == 1;
    bool startThenStartOrdered =
      staleStartReleasePreservedLatest &&
      lifecycleRaceLastFinalSnapshotRead &&
      SmokeStoppedSnapshotIsValid(
        &lifecycleRaceLastFinalSnapshot,
        lifecycleRaceLastFinalSnapshot.watcher_lifecycle_token
      ) &&
      latestStartContext->releaseCount == 1 &&
      latestStartContext->callbackAfterReleaseCount == 0;
    bool lifecycleRacesValid =
      startThenStopOrdered && stopThenStartOrdered && startThenStartOrdered;

    int totalReleases = 0;
    int callbackAfterReleaseCount = 0;
    long long firstLifecycle = 0;
    long long lastLifecycle = 0;
    bool lifecycleMonotonic = true;
    bool exactlyOnce = true;
    int disabledReadinessCount = 0;
    int unexpectedDisabledSelectionCallbackCount = 0;

    for (int index = 0; index < SmokeContextCount; index += 1) {
      SmokeContext *context = &gContexts[index];
      totalReleases += context->releaseCount;
      callbackAfterReleaseCount += context->callbackAfterReleaseCount;
      disabledReadinessCount += context->disabledReadinessCount;
      unexpectedDisabledSelectionCallbackCount +=
        context->unexpectedDisabledSelectionCallbackCount;
      exactlyOnce = exactlyOnce && context->releaseCount == 1;

      if (index == 0) {
        firstLifecycle = context->releaseLifecycle;
      } else if (context->releaseLifecycle <= lastLifecycle) {
        lifecycleMonotonic = false;
      }
      lastLifecycle = context->releaseLifecycle;
    }

    bool diagnosticsValid =
      gDiagnosticCount == SmokeDiagnosticCount &&
      paper_float_test_selection_retry_terminal(1) == 0 &&
      paper_float_test_selection_retry_terminal(2) == 0 &&
      paper_float_test_selection_retry_terminal(3) == 1 &&
      paper_float_test_observer_empty_terminal(1) == 0 &&
      paper_float_test_observer_empty_terminal(0) == 1 &&
      SmokeDiagnosticMatches(
        0,
        @"selection_read_found",
        @"test_success",
        @"true",
        2,
        4.5,
        @"dev.paperfloat.diagnostic-fixture",
        @"none",
        101,
        1,
        12.5,
        @"true",
        501
      ) &&
      SmokeDiagnosticMatches(
        1,
        @"selection_read_empty",
        @"test_empty",
        @"false",
        0,
        4.5,
        @"unknown_bundle_id",
        @"no_selected_text",
        102,
        2,
        305.0,
        @"false",
        502
      ) &&
      SmokeDiagnosticMatches(
        2,
        @"selection_read_empty",
        @"test_timeout",
        @"false",
        0,
        250.0,
        @"dev.paperfloat.diagnostic-fixture",
        @"ax_timeout",
        103,
        3,
        650.0,
        @"true",
        503
      );
    bool privacyValid = diagnosticsValid && SmokeDiagnosticsPreservePrivacy();
    bool tapInjectionValid =
      fixedTapInjectionsAccepted &&
      backgroundTapInjectionScheduled &&
      backgroundInjectionAccepted == 1 &&
      stoppedMouseTapInjectionRejected &&
      SmokeTapStatusObserved("key_tap_disabled_timeout_recovered") &&
      SmokeTapStatusObserved("key_tap_disabled_timeout_fallback") &&
      SmokeTapStatusObserved("key_tap_disabled_user_input_fallback") &&
      SmokeTapStatusObserved("selection_mouse_tap_disabled_timeout_recovered") &&
      SmokeTapStatusObserved("selection_mouse_tap_disabled_user_input_unavailable") &&
      SmokeTapStatusObserved("selection_mouse_tap_disabled_user_input_fallback_ax");
    bool resourcePeaksBounded =
      resourcePeaks.selection_observer_count <= 1 &&
      resourcePeaks.selection_observer_source_count <= 1 &&
      resourcePeaks.key_event_tap_count <= 1 &&
      resourcePeaks.key_event_tap_source_count <= 1 &&
      resourcePeaks.mouse_event_tap_count <= 1 &&
      resourcePeaks.mouse_event_tap_source_count <= 1 &&
      resourcePeaks.pasteboard_timer_count == 1 &&
      resourcePeaks.workspace_activation_observer_count == 1 &&
      resourcePeaks.callback_count == 1 &&
      resourcePeaks.context_count == 1 &&
      resourcePeaks.effective_source_set_count == 1 &&
      resourcePeaks.total_source_count <= 5;

    bool passed =
      exactlyOnce &&
      totalReleases == SmokeContextCount &&
      callbackAfterReleaseCount == 0 &&
      lifecycleMonotonic &&
      nullSnapshotRejected &&
      resourceFailures == 0 &&
      resourceSnapshotCount == SmokeContextCount + 1 &&
      selectionEnabledStartCount == 26 &&
      selectionDisabledStartCount == 25 &&
      disabledReadinessCount == selectionDisabledStartCount &&
      unexpectedDisabledSelectionCallbackCount == 0 &&
      resourcePeaksBounded &&
      lifecycleRacesValid &&
      diagnosticsValid &&
      privacyValid &&
      tapInjectionValid;

    printf(
      "NATIVE_WATCHER_CONTEXT_SMOKE %s starts=%d restarts=%d releases=%d "
      "callbackAfterRelease=%d lifecycleMonotonic=%s firstLifecycle=%lld lastLifecycle=%lld "
      "resourceSnapshots=%d resourceFailures=%d peakSourceSets=%u finalSources=%u "
      "toggles=%d selectionEnabledStarts=%d selectionDisabledStarts=%d "
      "disabledReadiness=%d unexpectedDisabledSelection=%d "
      "lifecycleRaces=%s raceFinalSources=%u "
      "diagnostics=%d diagnosticProtocol=%s tapInjection=%s privacy=%s\n",
      passed ? "PASS" : "FAIL",
      SmokeContextCount,
      SmokeRestartCount,
      totalReleases,
      callbackAfterReleaseCount,
      lifecycleMonotonic ? "true" : "false",
      firstLifecycle,
      lastLifecycle,
      resourceSnapshotCount,
      resourceFailures,
      resourcePeaks.effective_source_set_count,
      stoppedSnapshot.total_source_count,
      SmokeRestartCount,
      selectionEnabledStartCount,
      selectionDisabledStartCount,
      disabledReadinessCount,
      unexpectedDisabledSelectionCallbackCount,
      lifecycleRacesValid ? "true" : "false",
      lifecycleRaceLastFinalSnapshot.total_source_count,
      gDiagnosticCount,
      diagnosticsValid ? "true" : "false",
      tapInjectionValid ? "true" : "false",
      privacyValid ? "true" : "false"
    );

    return passed ? 0 : 1;
  }
}
