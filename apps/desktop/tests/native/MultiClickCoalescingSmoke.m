#import <CoreFoundation/CoreFoundation.h>
#import <Foundation/Foundation.h>

#include <dispatch/dispatch.h>
#include <math.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef void (*PaperFloatTestMultiClickCallback)(void *context, int eventType);
typedef void (*PaperFloatWatcherCallback)(
  void *context,
  int eventType,
  const char *text,
  double x,
  double y,
  long long delta,
  int targetPID
);

extern void paper_float_test_multi_click_schedule_double(
  void *context,
  PaperFloatTestMultiClickCallback callback,
  double quietWindow
);
extern void paper_float_test_multi_click_begin_mouse_down(void);
extern void paper_float_test_multi_click_commit_triple(
  void *context,
  PaperFloatTestMultiClickCallback callback
);
extern void paper_float_test_multi_click_observer_attempt(
  void *context,
  PaperFloatTestMultiClickCallback callback
);
extern void paper_float_test_multi_click_invalidate(void);
extern int paper_float_test_multi_click_pending(void);
extern double paper_float_test_multi_click_quiet_window_for_interval(
  double systemInterval
);
extern double paper_float_test_configured_multi_click_quiet_window(void);
extern void paper_float_test_set_multi_click_system_interval(double systemInterval);
extern double paper_float_multi_click_quiet_window_seconds(void);
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

extern int paper_float_native_resource_snapshot(
  PaperFloatNativeResourceSnapshot *snapshot
);

enum {
  SmokeSequenceCapacity = 8
};

typedef struct {
  int doubleCount;
  int tripleCount;
  int observerCount;
  int unexpectedCount;
  int callbackCount;
  int sequence[SmokeSequenceCapacity];
} SmokeContext;

typedef struct {
  int invalidStatusCount;
  int unexpectedSelectionStatusCount;
} InvalidStartupContext;

static int gChecks = 0;
static int gFailures = 0;

static void SmokeCheck(bool condition, const char *expression, int line) {
  gChecks += 1;
  if (condition) {
    return;
  }
  gFailures += 1;
  fprintf(stderr, "check failed at line %d: %s\n", line, expression);
}

#define SMOKE_CHECK(condition) SmokeCheck((condition), #condition, __LINE__)

static bool SmokeNearlyEqual(double lhs, double rhs) {
  return fabs(lhs - rhs) <= 1e-9;
}

static void SmokeCallback(void *rawContext, int eventType) {
  SmokeContext *context = rawContext;
  if (context == NULL) {
    return;
  }
  if (context->callbackCount < SmokeSequenceCapacity) {
    context->sequence[context->callbackCount] = eventType;
  }
  context->callbackCount += 1;
  switch (eventType) {
    case 2:
      context->doubleCount += 1;
      break;
    case 3:
      context->tripleCount += 1;
      break;
    case 4:
      context->observerCount += 1;
      break;
    default:
      context->unexpectedCount += 1;
      break;
  }
}

static void InvalidStartupCallback(
  void *rawContext,
  int eventType,
  const char *text,
  double x,
  double y,
  long long delta,
  int targetPID
) {
  (void)x;
  (void)y;
  (void)delta;
  (void)targetPID;
  InvalidStartupContext *context = rawContext;
  if (context == NULL || eventType != 2) {
    return;
  }
  if (text != NULL &&
      strcmp(text, "selection_multi_click_quiet_window_invalid") == 0) {
    context->invalidStatusCount += 1;
  } else {
    context->unexpectedSelectionStatusCount += 1;
  }
}

static bool SmokeSelectionSourcesAreReleased(
  const PaperFloatNativeResourceSnapshot *snapshot
) {
  return snapshot->version == 1 &&
    snapshot->selection_observer_count == 0 &&
    snapshot->selection_observer_source_count == 0 &&
    snapshot->mouse_event_tap_count == 0 &&
    snapshot->mouse_event_tap_source_count == 0 &&
    snapshot->workspace_activation_observer_count == 0;
}

static void SmokePumpMainRunLoop(double seconds) {
  NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:seconds];
  while (true) {
    double remaining = deadline.timeIntervalSinceNow;
    if (remaining <= 0) {
      break;
    }
    CFTimeInterval slice = remaining < 0.01 ? remaining : 0.01;
    (void)CFRunLoopRunInMode(kCFRunLoopDefaultMode, slice, true);
  }
}

int main(void) {
  @autoreleasepool {
    paper_float_test_set_multi_click_system_interval(0.50);
    paper_float_test_multi_click_invalidate();
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);

    SmokeContext standalone = {0};
    paper_float_test_multi_click_schedule_double(
      &standalone,
      SmokeCallback,
      0.08
    );
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 1);
    SmokePumpMainRunLoop(0.03);
    SMOKE_CHECK(standalone.callbackCount == 0);
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 1);
    SmokePumpMainRunLoop(0.10);
    SMOKE_CHECK(standalone.callbackCount == 1);
    SMOKE_CHECK(standalone.doubleCount == 1);
    SMOKE_CHECK(standalone.tripleCount == 0);
    SMOKE_CHECK(standalone.observerCount == 0);
    SMOKE_CHECK(standalone.unexpectedCount == 0);
    SMOKE_CHECK(standalone.sequence[0] == 2);
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);
    SmokePumpMainRunLoop(0.05);
    SMOKE_CHECK(standalone.callbackCount == 1);

    SmokeContext lateTriple = {0};
    paper_float_test_multi_click_schedule_double(
      &lateTriple,
      SmokeCallback,
      0.25
    );
    SmokePumpMainRunLoop(0.12);
    SMOKE_CHECK(lateTriple.callbackCount == 0);
    SMOKE_CHECK(lateTriple.doubleCount == 0);
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 1);
    paper_float_test_multi_click_observer_attempt(
      &lateTriple,
      SmokeCallback
    );
    SMOKE_CHECK(lateTriple.callbackCount == 0);
    SMOKE_CHECK(lateTriple.observerCount == 0);
    paper_float_test_multi_click_begin_mouse_down();
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);
    paper_float_test_multi_click_commit_triple(
      &lateTriple,
      SmokeCallback
    );
    SMOKE_CHECK(lateTriple.callbackCount == 1);
    SMOKE_CHECK(lateTriple.doubleCount == 0);
    SMOKE_CHECK(lateTriple.tripleCount == 1);
    SMOKE_CHECK(lateTriple.observerCount == 0);
    SMOKE_CHECK(lateTriple.sequence[0] == 3);
    SmokePumpMainRunLoop(0.18);
    SMOKE_CHECK(lateTriple.callbackCount == 1);
    SMOKE_CHECK(lateTriple.doubleCount == 0);
    SMOKE_CHECK(lateTriple.tripleCount == 1);
    SMOKE_CHECK(lateTriple.observerCount == 0);

    SmokeContext invalidated = {0};
    paper_float_test_multi_click_schedule_double(
      &invalidated,
      SmokeCallback,
      0.07
    );
    SmokePumpMainRunLoop(0.02);
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 1);
    paper_float_test_multi_click_invalidate();
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);
    SmokePumpMainRunLoop(0.08);
    SMOKE_CHECK(invalidated.callbackCount == 0);

    SmokeContext stopped = {0};
    SmokeContext restarted = {0};
    paper_float_test_multi_click_schedule_double(
      &stopped,
      SmokeCallback,
      0.09
    );
    SmokePumpMainRunLoop(0.02);
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 1);
    paper_float_watchers_stop();
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);
    SmokePumpMainRunLoop(0.10);
    SMOKE_CHECK(stopped.callbackCount == 0);
    paper_float_test_multi_click_schedule_double(
      &restarted,
      SmokeCallback,
      0.04
    );
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 1);
    SmokePumpMainRunLoop(0.08);
    SMOKE_CHECK(stopped.callbackCount == 0);
    SMOKE_CHECK(restarted.callbackCount == 1);
    SMOKE_CHECK(restarted.doubleCount == 1);
    SMOKE_CHECK(restarted.unexpectedCount == 0);
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);

    SmokeContext superseded = {0};
    SmokeContext replacement = {0};
    paper_float_test_multi_click_schedule_double(
      &superseded,
      SmokeCallback,
      0.11
    );
    SmokePumpMainRunLoop(0.02);
    paper_float_test_multi_click_schedule_double(
      &replacement,
      SmokeCallback,
      0.04
    );
    SmokePumpMainRunLoop(0.07);
    SMOKE_CHECK(superseded.callbackCount == 0);
    SMOKE_CHECK(replacement.callbackCount == 1);
    SMOKE_CHECK(replacement.doubleCount == 1);
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);
    SmokePumpMainRunLoop(0.07);
    SMOKE_CHECK(superseded.callbackCount == 0);
    SMOKE_CHECK(replacement.callbackCount == 1);

    SmokeContext invalidDelay = {0};
    paper_float_test_multi_click_schedule_double(
      &invalidDelay,
      SmokeCallback,
      -1.0
    );
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);
    SmokePumpMainRunLoop(0.02);
    SMOKE_CHECK(invalidDelay.callbackCount == 0);

    paper_float_test_multi_click_schedule_double(&invalidDelay, SmokeCallback, 0);
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);
    paper_float_test_multi_click_schedule_double(&invalidDelay, SmokeCallback, NAN);
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);
    paper_float_test_multi_click_schedule_double(&invalidDelay, SmokeCallback, INFINITY);
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);
    paper_float_test_multi_click_schedule_double(&invalidDelay, SmokeCallback, 2.000001);
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);

    double fastQuiet = paper_float_test_multi_click_quiet_window_for_interval(0.20);
    double zeroQuiet = paper_float_test_multi_click_quiet_window_for_interval(0);
    double negativeQuiet = paper_float_test_multi_click_quiet_window_for_interval(-0.1);
    double nanQuiet = paper_float_test_multi_click_quiet_window_for_interval(NAN);
    double infiniteQuiet = paper_float_test_multi_click_quiet_window_for_interval(INFINITY);
    double maximumQuiet = paper_float_test_multi_click_quiet_window_for_interval(1.97);
    double oversizedQuiet = paper_float_test_multi_click_quiet_window_for_interval(1.970001);
    double configuredQuiet = paper_float_test_configured_multi_click_quiet_window();
    SMOKE_CHECK(SmokeNearlyEqual(fastQuiet, 0.23));
    SMOKE_CHECK(SmokeNearlyEqual(zeroQuiet, 0));
    SMOKE_CHECK(SmokeNearlyEqual(negativeQuiet, 0));
    SMOKE_CHECK(SmokeNearlyEqual(nanQuiet, 0));
    SMOKE_CHECK(SmokeNearlyEqual(infiniteQuiet, 0));
    SMOKE_CHECK(SmokeNearlyEqual(maximumQuiet, 2.0));
    SMOKE_CHECK(SmokeNearlyEqual(oversizedQuiet, 0));
    SMOKE_CHECK(isfinite(configuredQuiet));
    SMOKE_CHECK(configuredQuiet > 0.03);
    SMOKE_CHECK(configuredQuiet <= 2.0);

    // The exported production value is the state machine's captured value,
    // not a fresh read of the system preference. Re-evaluating a different
    // system interval therefore cannot drift an active runtime value.
    paper_float_test_set_multi_click_system_interval(0.50);
    paper_float_watchers_start(NULL, NULL, 1);
    SMOKE_CHECK(SmokeNearlyEqual(
      paper_float_multi_click_quiet_window_seconds(),
      0.53
    ));
    paper_float_test_set_multi_click_system_interval(0.20);
    SMOKE_CHECK(SmokeNearlyEqual(
      paper_float_test_multi_click_quiet_window_for_interval(0.20),
      0.23
    ));
    SMOKE_CHECK(SmokeNearlyEqual(
      paper_float_multi_click_quiet_window_seconds(),
      0.53
    ));
    paper_float_watchers_stop();
    SMOKE_CHECK(SmokeNearlyEqual(
      paper_float_multi_click_quiet_window_seconds(),
      0
    ));
    paper_float_watchers_start(NULL, NULL, 1);
    SMOKE_CHECK(SmokeNearlyEqual(
      paper_float_multi_click_quiet_window_seconds(),
      0.23
    ));
    paper_float_watchers_stop();
    SMOKE_CHECK(SmokeNearlyEqual(
      paper_float_multi_click_quiet_window_seconds(),
      0
    ));

    // Off-main watcher starts enqueue their main block asynchronously. The
    // production Q getter's synchronous main-queue read must be FIFO-ordered
    // behind that start, making it a completion barrier instead of a stale
    // read from the previous lifecycle.
    paper_float_test_set_multi_click_system_interval(0.45);
    __block double backgroundCapturedQuiet = 0;
    dispatch_semaphore_t backgroundCaptureDone = dispatch_semaphore_create(0);
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
      paper_float_watchers_start(NULL, NULL, 1);
      backgroundCapturedQuiet = paper_float_multi_click_quiet_window_seconds();
      dispatch_semaphore_signal(backgroundCaptureDone);
    });
    bool backgroundCaptureCompleted = false;
    NSDate *backgroundCaptureDeadline = [NSDate dateWithTimeIntervalSinceNow:1.0];
    while (!backgroundCaptureCompleted &&
           backgroundCaptureDeadline.timeIntervalSinceNow > 0) {
      backgroundCaptureCompleted = dispatch_semaphore_wait(
        backgroundCaptureDone,
        DISPATCH_TIME_NOW
      ) == 0;
      if (!backgroundCaptureCompleted) {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.01, true);
      }
    }
    SMOKE_CHECK(backgroundCaptureCompleted);
    SMOKE_CHECK(SmokeNearlyEqual(backgroundCapturedQuiet, 0.48));
    paper_float_watchers_stop();

    const double invalidSystemIntervals[] = {0, -0.1, NAN, INFINITY, 1.970001};
    for (size_t index = 0;
         index < sizeof(invalidSystemIntervals) / sizeof(invalidSystemIntervals[0]);
         index += 1) {
      InvalidStartupContext invalidStartup = {0};
      paper_float_test_set_multi_click_system_interval(
        invalidSystemIntervals[index]
      );
      paper_float_watchers_start(
        &invalidStartup,
        InvalidStartupCallback,
        1
      );
      PaperFloatNativeResourceSnapshot snapshot = {0};
      SMOKE_CHECK(SmokeNearlyEqual(
        paper_float_multi_click_quiet_window_seconds(),
        0
      ));
      SMOKE_CHECK(paper_float_native_resource_snapshot(&snapshot) == 1);
      SMOKE_CHECK(SmokeSelectionSourcesAreReleased(&snapshot));
      SMOKE_CHECK(invalidStartup.invalidStatusCount == 1);
      SMOKE_CHECK(invalidStartup.unexpectedSelectionStatusCount == 0);
      paper_float_watchers_stop();
    }

    paper_float_test_multi_click_invalidate();
    SMOKE_CHECK(paper_float_test_multi_click_pending() == 0);

    printf(
      "MULTI_CLICK_COALESCING_SMOKE %s checks=%d failures=%d\n",
      gFailures == 0 ? "PASS" : "FAIL",
      gChecks,
      gFailures
    );
  }
  return gFailures == 0 ? 0 : 1;
}
