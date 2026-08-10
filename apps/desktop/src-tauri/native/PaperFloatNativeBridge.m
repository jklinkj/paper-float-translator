#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <LocalAuthentication/LocalAuthentication.h>
#import <Security/Security.h>
#import <dispatch/dispatch.h>
#import <limits.h>
#import <math.h>
#import <os/lock.h>
#import <stdint.h>
#import <stdlib.h>
#import <string.h>
#import <sys/types.h>
#import <unistd.h>

typedef void (*PaperFloatWatcherCallback)(
  void *context,
  int event_type,
  const char *text,
  double x,
  double y,
  long long delta,
  int target_pid
);

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

typedef struct {
  uint32_t version;
  uint32_t display_id;
  double x;
  double y;
  double width;
  double height;
} PaperFloatNativeVisibleWorkArea;

enum {
  PaperFloatEventPasteboardStatus = 1,
  PaperFloatEventSelectionStatus = 2,
  PaperFloatEventPasteboardChange = 3,
  PaperFloatEventDoubleCopy = 4,
  PaperFloatEventMouseDown = 5,
  PaperFloatEventSelection = 6,
  PaperFloatEventWatcherContextRelease = 7,
  PaperFloatEventPopupEscape = 8,
  PaperFloatEventPopupFocusRequest = 9
};

@interface PFSelectionReadContext : NSObject
@property(nonatomic, assign) long long generation;
@property(nonatomic, assign) pid_t targetPID;
@property(nonatomic, assign) CGPoint anchor;
@property(nonatomic, assign) NSTimeInterval triggerUptime;
@property(nonatomic, copy) NSString *reason;
@property(nonatomic, assign) BOOL emitEmptyStatus;
@property(nonatomic, assign) CGWindowID targetWindowID;
@property(nonatomic, strong) id targetElement;
@property(nonatomic, strong) id secondaryPreferredElement;
@property(nonatomic, strong) id targetWindow;
@property(nonatomic, strong) id anchorElement;
@property(nonatomic, assign) BOOL hasPreferredElement;
@property(nonatomic, assign) BOOL primaryPreferredElementRequiresAnchorProof;
@property(nonatomic, assign) BOOL hasSecondaryPreferredElement;
@property(nonatomic, assign) BOOL requiresAnchorWindowRevalidation;
@property(nonatomic, assign) BOOL requiresDirectSelectionChangeEvidence;
@property(nonatomic, assign) BOOL directSelectionChangeObserved;
@property(nonatomic, assign) BOOL directSelectionBaselineCaptured;
@property(nonatomic, copy) NSArray *directSelectionBaselineElements;
@property(nonatomic, copy) NSArray *directSelectionBaselineTexts;
@property(nonatomic, copy) NSString *directSelectionBaselineFailureReason;
@property(nonatomic, copy) NSString *suppressedDuplicateText;
@end

@implementation PFSelectionReadContext
@end

static void *gContext = NULL;
static PaperFloatWatcherCallback gCallback = NULL;

static CFMachPortRef gKeyEventTap = NULL;
static CFMachPortRef gMouseEventTap = NULL;
static CFRunLoopSourceRef gKeyRunLoopSource = NULL;
static CFRunLoopSourceRef gMouseRunLoopSource = NULL;
static NSTimer *gPasteboardTimer = nil;
static BOOL gKeyEventTapHealthy = NO;
static BOOL gMouseEventTapHealthy = NO;
static BOOL gSelectionWatcherEnabled = NO;
static BOOL gSelectionAXMessagingTimeoutHealthy = YES;
static long long gWatcherLifecycleToken = 0;
static os_unfair_lock gWatcherSubmissionLock = OS_UNFAIR_LOCK_INIT;
static NSObject *gWatcherSubmissionToken = nil;

static NSInteger gPasteboardLastChangeCount = 0;
static NSTimeInterval gPasteboardLastCopyShortcutAt = 0;
static NSTimeInterval gPasteboardLastFallbackCopyAt = 0;
static NSString *gPasteboardLastFallbackText = nil;
static const NSTimeInterval kDoubleCopyWindow = 0.9;
static const NSTimeInterval kPasteboardReadDelay = 0.08;
static const NSTimeInterval kPasteboardPollInterval = 0.05;
static BOOL gSelectionDidDrag = NO;
static BOOL gSelectionHasMouseDownLocation = NO;
static BOOL gSelectionIgnoreCurrentMouseGesture = NO;
static CGPoint gSelectionMouseDownLocation = {0, 0};
static pid_t gSelectionMouseDownTargetPID = 0;
static BOOL gSelectionMouseDownTargetPIDAnchored = NO;
static CGWindowID gSelectionMouseDownWindowID = kCGNullWindowID;
static long long gSelectionMouseGeneration = 0;
static NSTimeInterval gSelectionLastEmptyStatusAt = 0;
static NSString *gSelectionLastEmittedText = nil;
static NSTimeInterval gSelectionLastEmittedAt = 0;
static long long gSelectionLastEmittedGeneration = 0;
static AXObserverRef gSelectionObserver = NULL;
static CFRunLoopSourceRef gSelectionObserverRunLoopSource = NULL;
static id gSelectionObserverAppElement = nil;
static id gSelectionObserverSelectionElement = nil;
static BOOL gSelectionObserverFocusNotificationRegistered = NO;
static BOOL gSelectionObserverWindowNotificationRegistered = NO;
static BOOL gSelectionObserverSelectionNotificationReady = NO;
static pid_t gSelectionObserverPID = 0;
static id gWorkspaceActivationObserver = nil;
static long long gSelectionGeneration = 0;
static long long gSelectionReadToken = 0;
static long long gSelectionCompletedGeneration = 0;
static long long gSelectionObserverDebounceToken = 0;
static long long gSelectionPendingObserverGeneration = 0;
static pid_t gSelectionPendingObserverPID = 0;
static CGPoint gSelectionPendingObserverAnchor = {0, 0};
static NSTimeInterval gSelectionLastObserverNotificationAt = 0;
static long long gSelectionLastMouseUpGeneration = 0;
static pid_t gSelectionLastMouseUpPID = 0;
static CGPoint gSelectionLastMouseUpAnchor = {0, 0};
static NSTimeInterval gSelectionLastMouseUpAt = 0;
static CGWindowID gSelectionLastMouseUpWindowID = kCGNullWindowID;
static BOOL gSelectionLastMouseGestureInvalid = NO;
static PFSelectionReadContext *gSelectionLastMouseContext = nil;
static BOOL gSelectionMouseObservedSelectionChange = NO;
static long long gSelectionLastMouseSelectionChangeGeneration = 0;
static long long gSelectionDirectBaselineGeneration = 0;
static BOOL gSelectionDirectBaselineCaptured = NO;
static NSArray *gSelectionDirectBaselineElements = nil;
static NSArray *gSelectionDirectBaselineTexts = nil;
static NSString *gSelectionDirectBaselineFailureReason = nil;
static long long gSelectionProvisionalDoubleToken = 0;
static long long gSelectionProvisionalDoubleGeneration = 0;
static BOOL gSelectionProvisionalDoublePending = NO;
// Captured once when the production selection watcher starts. This value is
// the state machine's source of truth; acceptance diagnostics must read this
// captured value instead of querying the mutable system preference again.
static NSTimeInterval gSelectionMultiClickQuietWindow = 0;
static const CGFloat kSelectionDragDistanceThreshold = 4.0;
static const NSTimeInterval kSelectionObserverDebounceDelay = 0.15;
static const NSTimeInterval kSelectionObserverGroupingWindow = 0.25;
static const NSTimeInterval kSelectionMouseObserverCorrelationWindow = 0.80;
static const NSTimeInterval kSelectionSameGenerationDedupWindow = 1.0;
static const NSTimeInterval kSelectionFrozenReadLifetime = 1.5;
static const NSTimeInterval kSelectionMultiClickDeliveryMargin = 0.03;
static const NSTimeInterval kSelectionMultiClickMaximumQuietWindow = 2.0;
#if defined(PAPER_FLOAT_NATIVE_TESTING)
static BOOL gPaperFloatTestMultiClickSystemIntervalOverrideEnabled = NO;
static NSTimeInterval gPaperFloatTestMultiClickSystemIntervalOverride = 0;
#endif
static const NSTimeInterval kSelectionProvisionalDoubleSecondRetryDelay = 0.10;
static const NSTimeInterval kSelectionProvisionalDoubleFinalRetryDelay = 0.25;
static const float kAXMessagingTimeoutSeconds = 0.25f;
static const float kSelectionBaselineAXTimeoutSeconds = 0.003f;
static const NSTimeInterval kSelectionBaselineWallClockBudgetSeconds = 0.045;
static const NSTimeInterval kSelectionBaselineSupplementalMinimumBudgetSeconds = 0.009;
static const NSInteger kSelectionBaselineMaximumDepth = 16;
static const NSInteger kSelectionBaselineSupplementalMaximumDepth = 2;
static AXError gSelectionLastAXReadError = kAXErrorSuccess;

static void PFRunOnMainQueue(dispatch_block_t block);
static BOOL PFElementsEqual(id first, id second);
static void PFEmitSelectionCapabilityStatus(void);

static BOOL PFSelectionAttributeErrorIsSafeAbsence(AXError error) {
  return error == kAXErrorNoValue || error == kAXErrorAttributeUnsupported;
}

static BOOL PFPreferredSelectionShouldContinueToParent(
  BOOL foundText,
  BOOL readFailed
) {
  return !foundText && !readFailed;
}

static BOOL PFFocusedPreferredElementCanBeUsed(
  BOOL belongsToTargetPID,
  BOOL isWithinFrozenWindow,
  BOOL isFrozenWindow,
  BOOL duplicatesPrimaryAncestry,
  BOOL sharesControlSubtree,
  BOOL frameContainsAnchor
) {
  return belongsToTargetPID &&
    isWithinFrozenWindow &&
    !isFrozenWindow &&
    !duplicatesPrimaryAncestry &&
    (sharesControlSubtree || frameContainsAnchor);
}

static BOOL PFMousePrimaryPreferredElementRequiresAnchorProof(
  BOOL anchorElementValid,
  BOOL focusedElementSafe,
  BOOL focusedElementCoveredByPrimaryAncestry
) {
  // When the hit-tested element is valid, its verified ancestry already ties
  // every preferred parent to the mouse-up anchor. Keep ordinary double direct
  // reads available there because Preview/PDFKit may expose AXSelectedText but
  // no selected ranges or bounds. Only a focused element promoted without a
  // hit-tested anchor element must prove the selection through range bounds.
  (void)focusedElementCoveredByPrimaryAncestry;
  return !anchorElementValid && focusedElementSafe;
}

static BOOL PFAnchorIdentityRevalidationPasses(
  BOOL withinFrozenWindow,
  BOOL frozenAnchorAvailable,
  BOOL exactElementMatch,
  BOOL nearbyAncestryMatch
) {
  return withinFrozenWindow &&
    (!frozenAnchorAvailable || exactElementMatch || nearbyAncestryMatch);
}

typedef NS_ENUM(NSInteger, PFObserverMouseCorrelationDecision) {
  PFObserverMouseCorrelationIndependent = 0,
  PFObserverMouseCorrelationReusePendingMouse = 1
};

static PFObserverMouseCorrelationDecision PFResolveObserverMouseCorrelation(
  BOOL recentMouseGesture,
  BOOL mouseGenerationCompleted,
  BOOL mouseGestureInvalid,
  BOOL samePID
) {
  return recentMouseGesture &&
      !mouseGenerationCompleted &&
      !mouseGestureInvalid &&
      samePID
    ? PFObserverMouseCorrelationReusePendingMouse
    : PFObserverMouseCorrelationIndependent;
}

static BOOL PFCompletedMouseObserverEchoNeedsTextCheck(
  BOOL recentMouseGesture,
  BOOL mouseGenerationCompleted,
  BOOL samePID,
  BOOL completedMouseTextAvailable
) {
  return recentMouseGesture &&
    mouseGenerationCompleted &&
    samePID &&
    completedMouseTextAvailable;
}

static BOOL PFStableSelectedRangeReadCanEmit(
  BOOL identityValidBeforeRead,
  BOOL allRangeTextRead,
  BOOL identityValidAfterRead
) {
  return identityValidBeforeRead && allRangeTextRead && identityValidAfterRead;
}

typedef NS_ENUM(NSInteger, PFDirectSelectedTextReadStatus) {
  PFDirectSelectedTextReadSafeAbsence = 0,
  PFDirectSelectedTextReadText = 1,
  PFDirectSelectedTextReadFailure = 2
};

typedef NS_ENUM(NSInteger, PFPreferredDirectReadDecision) {
  PFPreferredDirectReadTryRanges = 0,
  PFPreferredDirectReadEmit = 1,
  PFPreferredDirectReadFailClosed = 2
};

static PFPreferredDirectReadDecision PFResolvePreferredDirectReadDecision(
  PFDirectSelectedTextReadStatus firstStatus,
  PFDirectSelectedTextReadStatus secondStatus,
  BOOL textsEqual
) {
  if (firstStatus == PFDirectSelectedTextReadSafeAbsence) {
    return PFPreferredDirectReadTryRanges;
  }
  if (firstStatus != PFDirectSelectedTextReadText) {
    return PFPreferredDirectReadFailClosed;
  }
  return secondStatus == PFDirectSelectedTextReadText && textsEqual
    ? PFPreferredDirectReadEmit
    : PFPreferredDirectReadFailClosed;
}

static BOOL PFDirectSelectedTextMayEmit(
  PFPreferredDirectReadDecision decision,
  BOOL requiresSelectionChangeEvidence,
  BOOL selectionChangeObserved,
  BOOL baselineComparable,
  BOOL textMatchesBaseline
) {
  return decision == PFPreferredDirectReadEmit &&
    (!requiresSelectionChangeEvidence ||
     selectionChangeObserved ||
     (baselineComparable && !textMatchesBaseline));
}

static NSTimeInterval PFSelectionBaselineMaximumAXBudgetSeconds(void) {
  return kSelectionBaselineWallClockBudgetSeconds;
}

static PFDirectSelectedTextReadStatus PFResolveAnchoredFocusedDirectReadStatus(
  BOOL selectedRangesStable,
  BOOL anchorMatchesSelectedRangeBounds,
  PFDirectSelectedTextReadStatus firstStatus,
  PFDirectSelectedTextReadStatus secondStatus,
  BOOL textsEqual
) {
  if (!anchorMatchesSelectedRangeBounds) {
    return PFDirectSelectedTextReadSafeAbsence;
  }
  if (!selectedRangesStable) {
    return PFDirectSelectedTextReadFailure;
  }
  PFPreferredDirectReadDecision decision = PFResolvePreferredDirectReadDecision(
    firstStatus,
    secondStatus,
    textsEqual
  );
  if (decision == PFPreferredDirectReadEmit) {
    return PFDirectSelectedTextReadText;
  }
  return firstStatus == PFDirectSelectedTextReadSafeAbsence
    ? PFDirectSelectedTextReadSafeAbsence
    : PFDirectSelectedTextReadFailure;
}

static void PFRecordSelectionAXReadError(AXError error) {
  if (error == kAXErrorSuccess || PFSelectionAttributeErrorIsSafeAbsence(error)) {
    return;
  }
  if (gSelectionLastAXReadError != kAXErrorCannotComplete ||
      error == kAXErrorCannotComplete) {
    gSelectionLastAXReadError = error;
  }
}

static NSTimeInterval PFMonotonicUptime(void) {
  return NSProcessInfo.processInfo.systemUptime;
}

static NSTimeInterval PFMultiClickQuietWindowForSystemInterval(
  NSTimeInterval systemInterval
) {
  if (!isfinite(systemInterval) ||
      systemInterval <= 0 ||
      systemInterval >
        kSelectionMultiClickMaximumQuietWindow - kSelectionMultiClickDeliveryMargin) {
    return 0;
  }
  NSTimeInterval quietWindow = systemInterval + kSelectionMultiClickDeliveryMargin;
  return isfinite(quietWindow) &&
      quietWindow > 0 &&
      quietWindow <= kSelectionMultiClickMaximumQuietWindow
    ? quietWindow
    : 0;
}

static NSTimeInterval PFConfiguredMultiClickQuietWindow(void) {
#if defined(PAPER_FLOAT_NATIVE_TESTING)
  if (gPaperFloatTestMultiClickSystemIntervalOverrideEnabled) {
    return PFMultiClickQuietWindowForSystemInterval(
      gPaperFloatTestMultiClickSystemIntervalOverride
    );
  }
#endif
  return PFMultiClickQuietWindowForSystemInterval(NSEvent.doubleClickInterval);
}

static NSTimeInterval PFElapsedMillisecondsSince(NSTimeInterval startedAt) {
  NSTimeInterval elapsed = (PFMonotonicUptime() - startedAt) * 1000.0;
  return isfinite(elapsed) && elapsed > 0 ? elapsed : 0;
}

static long long PFElapsedMicrosecondsSince(NSTimeInterval startedAt) {
  NSTimeInterval elapsed = (PFMonotonicUptime() - startedAt) * 1000000.0;
  if (!isfinite(elapsed) || elapsed <= 0) {
    return 0;
  }
  if (elapsed >= (NSTimeInterval)LLONG_MAX) {
    return LLONG_MAX;
  }
  long long rounded = llround(elapsed);
  return rounded > 0 ? rounded : 1;
}

static void PFEmit(int eventType, NSString *text, double x, double y, long long delta) {
  if (gCallback == NULL) {
    return;
  }

  const char *rawText = text == nil ? "" : [text UTF8String];
  gCallback(gContext, eventType, rawText, x, y, delta, 0);
}

static void PFEmitForTargetPID(
  int eventType,
  NSString *text,
  double x,
  double y,
  long long delta,
  pid_t targetPID
) {
  if (gCallback == NULL) {
    return;
  }

  const char *rawText = text == nil ? "" : [text UTF8String];
  gCallback(gContext, eventType, rawText, x, y, delta, (int)targetPID);
}

static void PFEmitDeferred(int eventType, NSString *text, double x, double y, long long delta) {
  long long lifecycleToken = gWatcherLifecycleToken;
  NSString *payload = [text copy];
  dispatch_async(dispatch_get_main_queue(), ^{
    if (lifecycleToken == gWatcherLifecycleToken) {
      PFEmit(eventType, payload, x, y, delta);
    }
  });
}

static void PFEmitTapStateDeferred(
  int eventType,
  NSString *text,
  NSTimeInterval disabledAtUptime
) {
  long long lifecycleToken = gWatcherLifecycleToken;
  NSString *payload = [text copy];
  dispatch_async(dispatch_get_main_queue(), ^{
    if (lifecycleToken == gWatcherLifecycleToken) {
      PFEmit(
        eventType,
        payload,
        0,
        0,
        PFElapsedMicrosecondsSince(disabledAtUptime)
      );
    }
  });
}

static BOOL PFSetGlobalAXMessagingTimeout(float timeoutInSeconds) {
  AXUIElementRef systemWide = AXUIElementCreateSystemWide();
  if (systemWide == NULL) {
    return NO;
  }
  AXError error = AXUIElementSetMessagingTimeout(systemWide, timeoutInSeconds);
  CFRelease(systemWide);
  return error == kAXErrorSuccess;
}

int paper_float_accessibility_is_trusted(void) {
  return AXIsProcessTrusted() ? 1 : 0;
}

int paper_float_listen_event_access_preflight(void) {
  if (@available(macOS 10.15, *)) {
    return CGPreflightListenEventAccess() ? 1 : 0;
  }
  return -1;
}

int paper_float_keychain_contains(const char *service, const char *account) {
  if (service == NULL || account == NULL) {
    return (int)errSecParam;
  }

  @autoreleasepool {
    NSString *serviceValue = [NSString stringWithUTF8String:service];
    NSString *accountValue = [NSString stringWithUTF8String:account];
    if (serviceValue == nil || accountValue == nil) {
      return (int)errSecParam;
    }

    LAContext *authenticationContext = [[LAContext alloc] init];
    authenticationContext.interactionNotAllowed = YES;
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    id legacyAuthenticationUIFail = (__bridge id)kSecUseAuthenticationUIFail;
#pragma clang diagnostic pop
    NSDictionary *query = @{
      (__bridge id)kSecClass: (__bridge id)kSecClassGenericPassword,
      (__bridge id)kSecAttrService: serviceValue,
      (__bridge id)kSecAttrAccount: accountValue,
      // Settings only need to know whether a Key exists. Returning attributes
      // keeps the encrypted secret out of the startup path; secret-data
      // retrieval is deliberately reserved for an actual translation request.
      (__bridge id)kSecReturnAttributes: @YES,
      (__bridge id)kSecMatchLimit: (__bridge id)kSecMatchLimitOne,
      (__bridge id)kSecUseAuthenticationContext: authenticationContext,
      (__bridge id)kSecUseAuthenticationUI: legacyAuthenticationUIFail
    };
    CFTypeRef result = NULL;
    OSStatus status = SecItemCopyMatching((__bridge CFDictionaryRef)query, &result);
    if (result != NULL) {
      CFRelease(result);
    }
    if (status == errSecItemNotFound) {
      return 1;
    }
    return (int)status;
  }
}

int paper_float_keychain_get(const char *service, const char *account, char **outValue) {
  if (service == NULL || account == NULL || outValue == NULL) {
    return (int)errSecParam;
  }
  *outValue = NULL;

  @autoreleasepool {
    NSString *serviceValue = [NSString stringWithUTF8String:service];
    NSString *accountValue = [NSString stringWithUTF8String:account];
    if (serviceValue == nil || accountValue == nil) {
      return (int)errSecParam;
    }

    LAContext *authenticationContext = [[LAContext alloc] init];
    authenticationContext.interactionNotAllowed = YES;
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    // LAContext.interactionNotAllowed is documented as equivalent to
    // kSecUseNoAuthenticationUI, which macOS explicitly limits to the Data
    // Protection keychain. Existing Paper Float items live in the legacy
    // login keychain, so retain the legacy UI-fail control as well. The
    // Security headers recommend combining the context and UI attributes when
    // an operation must never present authentication UI.
    id legacyAuthenticationUIFail = (__bridge id)kSecUseAuthenticationUIFail;
#pragma clang diagnostic pop
    NSDictionary *query = @{
      (__bridge id)kSecClass: (__bridge id)kSecClassGenericPassword,
      (__bridge id)kSecAttrService: serviceValue,
      (__bridge id)kSecAttrAccount: accountValue,
      (__bridge id)kSecReturnData: @YES,
      (__bridge id)kSecMatchLimit: (__bridge id)kSecMatchLimitOne,
      // Settings/status refreshes are background reads. They must fail with an
      // OSStatus instead of presenting a login-keychain authorization dialog.
      // User-authorized writes remain confined to paper_float_keychain_set.
      (__bridge id)kSecUseAuthenticationContext: authenticationContext,
      (__bridge id)kSecUseAuthenticationUI: legacyAuthenticationUIFail
    };
    CFTypeRef result = NULL;
    OSStatus status = SecItemCopyMatching((__bridge CFDictionaryRef)query, &result);
    if (status == errSecItemNotFound) {
      return 1;
    }
    if (status != errSecSuccess || result == NULL) {
      if (result != NULL) {
        CFRelease(result);
      }
      return (int)status;
    }

    NSData *data = CFBridgingRelease(result);
    NSString *value = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
    const char *utf8Value = value.UTF8String;
    if (utf8Value == NULL) {
      return (int)errSecDecode;
    }
    char *copy = strdup(utf8Value);
    if (copy == NULL) {
      return (int)errSecAllocate;
    }
    *outValue = copy;
    return 0;
  }
}

int paper_float_keychain_set(const char *service, const char *account, const char *secret) {
  if (service == NULL || account == NULL || secret == NULL) {
    return (int)errSecParam;
  }

  @autoreleasepool {
    NSString *serviceValue = [NSString stringWithUTF8String:service];
    NSString *accountValue = [NSString stringWithUTF8String:account];
    NSData *secretData = [[NSString stringWithUTF8String:secret] dataUsingEncoding:NSUTF8StringEncoding];
    if (serviceValue == nil || accountValue == nil || secretData == nil) {
      return (int)errSecParam;
    }

    NSDictionary *query = @{
      (__bridge id)kSecClass: (__bridge id)kSecClassGenericPassword,
      (__bridge id)kSecAttrService: serviceValue,
      (__bridge id)kSecAttrAccount: accountValue
    };

    // The legacy generic-password default ACL was bound to whichever ad-hoc
    // build first created the item. Every rebuild then appeared to Keychain as
    // a different application and caused the login-password dialog on the hot
    // translation path. Create an explicit trusted-application ACL instead.
    // Combined with the stable local signer and the v2 namespace in Rust, the
    // running app remains trusted across rebuilds and the old ACL is never read.
    SecTrustedApplicationRef trustedApplication = NULL;
    SecAccessRef access = NULL;
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    OSStatus status = SecTrustedApplicationCreateFromPath(NULL, &trustedApplication);
    if (status == errSecSuccess && trustedApplication != NULL) {
      const void *trustedValues[] = {trustedApplication};
      CFArrayRef trustedApplications = CFArrayCreate(
        kCFAllocatorDefault,
        trustedValues,
        1,
        &kCFTypeArrayCallBacks
      );
      if (trustedApplications == NULL) {
        status = errSecAllocate;
      } else {
        status = SecAccessCreate(
          (__bridge CFStringRef)@"Paper Float Translator API Key",
          trustedApplications,
          &access
        );
        CFRelease(trustedApplications);
      }
    }
#pragma clang diagnostic pop
    if (trustedApplication != NULL) {
      CFRelease(trustedApplication);
    }
    if (status != errSecSuccess || access == NULL) {
      if (access != NULL) {
        CFRelease(access);
      }
      return (int)status;
    }

    NSDictionary *update = @{
      (__bridge id)kSecValueData: secretData,
      (__bridge id)kSecAttrAccess: (__bridge id)access
    };
    status = SecItemUpdate(
      (__bridge CFDictionaryRef)query,
      (__bridge CFDictionaryRef)update
    );
    if (status == errSecItemNotFound) {
      NSMutableDictionary *addition = [query mutableCopy];
      addition[(__bridge id)kSecValueData] = secretData;
      addition[(__bridge id)kSecAttrAccess] = (__bridge id)access;
      status = SecItemAdd((__bridge CFDictionaryRef)addition, NULL);
    }
    CFRelease(access);
    return (int)status;
  }
}

int paper_float_keychain_delete(const char *service, const char *account) {
  if (service == NULL || account == NULL) {
    return (int)errSecParam;
  }

  @autoreleasepool {
    NSString *serviceValue = [NSString stringWithUTF8String:service];
    NSString *accountValue = [NSString stringWithUTF8String:account];
    if (serviceValue == nil || accountValue == nil) {
      return (int)errSecParam;
    }
    NSDictionary *query = @{
      (__bridge id)kSecClass: (__bridge id)kSecClassGenericPassword,
      (__bridge id)kSecAttrService: serviceValue,
      (__bridge id)kSecAttrAccount: accountValue
    };
    OSStatus status = SecItemDelete((__bridge CFDictionaryRef)query);
    return status == errSecItemNotFound ? 0 : (int)status;
  }
}

void paper_float_keychain_free(char *value) {
  free(value);
}

void paper_float_accessibility_prompt(void) {
  NSDictionary *trustOptions = @{(__bridge NSString *)kAXTrustedCheckOptionPrompt: @YES};
  AXIsProcessTrustedWithOptions((__bridge CFDictionaryRef)trustOptions);
}

static BOOL PFStringIsBlank(NSString *text) {
  if (text == nil) {
    return YES;
  }

  NSString *trimmed = [text stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]];
  return trimmed.length == 0;
}

static BOOL PFRectIsFiniteAndUsable(CGRect bounds);

static id PFAttributeWithError(
  id element,
  CFStringRef attribute,
  AXError *readError
) {
  if (readError != NULL) {
    *readError = kAXErrorIllegalArgument;
  }
  if (element == nil) {
    return nil;
  }

  CFTypeRef value = NULL;
  AXError error = AXUIElementCopyAttributeValue((__bridge AXUIElementRef)element, attribute, &value);
  AXError effectiveError = error == kAXErrorSuccess && value == NULL
    ? kAXErrorFailure
    : error;
  if (readError != NULL) {
    *readError = effectiveError;
  }
  if (effectiveError != kAXErrorSuccess) {
    PFRecordSelectionAXReadError(effectiveError);
    if (value != NULL) {
      CFRelease(value);
    }
    return nil;
  }

  return CFBridgingRelease(value);
}

static id PFAttribute(id element, CFStringRef attribute) {
  return PFAttributeWithError(element, attribute, NULL);
}

static PFDirectSelectedTextReadStatus PFCopyDirectSelectedText(
  id element,
  NSString **selectedText
) {
  if (selectedText != NULL) {
    *selectedText = nil;
  }
  if (element == nil) {
    return PFDirectSelectedTextReadFailure;
  }

  CFTypeRef rawValue = NULL;
  AXError error = AXUIElementCopyAttributeValue(
    (__bridge AXUIElementRef)element,
    kAXSelectedTextAttribute,
    &rawValue
  );
  if (PFSelectionAttributeErrorIsSafeAbsence(error) ||
      (error == kAXErrorSuccess && rawValue == NULL)) {
    if (rawValue != NULL) {
      CFRelease(rawValue);
    }
    return PFDirectSelectedTextReadSafeAbsence;
  }
  if (error != kAXErrorSuccess || rawValue == NULL) {
    PFRecordSelectionAXReadError(error);
    if (rawValue != NULL) {
      CFRelease(rawValue);
    }
    return PFDirectSelectedTextReadFailure;
  }

  id value = CFBridgingRelease(rawValue);
  if (![value isKindOfClass:[NSString class]]) {
    PFRecordSelectionAXReadError(kAXErrorFailure);
    return PFDirectSelectedTextReadFailure;
  }
  NSString *text = (NSString *)value;
  if (PFStringIsBlank(text)) {
    return PFDirectSelectedTextReadSafeAbsence;
  }
  if (selectedText != NULL) {
    *selectedText = text;
  }
  return PFDirectSelectedTextReadText;
}

static NSString *PFStringForRange(id element, id rangeObject) {
  if (element == nil || rangeObject == nil || CFGetTypeID((__bridge CFTypeRef)rangeObject) != AXValueGetTypeID()) {
    return nil;
  }

  AXValueRef rangeValue = (__bridge AXValueRef)rangeObject;
  CFRange range = CFRangeMake(0, 0);
  if (AXValueGetType(rangeValue) != kAXValueCFRangeType ||
      !AXValueGetValue(rangeValue, kAXValueCFRangeType, &range) ||
      range.length <= 0) {
    return nil;
  }

  CFTypeRef result = NULL;
  AXError error = AXUIElementCopyParameterizedAttributeValue(
    (__bridge AXUIElementRef)element,
    kAXStringForRangeParameterizedAttribute,
    rangeValue,
    &result
  );
  if (error != kAXErrorSuccess || result == NULL) {
    PFRecordSelectionAXReadError(error);
    return nil;
  }

  id rawText = CFBridgingRelease(result);
  if (![rawText isKindOfClass:[NSString class]]) {
    return nil;
  }
  NSString *text = (NSString *)rawText;
  return PFStringIsBlank(text) ? nil : text;
}

static BOOL PFSelectedRangeValueIsValid(id rawRange, BOOL *nonEmpty) {
  if (nonEmpty != NULL) {
    *nonEmpty = NO;
  }
  if (rawRange == nil ||
      CFGetTypeID((__bridge CFTypeRef)rawRange) != AXValueGetTypeID()) {
    return NO;
  }
  AXValueRef rangeValue = (__bridge AXValueRef)rawRange;
  CFRange range = CFRangeMake(0, 0);
  if (AXValueGetType(rangeValue) != kAXValueCFRangeType ||
      !AXValueGetValue(rangeValue, kAXValueCFRangeType, &range) ||
      range.location < 0 ||
      range.length < 0) {
    return NO;
  }
  if (nonEmpty != NULL) {
    *nonEmpty = range.length > 0;
  }
  return YES;
}

static BOOL PFSelectedRangeValueIsNonEmpty(id rawRange) {
  BOOL nonEmpty = NO;
  return PFSelectedRangeValueIsValid(rawRange, &nonEmpty) && nonEmpty;
}

static NSArray *PFNonEmptySelectedRangeValues(
  id element,
  BOOL *valid,
  AXError *hardError
) {
  if (valid != NULL) {
    *valid = YES;
  }
  if (hardError != NULL) {
    *hardError = kAXErrorSuccess;
  }
  AXError rangesError = kAXErrorSuccess;
  id rawRanges = PFAttributeWithError(
    element,
    kAXSelectedTextRangesAttribute,
    &rangesError
  );
  NSMutableArray *result = [NSMutableArray array];
  if (rangesError != kAXErrorSuccess &&
      !PFSelectionAttributeErrorIsSafeAbsence(rangesError)) {
    if (valid != NULL) {
      *valid = NO;
    }
    if (hardError != NULL) {
      *hardError = rangesError;
    }
    return @[];
  }
  if (rawRanges != nil) {
    if (![rawRanges isKindOfClass:[NSArray class]]) {
      if (valid != NULL) {
        *valid = NO;
      }
      if (hardError != NULL) {
        *hardError = kAXErrorFailure;
      }
      return @[];
    }
    for (id rawRange in (NSArray *)rawRanges) {
      BOOL nonEmpty = NO;
      if (!PFSelectedRangeValueIsValid(rawRange, &nonEmpty)) {
        if (valid != NULL) {
          *valid = NO;
        }
        if (hardError != NULL) {
          *hardError = kAXErrorFailure;
        }
        return @[];
      }
      if (nonEmpty) {
        [result addObject:rawRange];
      }
    }
    if (result.count > 0) {
      return result;
    }
  }

  AXError rangeError = kAXErrorSuccess;
  id rawRange = PFAttributeWithError(
    element,
    kAXSelectedTextRangeAttribute,
    &rangeError
  );
  if (rangeError != kAXErrorSuccess &&
      !PFSelectionAttributeErrorIsSafeAbsence(rangeError)) {
    if (valid != NULL) {
      *valid = NO;
    }
    if (hardError != NULL) {
      *hardError = rangeError;
    }
    return @[];
  }
  if (rawRange == nil) {
    return result;
  }
  BOOL nonEmpty = NO;
  if (!PFSelectedRangeValueIsValid(rawRange, &nonEmpty)) {
    if (valid != NULL) {
      *valid = NO;
    }
    if (hardError != NULL) {
      *hardError = kAXErrorFailure;
    }
    return @[];
  }
  if (nonEmpty) {
    [result addObject:rawRange];
  }
  return result;
}

static NSString *PFCombineSelectedRangeTextParts(NSArray *parts) {
  if (parts.count == 0) {
    return nil;
  }
  for (id part in parts) {
    if (![part isKindOfClass:[NSString class]] || PFStringIsBlank((NSString *)part)) {
      return nil;
    }
  }
  NSString *combined = [parts componentsJoinedByString:@"\n"];
  return PFStringIsBlank(combined) ? nil : combined;
}

static NSString *PFStringForSelectedRangeValues(id element, NSArray *rawRanges) {
  if (element == nil || rawRanges.count == 0) {
    return nil;
  }
  NSMutableArray<NSString *> *parts = [NSMutableArray arrayWithCapacity:rawRanges.count];
  for (id rawRange in rawRanges) {
    if (!PFSelectedRangeValueIsNonEmpty(rawRange)) {
      return nil;
    }
    NSString *part = PFStringForRange(element, rawRange);
    if (PFStringIsBlank(part)) {
      return nil;
    }
    [parts addObject:part];
  }
  return PFCombineSelectedRangeTextParts(parts);
}

static BOOL PFSelectedRangeValueArraysEqual(NSArray *first, NSArray *second) {
  if (first.count != second.count) {
    return NO;
  }
  for (NSUInteger index = 0; index < first.count; index += 1) {
    if (!CFEqual(
          (__bridge CFTypeRef)first[index],
          (__bridge CFTypeRef)second[index]
        )) {
      return NO;
    }
  }
  return YES;
}

static BOOL PFElementHasExpectedSelectedRanges(id element, NSArray *expectedRanges) {
  BOOL currentRangesValid = YES;
  AXError rangesError = kAXErrorSuccess;
  NSArray *currentRanges = PFNonEmptySelectedRangeValues(
    element,
    &currentRangesValid,
    &rangesError
  );
  return currentRangesValid &&
    rangesError == kAXErrorSuccess &&
    PFSelectedRangeValueArraysEqual(expectedRanges, currentRanges);
}

static NSString *PFStringForStableSelectedRangeValues(
  id element,
  NSArray *expectedRanges
) {
  BOOL identityValidBeforeRead = PFElementHasExpectedSelectedRanges(
    element,
    expectedRanges
  );
  if (!identityValidBeforeRead) {
    return nil;
  }
  NSString *text = PFStringForSelectedRangeValues(element, expectedRanges);
  BOOL allRangeTextRead = !PFStringIsBlank(text);
  BOOL identityValidAfterRead = allRangeTextRead &&
    PFElementHasExpectedSelectedRanges(element, expectedRanges);
  if (!PFStableSelectedRangeReadCanEmit(
        identityValidBeforeRead,
        allRangeTextRead,
        identityValidAfterRead
      )) {
    return nil;
  }
  return text;
}

static NSString *PFSelectedText(
  id element,
  BOOL requiresSelectionChangeEvidence,
  BOOL selectionChangeObserved,
  BOOL baselineCaptured,
  NSArray *baselineElements,
  NSArray *baselineTexts,
  BOOL *readFailed
) {
  if (readFailed != NULL) {
    *readFailed = NO;
  }

  // Preview/PDFKit may expose AXSelectedText without implementing the full
  // selected-range/string-for-range/bounds family. This path is restricted to
  // the already PID/window/anchor-validated preferred element ancestry. Read
  // twice so a changing selection cannot be emitted as a stable result.
  NSString *firstDirect = nil;
  PFDirectSelectedTextReadStatus firstDirectStatus = PFCopyDirectSelectedText(
    element,
    &firstDirect
  );
  NSString *secondDirect = nil;
  PFDirectSelectedTextReadStatus secondDirectStatus = firstDirectStatus ==
      PFDirectSelectedTextReadText
    ? PFCopyDirectSelectedText(element, &secondDirect)
    : PFDirectSelectedTextReadSafeAbsence;
  PFPreferredDirectReadDecision directDecision =
    PFResolvePreferredDirectReadDecision(
      firstDirectStatus,
      secondDirectStatus,
      firstDirect != nil && secondDirect != nil &&
        [firstDirect isEqualToString:secondDirect]
    );
  NSInteger baselineIndex = NSNotFound;
  if (baselineCaptured &&
      baselineElements.count == baselineTexts.count) {
    for (NSUInteger index = 0; index < baselineElements.count; index += 1) {
      if (PFElementsEqual(element, baselineElements[index])) {
        baselineIndex = (NSInteger)index;
        break;
      }
    }
  }
  id rawBaseline = baselineIndex == NSNotFound
    ? nil
    : baselineTexts[(NSUInteger)baselineIndex];
  NSString *baselineText = [rawBaseline isKindOfClass:[NSString class]]
    ? (NSString *)rawBaseline
    : nil;
  if (PFDirectSelectedTextMayEmit(
        directDecision,
        requiresSelectionChangeEvidence,
        selectionChangeObserved,
        baselineIndex != NSNotFound,
        baselineIndex != NSNotFound &&
          ((baselineText == nil && firstDirect == nil) ||
           [baselineText isEqualToString:firstDirect])
      )) {
    return firstDirect;
  }
  if (directDecision == PFPreferredDirectReadFailClosed) {
    if (readFailed != NULL) {
      *readFailed = YES;
    }
    return nil;
  }

  BOOL rangesValid = YES;
  AXError rangesError = kAXErrorSuccess;
  NSArray *rawRanges = PFNonEmptySelectedRangeValues(
    element,
    &rangesValid,
    &rangesError
  );
  if (!rangesValid || rangesError != kAXErrorSuccess) {
    if (readFailed != NULL) {
      *readFailed = YES;
    }
    return nil;
  }
  if (rawRanges.count > 0) {
    NSString *text = PFStringForStableSelectedRangeValues(element, rawRanges);
    if (PFStringIsBlank(text) && readFailed != NULL) {
      *readFailed = YES;
    }
    return text;
  }

  return nil;
}

static BOOL PFBoundsForSelectedRange(id element, id rawRange, CGRect *bounds) {
  if (bounds != NULL) {
    *bounds = CGRectNull;
  }
  if (!PFSelectedRangeValueIsNonEmpty(rawRange)) {
    return NO;
  }

  CFTypeRef result = NULL;
  AXError error = AXUIElementCopyParameterizedAttributeValue(
    (__bridge AXUIElementRef)element,
    kAXBoundsForRangeParameterizedAttribute,
    (__bridge AXValueRef)rawRange,
    &result
  );
  if (error != kAXErrorSuccess || result == NULL) {
    PFRecordSelectionAXReadError(error);
    if (result != NULL) {
      CFRelease(result);
    }
    return NO;
  }

  id rawBounds = CFBridgingRelease(result);
  if (CFGetTypeID((__bridge CFTypeRef)rawBounds) != AXValueGetTypeID()) {
    return NO;
  }
  AXValueRef boundsValue = (__bridge AXValueRef)rawBounds;
  CGRect resolved = CGRectNull;
  if (AXValueGetType(boundsValue) != kAXValueCGRectType ||
      !AXValueGetValue(boundsValue, kAXValueCGRectType, &resolved) ||
      !PFRectIsFiniteAndUsable(resolved)) {
    return NO;
  }
  if (bounds != NULL) {
    *bounds = resolved;
  }
  return YES;
}

static BOOL PFSelectedRangeBoundsContainAnchor(
  id element,
  NSArray *selectedRanges,
  CGPoint anchor
) {
  if (element == nil ||
      selectedRanges.count == 0 ||
      !isfinite(anchor.x) ||
      !isfinite(anchor.y)) {
    return NO;
  }
  BOOL matched = NO;
  for (id rawRange in selectedRanges) {
    CGRect bounds = CGRectNull;
    if (!PFBoundsForSelectedRange(element, rawRange, &bounds)) {
      return NO;
    }
    if (CGRectContainsPoint(CGRectInset(bounds, -2.0, -2.0), anchor)) {
      matched = YES;
    }
  }
  return matched;
}

static PFDirectSelectedTextReadStatus PFCopyAnchoredFocusedSelectedText(
  id element,
  CGPoint anchor,
  NSString **selectedText
) {
  if (selectedText != NULL) {
    *selectedText = nil;
  }

  BOOL rangesValid = YES;
  AXError rangesError = kAXErrorSuccess;
  NSArray *expectedRanges = PFNonEmptySelectedRangeValues(
    element,
    &rangesValid,
    &rangesError
  );
  if (!rangesValid || rangesError != kAXErrorSuccess) {
    return PFDirectSelectedTextReadFailure;
  }
  BOOL anchorMatches = PFSelectedRangeBoundsContainAnchor(
    element,
    expectedRanges,
    anchor
  );
  if (!anchorMatches) {
    return PFDirectSelectedTextReadSafeAbsence;
  }

  NSString *firstDirect = nil;
  PFDirectSelectedTextReadStatus firstStatus = PFCopyDirectSelectedText(
    element,
    &firstDirect
  );
  NSString *secondDirect = nil;
  PFDirectSelectedTextReadStatus secondStatus = firstStatus ==
      PFDirectSelectedTextReadText
    ? PFCopyDirectSelectedText(element, &secondDirect)
    : PFDirectSelectedTextReadSafeAbsence;
  BOOL rangesStable = PFElementHasExpectedSelectedRanges(element, expectedRanges);
  BOOL textsEqual = firstDirect != nil &&
    secondDirect != nil &&
    [firstDirect isEqualToString:secondDirect];
  PFDirectSelectedTextReadStatus resolvedStatus =
    PFResolveAnchoredFocusedDirectReadStatus(
      rangesStable,
      anchorMatches,
      firstStatus,
      secondStatus,
      textsEqual
    );
  if (resolvedStatus == PFDirectSelectedTextReadText) {
    if (selectedText != NULL) {
      *selectedText = firstDirect;
    }
  }
  return resolvedStatus;
}

static id PFParent(id element) {
  id value = PFAttribute(element, kAXParentAttribute);
  if (value != nil && CFGetTypeID((__bridge CFTypeRef)value) == AXUIElementGetTypeID()) {
    return value;
  }

  return nil;
}

static id PFFocusedWindow(id appElement) {
  id value = PFAttribute(appElement, kAXFocusedWindowAttribute);
  if (value != nil && CFGetTypeID((__bridge CFTypeRef)value) == AXUIElementGetTypeID()) {
    return value;
  }

  return nil;
}

static pid_t PFSelfPID(void) {
  return getpid();
}

static BOOL PFIsSelfPID(pid_t pid) {
  return pid > 0 && pid == PFSelfPID();
}

static pid_t PFFrontmostPID(void) {
  NSRunningApplication *frontmostApp = [NSWorkspace sharedWorkspace].frontmostApplication;
  return frontmostApp == nil ? 0 : frontmostApp.processIdentifier;
}

static pid_t PFEventTargetPID(CGEventRef event) {
  if (event == NULL) {
    return 0;
  }

  int64_t rawPID = CGEventGetIntegerValueField(event, kCGEventTargetUnixProcessID);
  if (rawPID <= 0 || rawPID > INT_MAX) {
    return 0;
  }

  return (pid_t)rawPID;
}

typedef NS_ENUM(NSInteger, PFSelectionWindowFreezeStatus) {
  PFSelectionWindowFreezeSuccess = 0,
  PFSelectionWindowFreezeMissingWindowID = 1,
  PFSelectionWindowFreezeWindowIDMismatch = 2,
  PFSelectionWindowFreezeInvalidTargetPID = 3,
  PFSelectionWindowFreezeSelfTarget = 4,
  PFSelectionWindowFreezeMetadataMissing = 5,
  PFSelectionWindowFreezeMetadataAmbiguous = 6,
  PFSelectionWindowFreezeMetadataPIDMismatch = 7,
  PFSelectionWindowFreezeMetadataOffscreen = 8,
  PFSelectionWindowFreezeMetadataLayerMismatch = 9,
  PFSelectionWindowFreezeMetadataBoundsInvalid = 10,
  PFSelectionWindowFreezeAXWindowMissing = 11,
  PFSelectionWindowFreezeAXWindowAmbiguous = 12,
  PFSelectionWindowFreezeGesturePIDMismatch = 13
};

typedef NS_ENUM(NSInteger, PFSelectionWindowResolutionStrategy) {
  PFSelectionWindowResolutionReject = 0,
  PFSelectionWindowResolutionFrozenCGWindow = 1,
  PFSelectionWindowResolutionAnchoredAXWindow = 2
};

static PFSelectionWindowResolutionStrategy PFChooseSelectionWindowResolutionStrategy(
  PFSelectionWindowFreezeStatus pidStatus,
  PFSelectionWindowFreezeStatus windowStatus,
  PFSelectionWindowFreezeStatus *failureStatus
) {
  if (failureStatus != NULL) {
    *failureStatus = PFSelectionWindowFreezeSuccess;
  }
  if (pidStatus != PFSelectionWindowFreezeSuccess) {
    if (failureStatus != NULL) {
      *failureStatus = pidStatus;
    }
    return PFSelectionWindowResolutionReject;
  }
  if (windowStatus == PFSelectionWindowFreezeSuccess) {
    return PFSelectionWindowResolutionFrozenCGWindow;
  }
  if (windowStatus == PFSelectionWindowFreezeMissingWindowID) {
    // Listen-only taps commonly expose no CGWindow ID, and querying another
    // process through CGWindowList may itself be privacy-restricted. The later
    // AX path still freezes the event PID and mouse-up anchor, verifies the
    // hit-tested element/window ancestry, and fails closed if focus changed.
    return PFSelectionWindowResolutionAnchoredAXWindow;
  }
  if (failureStatus != NULL) {
    *failureStatus = windowStatus;
  }
  return PFSelectionWindowResolutionReject;
}

typedef struct {
  CGWindowID windowID;
  pid_t ownerPID;
  NSInteger layer;
  BOOL isOnscreen;
  CGRect bounds;
} PFWindowMetadataSnapshot;

static CGWindowID PFEventReceivingWindowID(CGEventRef event) {
  if (event == NULL) {
    return kCGNullWindowID;
  }

  // Both public CGEvent fields are O(1) payload reads. Recent macOS releases
  // may leave the handleable-window field at zero for listen-only taps even
  // though the under-pointer field identifies the Preview document window.
  // Treat the latter only as a frozen candidate. If both fields are zero, the
  // main-queue reader uses the fixed event PID and mouse-up anchor to resolve
  // and revalidate one AX window. Do not query CGWindowList here: that call is
  // privacy-restricted for other apps and is too expensive for an event tap.
  int64_t rawWindowID = CGEventGetIntegerValueField(
    event,
    kCGMouseEventWindowUnderMousePointerThatCanHandleThisEvent
  );
  if (rawWindowID > 0 && (uint64_t)rawWindowID <= UINT32_MAX) {
    return (CGWindowID)rawWindowID;
  }

  rawWindowID = CGEventGetIntegerValueField(
    event,
    kCGMouseEventWindowUnderMousePointer
  );
  return rawWindowID > 0 && (uint64_t)rawWindowID <= UINT32_MAX
    ? (CGWindowID)rawWindowID
    : kCGNullWindowID;
}

static PFSelectionWindowFreezeStatus PFChooseGestureWindowID(
  CGWindowID mouseDownWindowID,
  CGWindowID mouseUpWindowID,
  CGWindowID *selectedWindowID
) {
  if (selectedWindowID != NULL) {
    *selectedWindowID = kCGNullWindowID;
  }
  if (mouseDownWindowID != kCGNullWindowID &&
      mouseUpWindowID != kCGNullWindowID &&
      mouseDownWindowID != mouseUpWindowID) {
    return PFSelectionWindowFreezeWindowIDMismatch;
  }

  CGWindowID selected = mouseDownWindowID != kCGNullWindowID
    ? mouseDownWindowID
    : mouseUpWindowID;
  if (selected == kCGNullWindowID) {
    return PFSelectionWindowFreezeMissingWindowID;
  }
  if (selectedWindowID != NULL) {
    *selectedWindowID = selected;
  }
  return PFSelectionWindowFreezeSuccess;
}

static PFSelectionWindowFreezeStatus PFChooseGestureTargetPID(
  pid_t mouseDownPID,
  pid_t mouseUpPID,
  BOOL mouseDownPIDAnchored,
  pid_t selfPID,
  pid_t *selectedPID
) {
  if (selectedPID != NULL) {
    *selectedPID = 0;
  }
  if (mouseDownPIDAnchored && mouseDownPID <= 0) {
    return PFSelectionWindowFreezeInvalidTargetPID;
  }
  // The public CGEvent target PID is not a stable application identity for
  // every client. Chromium may route the event through a process identity
  // that differs from the AX element/window owner. When mouse-down already
  // froze the under-pointer AX owner, keep that stronger identity and let the
  // later anchored element/window/frontmost checks reject an actual app
  // switch. Without an AX anchor, retain the strict down/up event-PID check.
  if (!mouseDownPIDAnchored &&
      mouseDownPID > 0 && mouseUpPID > 0 && mouseDownPID != mouseUpPID) {
    return PFSelectionWindowFreezeGesturePIDMismatch;
  }
  pid_t selected = mouseDownPID > 0 ? mouseDownPID : mouseUpPID;
  if (selected <= 0) {
    return PFSelectionWindowFreezeInvalidTargetPID;
  }
  if (selected == selfPID) {
    return PFSelectionWindowFreezeSelfTarget;
  }
  if (selectedPID != NULL) {
    *selectedPID = selected;
  }
  return PFSelectionWindowFreezeSuccess;
}

static BOOL PFRectIsFiniteAndUsable(CGRect bounds) {
  return isfinite(bounds.origin.x) &&
    isfinite(bounds.origin.y) &&
    isfinite(bounds.size.width) &&
    isfinite(bounds.size.height) &&
    isfinite(CGRectGetMaxX(bounds)) &&
    isfinite(CGRectGetMaxY(bounds)) &&
    bounds.size.width > 0 &&
    bounds.size.height > 0;
}

typedef NS_ENUM(NSInteger, PFSelectionAnchorResolutionStatus) {
  PFSelectionAnchorResolutionSuccess = 0,
  PFSelectionAnchorResolutionNoCandidate = 1,
  PFSelectionAnchorResolutionAmbiguous = 2,
  PFSelectionAnchorResolutionBoundsUnavailable = 3,
  PFSelectionAnchorResolutionInvalidInput = 4
};

static BOOL PFPointIsFinite(CGPoint point) {
  return isfinite(point.x) && isfinite(point.y);
}

static BOOL PFRectClearlyContainsPoint(CGRect bounds, CGPoint point) {
  return PFRectIsFiniteAndUsable(bounds) &&
    PFPointIsFinite(point) &&
    point.x >= CGRectGetMinX(bounds) &&
    point.x <= CGRectGetMaxX(bounds) &&
    point.y >= CGRectGetMinY(bounds) &&
    point.y <= CGRectGetMaxY(bounds);
}

static CGFloat PFDistanceFromPointToRect(CGPoint point, CGRect bounds) {
  CGFloat dx = 0;
  if (point.x < CGRectGetMinX(bounds)) {
    dx = CGRectGetMinX(bounds) - point.x;
  } else if (point.x > CGRectGetMaxX(bounds)) {
    dx = point.x - CGRectGetMaxX(bounds);
  }
  CGFloat dy = 0;
  if (point.y < CGRectGetMinY(bounds)) {
    dy = CGRectGetMinY(bounds) - point.y;
  } else if (point.y > CGRectGetMaxY(bounds)) {
    dy = point.y - CGRectGetMaxY(bounds);
  }
  return hypot(dx, dy);
}

static PFSelectionAnchorResolutionStatus PFResolveSelectionAnchorCandidate(
  CGPoint anchor,
  const CGRect *candidateBounds,
  const BOOL *boundsAvailable,
  const size_t *candidateGroupIndices,
  size_t candidateCount,
  size_t groupCount,
  size_t *selectedGroupIndex
) {
  if (selectedGroupIndex != NULL) {
    *selectedGroupIndex = SIZE_MAX;
  }
  if (!PFPointIsFinite(anchor) ||
      (candidateCount > 0 &&
       (candidateBounds == NULL || boundsAvailable == NULL ||
        candidateGroupIndices == NULL || groupCount == 0)) ||
      (candidateCount == 0 && groupCount != 0)) {
    return PFSelectionAnchorResolutionInvalidInput;
  }
  if (candidateCount == 0) {
    return PFSelectionAnchorResolutionNoCandidate;
  }

  for (size_t index = 0; index < candidateCount; index += 1) {
    if (!boundsAvailable[index]) {
      return PFSelectionAnchorResolutionBoundsUnavailable;
    }
    if (candidateGroupIndices[index] >= groupCount ||
        !PFRectIsFiniteAndUsable(candidateBounds[index])) {
      return PFSelectionAnchorResolutionInvalidInput;
    }
    CGFloat distance = PFDistanceFromPointToRect(anchor, candidateBounds[index]);
    if (!isfinite(distance)) {
      return PFSelectionAnchorResolutionInvalidInput;
    }
  }

  BOOL *groupSeen = calloc(groupCount, sizeof(BOOL));
  BOOL *groupContainsAnchor = calloc(groupCount, sizeof(BOOL));
  CGFloat *groupNearestDistances = calloc(groupCount, sizeof(CGFloat));
  if (groupSeen == NULL || groupContainsAnchor == NULL ||
      groupNearestDistances == NULL) {
    free(groupSeen);
    free(groupContainsAnchor);
    free(groupNearestDistances);
    return PFSelectionAnchorResolutionInvalidInput;
  }
  for (size_t group = 0; group < groupCount; group += 1) {
    groupNearestDistances[group] = CGFLOAT_MAX;
  }
  for (size_t index = 0; index < candidateCount; index += 1) {
    size_t group = candidateGroupIndices[index];
    groupSeen[group] = YES;
    if (PFRectClearlyContainsPoint(candidateBounds[index], anchor)) {
      groupContainsAnchor[group] = YES;
    }
    CGFloat distance = PFDistanceFromPointToRect(anchor, candidateBounds[index]);
    if (distance < groupNearestDistances[group]) {
      groupNearestDistances[group] = distance;
    }
  }

  size_t containedGroupCount = 0;
  size_t containedGroupIndex = SIZE_MAX;
  for (size_t group = 0; group < groupCount; group += 1) {
    if (!groupSeen[group]) {
      free(groupSeen);
      free(groupContainsAnchor);
      free(groupNearestDistances);
      return PFSelectionAnchorResolutionInvalidInput;
    }
    if (groupContainsAnchor[group]) {
      containedGroupCount += 1;
      containedGroupIndex = group;
    }
  }
  if (containedGroupCount == 1) {
    if (selectedGroupIndex != NULL) {
      *selectedGroupIndex = containedGroupIndex;
    }
    free(groupSeen);
    free(groupContainsAnchor);
    free(groupNearestDistances);
    return PFSelectionAnchorResolutionSuccess;
  }
  if (containedGroupCount > 1) {
    free(groupSeen);
    free(groupContainsAnchor);
    free(groupNearestDistances);
    return PFSelectionAnchorResolutionAmbiguous;
  }

  CGFloat nearestDistance = CGFLOAT_MAX;
  CGFloat secondNearestDistance = CGFLOAT_MAX;
  size_t nearestGroupIndex = SIZE_MAX;
  for (size_t group = 0; group < groupCount; group += 1) {
    CGFloat distance = groupNearestDistances[group];
    if (distance < nearestDistance) {
      secondNearestDistance = nearestDistance;
      nearestDistance = distance;
      nearestGroupIndex = group;
    } else if (distance < secondNearestDistance) {
      secondNearestDistance = distance;
    }
  }
  free(groupSeen);
  free(groupContainsAnchor);
  free(groupNearestDistances);

  if (nearestGroupIndex == SIZE_MAX) {
    return PFSelectionAnchorResolutionNoCandidate;
  }
  const CGFloat distanceTieEpsilon = 0.25;
  if (secondNearestDistance - nearestDistance <= distanceTieEpsilon) {
    return PFSelectionAnchorResolutionAmbiguous;
  }
  if (selectedGroupIndex != NULL) {
    *selectedGroupIndex = nearestGroupIndex;
  }
  return PFSelectionAnchorResolutionSuccess;
}

typedef struct {
  CGDirectDisplayID displayID;
  CGRect bounds;
} PFDisplayGeometry;

typedef struct {
  CGDirectDisplayID displayID;
  CGRect frame;
  CGRect visibleFrame;
} PFScreenGeometry;

typedef NS_ENUM(NSInteger, PFVisibleWorkAreaStatus) {
  PFVisibleWorkAreaSuccess = 0,
  PFVisibleWorkAreaInvalidInput = 1,
  PFVisibleWorkAreaDisplayNotFound = 2,
  PFVisibleWorkAreaDisplayAmbiguous = 3,
  PFVisibleWorkAreaScreenNotFound = 4,
  PFVisibleWorkAreaScreenAmbiguous = 5,
  PFVisibleWorkAreaGeometryInvalid = 6
};

static BOOL PFNearlyEqual(CGFloat lhs, CGFloat rhs, CGFloat tolerance) {
  return isfinite(lhs) && isfinite(rhs) && fabs(lhs - rhs) <= tolerance;
}

static PFVisibleWorkAreaStatus PFResolveVisibleWorkAreaForAnchor(
  CGPoint anchor,
  const PFDisplayGeometry *displays,
  size_t displayCount,
  const PFScreenGeometry *screens,
  size_t screenCount,
  CGDirectDisplayID *selectedDisplayID,
  CGRect *visibleWorkArea
) {
  if (selectedDisplayID != NULL) {
    *selectedDisplayID = kCGNullDirectDisplay;
  }
  if (visibleWorkArea != NULL) {
    *visibleWorkArea = CGRectNull;
  }
  if (!PFPointIsFinite(anchor) ||
      displayCount == 0 ||
      displays == NULL ||
      screenCount == 0 ||
      screens == NULL) {
    return PFVisibleWorkAreaInvalidInput;
  }

  const PFDisplayGeometry *targetDisplay = NULL;
  size_t matchingDisplayCount = 0;
  for (size_t index = 0; index < displayCount; index += 1) {
    if (displays[index].displayID == kCGNullDirectDisplay ||
        !PFRectIsFiniteAndUsable(displays[index].bounds)) {
      return PFVisibleWorkAreaGeometryInvalid;
    }
    for (size_t prior = 0; prior < index; prior += 1) {
      if (displays[prior].displayID == displays[index].displayID) {
        return PFVisibleWorkAreaDisplayAmbiguous;
      }
    }
    if (PFRectClearlyContainsPoint(displays[index].bounds, anchor)) {
      targetDisplay = &displays[index];
      matchingDisplayCount += 1;
    }
  }
  if (matchingDisplayCount == 0 || targetDisplay == NULL) {
    return PFVisibleWorkAreaDisplayNotFound;
  }
  if (matchingDisplayCount != 1) {
    return PFVisibleWorkAreaDisplayAmbiguous;
  }

  const PFScreenGeometry *targetScreen = NULL;
  size_t matchingScreenCount = 0;
  for (size_t index = 0; index < screenCount; index += 1) {
    if (screens[index].displayID == targetDisplay->displayID) {
      targetScreen = &screens[index];
      matchingScreenCount += 1;
    }
  }
  if (matchingScreenCount == 0 || targetScreen == NULL) {
    return PFVisibleWorkAreaScreenNotFound;
  }
  if (matchingScreenCount != 1) {
    return PFVisibleWorkAreaScreenAmbiguous;
  }

  CGRect cgBounds = targetDisplay->bounds;
  CGRect frame = targetScreen->frame;
  CGRect visible = targetScreen->visibleFrame;
  if (!PFRectIsFiniteAndUsable(frame) ||
      !PFRectIsFiniteAndUsable(visible) ||
      !PFNearlyEqual(frame.size.width, cgBounds.size.width, 0.5) ||
      !PFNearlyEqual(frame.size.height, cgBounds.size.height, 0.5)) {
    return PFVisibleWorkAreaGeometryInvalid;
  }

  CGFloat left = CGRectGetMinX(visible) - CGRectGetMinX(frame);
  CGFloat top = CGRectGetMaxY(frame) - CGRectGetMaxY(visible);
  CGFloat right = CGRectGetMaxX(frame) - CGRectGetMaxX(visible);
  CGFloat bottom = CGRectGetMinY(visible) - CGRectGetMinY(frame);
  const CGFloat epsilon = 0.5;
  if (!isfinite(left) || !isfinite(top) || !isfinite(right) || !isfinite(bottom) ||
      left < -epsilon || top < -epsilon || right < -epsilon || bottom < -epsilon) {
    return PFVisibleWorkAreaGeometryInvalid;
  }
  left = left < 0 ? 0 : left;
  top = top < 0 ? 0 : top;
  right = right < 0 ? 0 : right;
  bottom = bottom < 0 ? 0 : bottom;

  CGRect resolved = CGRectMake(
    CGRectGetMinX(cgBounds) + left,
    CGRectGetMinY(cgBounds) + top,
    cgBounds.size.width - left - right,
    cgBounds.size.height - top - bottom
  );
  if (!PFRectIsFiniteAndUsable(resolved) ||
      CGRectGetMinX(resolved) < CGRectGetMinX(cgBounds) - epsilon ||
      CGRectGetMinY(resolved) < CGRectGetMinY(cgBounds) - epsilon ||
      CGRectGetMaxX(resolved) > CGRectGetMaxX(cgBounds) + epsilon ||
      CGRectGetMaxY(resolved) > CGRectGetMaxY(cgBounds) + epsilon) {
    return PFVisibleWorkAreaGeometryInvalid;
  }

  if (selectedDisplayID != NULL) {
    *selectedDisplayID = targetDisplay->displayID;
  }
  if (visibleWorkArea != NULL) {
    *visibleWorkArea = resolved;
  }
  return PFVisibleWorkAreaSuccess;
}

static BOOL PFBoundsMatch(CGRect first, CGRect second) {
  // CGWindow bounds and AX position/size are both global display points,
  // including on Retina displays. No backing-scale conversion is valid here.
  // Preview may expose the AX content/window frame with a small decoration
  // delta. Keep a tight point tolerance and retain unique-match fail-closed.
  static const CGFloat kBoundsTolerance = 2.0;
  return fabs(first.origin.x - second.origin.x) <= kBoundsTolerance &&
    fabs(first.origin.y - second.origin.y) <= kBoundsTolerance &&
    fabs(first.size.width - second.size.width) <= kBoundsTolerance &&
    fabs(first.size.height - second.size.height) <= kBoundsTolerance;
}

static PFSelectionWindowFreezeStatus PFValidateWindowMetadataSnapshots(
  CGWindowID frozenWindowID,
  pid_t targetPID,
  pid_t selfPID,
  const PFWindowMetadataSnapshot *snapshots,
  size_t snapshotCount,
  CGRect *validatedBounds
) {
  if (validatedBounds != NULL) {
    *validatedBounds = CGRectNull;
  }
  if (frozenWindowID == kCGNullWindowID) {
    return PFSelectionWindowFreezeMissingWindowID;
  }
  if (targetPID <= 0) {
    return PFSelectionWindowFreezeInvalidTargetPID;
  }
  if (targetPID == selfPID) {
    return PFSelectionWindowFreezeSelfTarget;
  }

  const PFWindowMetadataSnapshot *match = NULL;
  size_t matchCount = 0;
  for (size_t index = 0; index < snapshotCount; index += 1) {
    if (snapshots[index].windowID == frozenWindowID) {
      match = &snapshots[index];
      matchCount += 1;
    }
  }
  if (matchCount == 0 || match == NULL) {
    return PFSelectionWindowFreezeMetadataMissing;
  }
  if (matchCount != 1) {
    return PFSelectionWindowFreezeMetadataAmbiguous;
  }
  if (match->ownerPID != targetPID) {
    return PFSelectionWindowFreezeMetadataPIDMismatch;
  }
  if (!match->isOnscreen) {
    return PFSelectionWindowFreezeMetadataOffscreen;
  }
  if (match->layer != 0) {
    return PFSelectionWindowFreezeMetadataLayerMismatch;
  }
  if (!PFRectIsFiniteAndUsable(match->bounds)) {
    return PFSelectionWindowFreezeMetadataBoundsInvalid;
  }

  if (validatedBounds != NULL) {
    *validatedBounds = match->bounds;
  }
  return PFSelectionWindowFreezeSuccess;
}

static NSString *PFSelectionWindowFreezeError(PFSelectionWindowFreezeStatus status) {
  switch (status) {
    case PFSelectionWindowFreezeSuccess:
      return @"none";
    case PFSelectionWindowFreezeMissingWindowID:
      return @"frozen_window_id_unavailable";
    case PFSelectionWindowFreezeWindowIDMismatch:
      return @"frozen_window_changed_during_gesture";
    case PFSelectionWindowFreezeInvalidTargetPID:
      return @"target_pid_unavailable";
    case PFSelectionWindowFreezeSelfTarget:
      return @"self_process_filtered";
    case PFSelectionWindowFreezeMetadataMissing:
      return @"frozen_window_metadata_missing";
    case PFSelectionWindowFreezeMetadataAmbiguous:
      return @"frozen_window_metadata_ambiguous";
    case PFSelectionWindowFreezeMetadataPIDMismatch:
      return @"frozen_window_pid_mismatch";
    case PFSelectionWindowFreezeMetadataOffscreen:
      return @"frozen_window_offscreen";
    case PFSelectionWindowFreezeMetadataLayerMismatch:
      return @"frozen_window_layer_mismatch";
    case PFSelectionWindowFreezeMetadataBoundsInvalid:
      return @"frozen_window_bounds_invalid";
    case PFSelectionWindowFreezeAXWindowMissing:
      return @"frozen_ax_window_missing";
    case PFSelectionWindowFreezeAXWindowAmbiguous:
      return @"frozen_ax_window_ambiguous";
    case PFSelectionWindowFreezeGesturePIDMismatch:
      return @"target_pid_changed_during_gesture";
  }
}

static BOOL PFElementBelongsToPID(id element, pid_t targetPID) {
  if (element == nil || CFGetTypeID((__bridge CFTypeRef)element) != AXUIElementGetTypeID()) {
    return NO;
  }

  pid_t elementPID = 0;
  AXError error = AXUIElementGetPid((__bridge AXUIElementRef)element, &elementPID);
  if (error != kAXErrorSuccess || elementPID <= 0 || PFIsSelfPID(elementPID)) {
    PFRecordSelectionAXReadError(error);
    return NO;
  }

  return targetPID <= 0 || elementPID == targetPID;
}

static BOOL PFElementsEqual(id first, id second) {
  if (first == nil || second == nil) {
    return NO;
  }
  return CFEqual((__bridge CFTypeRef)first, (__bridge CFTypeRef)second);
}

static id PFWindowForElement(id element, pid_t targetPID) {
  id window = PFAttribute(element, kAXWindowAttribute);
  return PFElementBelongsToPID(window, targetPID) ? window : nil;
}

static id PFAppElementForPID(pid_t pid) {
  if (pid <= 0 || PFIsSelfPID(pid)) {
    return nil;
  }

  AXUIElementRef element = AXUIElementCreateApplication(pid);
  return CFBridgingRelease(element);
}

static PFSelectionWindowFreezeStatus PFCopyFrozenWindowMetadataBounds(
  CGWindowID frozenWindowID,
  pid_t targetPID,
  CGRect *validatedBounds
) {
  NSArray<NSNumber *> *requestedWindowIDs = @[@(frozenWindowID)];
  CFArrayRef rawWindowInfo = CGWindowListCreateDescriptionFromArray(
    (__bridge CFArrayRef)requestedWindowIDs
  );
  if (rawWindowInfo == NULL) {
    return PFSelectionWindowFreezeMetadataMissing;
  }

  NSArray *windowInfo = CFBridgingRelease(rawWindowInfo);
  NSUInteger count = windowInfo.count;
  PFWindowMetadataSnapshot *snapshots = count > 0
    ? calloc(count, sizeof(PFWindowMetadataSnapshot))
    : NULL;
  if (count > 0 && snapshots == NULL) {
    return PFSelectionWindowFreezeMetadataMissing;
  }

  for (NSUInteger index = 0; index < count; index += 1) {
    id rawEntry = windowInfo[index];
    if (![rawEntry isKindOfClass:[NSDictionary class]]) {
      continue;
    }

    NSDictionary *entry = rawEntry;
    id rawWindowID = entry[(__bridge NSString *)kCGWindowNumber];
    id rawOwnerPID = entry[(__bridge NSString *)kCGWindowOwnerPID];
    id rawLayer = entry[(__bridge NSString *)kCGWindowLayer];
    id rawIsOnscreen = entry[(__bridge NSString *)kCGWindowIsOnscreen];
    id rawBounds = entry[(__bridge NSString *)kCGWindowBounds];

    if ([rawWindowID isKindOfClass:[NSNumber class]]) {
      snapshots[index].windowID = [(NSNumber *)rawWindowID unsignedIntValue];
    }
    if ([rawOwnerPID isKindOfClass:[NSNumber class]]) {
      snapshots[index].ownerPID = [(NSNumber *)rawOwnerPID intValue];
    }
    snapshots[index].layer = [rawLayer isKindOfClass:[NSNumber class]]
      ? [(NSNumber *)rawLayer integerValue]
      : NSIntegerMax;
    snapshots[index].isOnscreen = [rawIsOnscreen isKindOfClass:[NSNumber class]]
      ? [(NSNumber *)rawIsOnscreen boolValue]
      : NO;
    snapshots[index].bounds = CGRectNull;
    if ([rawBounds isKindOfClass:[NSDictionary class]]) {
      (void)CGRectMakeWithDictionaryRepresentation(
        (__bridge CFDictionaryRef)rawBounds,
        &snapshots[index].bounds
      );
    }
  }

  PFSelectionWindowFreezeStatus status = PFValidateWindowMetadataSnapshots(
    frozenWindowID,
    targetPID,
    PFSelfPID(),
    snapshots,
    count,
    validatedBounds
  );
  free(snapshots);
  return status;
}

static BOOL PFAXWindowBounds(id window, CGRect *bounds) {
  if (bounds != NULL) {
    *bounds = CGRectNull;
  }
  id rawPosition = PFAttribute(window, kAXPositionAttribute);
  id rawSize = PFAttribute(window, kAXSizeAttribute);
  if (rawPosition == nil || rawSize == nil ||
      CFGetTypeID((__bridge CFTypeRef)rawPosition) != AXValueGetTypeID() ||
      CFGetTypeID((__bridge CFTypeRef)rawSize) != AXValueGetTypeID()) {
    return NO;
  }

  AXValueRef positionValue = (__bridge AXValueRef)rawPosition;
  AXValueRef sizeValue = (__bridge AXValueRef)rawSize;
  CGPoint position = CGPointZero;
  CGSize size = CGSizeZero;
  if (AXValueGetType(positionValue) != kAXValueCGPointType ||
      AXValueGetType(sizeValue) != kAXValueCGSizeType ||
      !AXValueGetValue(positionValue, kAXValueCGPointType, &position) ||
      !AXValueGetValue(sizeValue, kAXValueCGSizeType, &size)) {
    return NO;
  }

  CGRect result = CGRectMake(position.x, position.y, size.width, size.height);
  if (!PFRectIsFiniteAndUsable(result)) {
    return NO;
  }
  if (bounds != NULL) {
    *bounds = result;
  }
  return YES;
}

static id PFResolveFrozenAXWindow(
  CGWindowID frozenWindowID,
  pid_t targetPID,
  NSString **error
) {
  CGRect frozenBounds = CGRectNull;
  PFSelectionWindowFreezeStatus metadataStatus = PFCopyFrozenWindowMetadataBounds(
    frozenWindowID,
    targetPID,
    &frozenBounds
  );
  if (metadataStatus != PFSelectionWindowFreezeSuccess) {
    if (error != NULL) {
      *error = PFSelectionWindowFreezeError(metadataStatus);
    }
    return nil;
  }

  id appElement = PFAppElementForPID(targetPID);
  id rawWindows = PFAttribute(appElement, kAXWindowsAttribute);
  if (![rawWindows isKindOfClass:[NSArray class]]) {
    if (error != NULL) {
      *error = PFSelectionWindowFreezeError(PFSelectionWindowFreezeAXWindowMissing);
    }
    return nil;
  }

  id matchedWindow = nil;
  NSInteger matchCount = 0;
  for (id candidate in (NSArray *)rawWindows) {
    if (!PFElementBelongsToPID(candidate, targetPID)) {
      continue;
    }
    CGRect candidateBounds = CGRectNull;
    if (PFAXWindowBounds(candidate, &candidateBounds) && PFBoundsMatch(candidateBounds, frozenBounds)) {
      matchedWindow = candidate;
      matchCount += 1;
    }
  }

  if (matchCount == 0 || matchedWindow == nil) {
    if (error != NULL) {
      *error = PFSelectionWindowFreezeError(PFSelectionWindowFreezeAXWindowMissing);
    }
    return nil;
  }
  if (matchCount != 1) {
    if (error != NULL) {
      *error = PFSelectionWindowFreezeError(PFSelectionWindowFreezeAXWindowAmbiguous);
    }
    return nil;
  }
  if (error != NULL) {
    *error = nil;
  }
  return matchedWindow;
}

static id PFFocusedElementForApp(id appElement, pid_t targetPID) {
  id value = PFAttribute(appElement, kAXFocusedUIElementAttribute);
  return PFElementBelongsToPID(value, targetPID) ? value : nil;
}

static id PFElementAtCGPosition(CGPoint position, pid_t targetPID) {
  AXUIElementRef systemWide = AXUIElementCreateSystemWide();
  AXUIElementRef value = NULL;
  AXError error = AXUIElementCopyElementAtPosition(
    systemWide,
    (float)position.x,
    (float)position.y,
    &value
  );
  CFRelease(systemWide);

  if (error != kAXErrorSuccess || value == NULL) {
    PFRecordSelectionAXReadError(error);
    return nil;
  }

  id element = CFBridgingRelease(value);
  return PFElementBelongsToPID(element, targetPID) ? element : nil;
}

static CGPoint PFCurrentCGMouseAnchor(void) {
  CGEventRef currentEvent = CGEventCreate(NULL);
  if (currentEvent == NULL) {
    return CGPointZero;
  }

  CGPoint location = CGEventGetLocation(currentEvent);
  CFRelease(currentEvent);
  return location;
}

static NSArray *PFChildElements(id element, BOOL *complete) {
  if (complete != NULL) {
    *complete = NO;
  }
  if (element == nil) {
    return @[];
  }

  CFTypeRef rawChildren = NULL;
  AXError error = AXUIElementCopyAttributeValue(
    (__bridge AXUIElementRef)element,
    kAXChildrenAttribute,
    &rawChildren
  );
  if (error == kAXErrorNoValue || error == kAXErrorAttributeUnsupported) {
    if (complete != NULL) {
      *complete = YES;
    }
    return @[];
  }
  if (error != kAXErrorSuccess || rawChildren == NULL) {
    PFRecordSelectionAXReadError(error);
    if (rawChildren != NULL) {
      CFRelease(rawChildren);
    }
    return @[];
  }

  id children = CFBridgingRelease(rawChildren);
  if (![children isKindOfClass:[NSArray class]]) {
    return @[];
  }
  NSMutableArray *result = [NSMutableArray array];
  for (id child in (NSArray *)children) {
    if (child == nil ||
        CFGetTypeID((__bridge CFTypeRef)child) != AXUIElementGetTypeID()) {
      return @[];
    }
    [result addObject:child];
  }
  if (complete != NULL) {
    *complete = YES;
  }
  return result;
}

static BOOL PFMayReadFrozenSelectionNode(BOOL pidValid, BOOL ancestryValid) {
  return pidValid && ancestryValid;
}

static NSString *PFSanitizeDiagnosticValue(NSString *value) {
  if (value == nil || value.length == 0) {
    return @"unknown";
  }

  NSMutableString *result = [value mutableCopy];
  [result replaceOccurrencesOfString:@"\t" withString:@" " options:0 range:NSMakeRange(0, result.length)];
  [result replaceOccurrencesOfString:@"\n" withString:@" " options:0 range:NSMakeRange(0, result.length)];
  [result replaceOccurrencesOfString:@"\r" withString:@" " options:0 range:NSMakeRange(0, result.length)];
  return result;
}

static NSString *PFSourceBundleIdentifierForPID(pid_t pid) {
  NSRunningApplication *app = pid > 0
    ? [NSRunningApplication runningApplicationWithProcessIdentifier:pid]
    : nil;
  return app.bundleIdentifier.length > 0 ? app.bundleIdentifier : @"unknown_bundle_id";
}

static NSArray *PFAncestryWithinFrozenWindow(
  id element,
  id targetWindow,
  pid_t targetPID
) {
  if (element == nil || targetWindow == nil) {
    return nil;
  }

  NSMutableArray *ancestry = [NSMutableArray array];
  id current = element;
  NSInteger depth = 0;
  while (current != nil && depth < 16) {
    if (!PFElementBelongsToPID(current, targetPID)) {
      return nil;
    }
    [ancestry addObject:current];
    if (PFElementsEqual(current, targetWindow)) {
      return ancestry;
    }
    current = PFParent(current);
    depth += 1;
  }
  return nil;
}

static BOOL PFElementIsWithinFrozenWindow(id element, id targetWindow, pid_t targetPID) {
  return PFAncestryWithinFrozenWindow(element, targetWindow, targetPID) != nil;
}

static BOOL PFAncestriesShareNearbyIdentity(
  NSArray *firstAncestry,
  NSArray *secondAncestry,
  id targetWindow
) {
  if (firstAncestry == nil || secondAncestry == nil) {
    return NO;
  }
  const NSUInteger maximumNearbyDepth = 2;
  NSUInteger firstLimit = MIN(firstAncestry.count, maximumNearbyDepth);
  NSUInteger secondLimit = MIN(secondAncestry.count, maximumNearbyDepth);
  for (NSUInteger firstIndex = 0; firstIndex < firstLimit; firstIndex += 1) {
    id first = firstAncestry[firstIndex];
    if (PFElementsEqual(first, targetWindow)) {
      break;
    }
    for (NSUInteger secondIndex = 0; secondIndex < secondLimit; secondIndex += 1) {
      id second = secondAncestry[secondIndex];
      if (PFElementsEqual(second, targetWindow)) {
        break;
      }
      if (PFElementsEqual(first, second)) {
        return YES;
      }
    }
  }
  return NO;
}

static id PFResolveAnchoredAXWindow(
  pid_t targetPID,
  CGPoint anchor,
  id *anchoredElement,
  NSString **error
) {
  if (anchoredElement != NULL) {
    *anchoredElement = nil;
  }
  if (error != NULL) {
    *error = nil;
  }
  if (targetPID <= 0 || PFIsSelfPID(targetPID) || !PFPointIsFinite(anchor)) {
    if (error != NULL) {
      *error = @"anchored_ax_context_invalid";
    }
    return nil;
  }
  if (PFFrontmostPID() != targetPID) {
    if (error != NULL) {
      *error = @"anchored_target_not_frontmost";
    }
    return nil;
  }

  id element = PFElementAtCGPosition(anchor, targetPID);
  if (!PFElementBelongsToPID(element, targetPID)) {
    if (error != NULL) {
      *error = @"anchored_ax_element_unavailable";
    }
    return nil;
  }

  id window = PFWindowForElement(element, targetPID);
  if (window == nil) {
    id focusedWindow = PFFocusedWindow(PFAppElementForPID(targetPID));
    if (PFElementBelongsToPID(focusedWindow, targetPID) &&
        PFElementIsWithinFrozenWindow(element, focusedWindow, targetPID)) {
      window = focusedWindow;
    }
  }
  CGRect windowBounds = CGRectNull;
  if (!PFElementBelongsToPID(window, targetPID) ||
      !PFElementIsWithinFrozenWindow(element, window, targetPID) ||
      !PFAXWindowBounds(window, &windowBounds) ||
      !PFRectClearlyContainsPoint(windowBounds, anchor)) {
    if (error != NULL) {
      *error = @"anchored_ax_window_unavailable";
    }
    return nil;
  }

  if (anchoredElement != NULL) {
    *anchoredElement = element;
  }
  return window;
}

static void PFSetSelectionBaselineError(
  NSString **error,
  NSString *phase,
  AXError axError
) {
  if (error == NULL || *error != nil) {
    return;
  }
  *error = axError == kAXErrorSuccess
    ? [NSString stringWithFormat:@"selection_baseline_%@", phase]
    : [NSString stringWithFormat:
        @"selection_baseline_%@_%d",
        phase,
        (int)axError
      ];
}

static BOOL PFSelectionBaselineTimeoutWasAccepted(AXError error) {
  return error == kAXErrorSuccess;
}

static NSTimeInterval PFSelectionBaselineRemainingBudget(
  NSTimeInterval deadline,
  NSTimeInterval now
) {
  if (!isfinite(deadline) || !isfinite(now) || deadline <= now) {
    return 0;
  }
  return deadline - now;
}

static float PFSelectionBaselineOperationTimeoutForRemaining(
  NSTimeInterval remaining
) {
  if (!isfinite(remaining) || remaining <= 0) {
    return 0;
  }
  return (float)MIN(
    (NSTimeInterval)kSelectionBaselineAXTimeoutSeconds,
    remaining
  );
}

static BOOL PFSelectionBaselineMayAttemptSupplementalFocus(
  NSTimeInterval remaining
) {
  return isfinite(remaining) &&
    remaining >= kSelectionBaselineSupplementalMinimumBudgetSeconds;
}

static BOOL PFSetSelectionBaselineTimeout(
  AXUIElementRef element,
  float timeout,
  NSString *phase,
  NSString **error
) {
  if (element == NULL) {
    PFSetSelectionBaselineError(error, @"timeout_element_invalid", kAXErrorIllegalArgument);
    return NO;
  }
  AXError timeoutError = AXUIElementSetMessagingTimeout(element, timeout);
  if (!PFSelectionBaselineTimeoutWasAccepted(timeoutError)) {
    PFSetSelectionBaselineError(error, phase, timeoutError);
    return NO;
  }
  return YES;
}

static BOOL PFPrepareSelectionBaselineAXOperation(
  AXUIElementRef element,
  NSTimeInterval deadline,
  NSString *phase,
  NSString **error
) {
  NSTimeInterval remaining = PFSelectionBaselineRemainingBudget(
    deadline,
    PFMonotonicUptime()
  );
  float timeout = PFSelectionBaselineOperationTimeoutForRemaining(remaining);
  if (timeout <= 0) {
    PFSetSelectionBaselineError(error, @"budget_exceeded", kAXErrorCannotComplete);
    return NO;
  }
  return PFSetSelectionBaselineTimeout(element, timeout, phase, error);
}

static BOOL PFSelectionBaselineOperationFinishedBeforeDeadline(
  NSTimeInterval deadline,
  NSString **error
) {
  if (PFSelectionBaselineRemainingBudget(deadline, PFMonotonicUptime()) > 0) {
    return YES;
  }
  PFSetSelectionBaselineError(error, @"budget_exceeded", kAXErrorCannotComplete);
  return NO;
}

static BOOL PFRestoreSelectionBaselineTimeout(
  AXUIElementRef element,
  NSString *phase,
  NSString **error
) {
  return PFSetSelectionBaselineTimeout(
    element,
    kAXMessagingTimeoutSeconds,
    phase,
    error
  );
}

static BOOL PFRecoverSelectionApplicationTimeout(pid_t targetPID) {
  id recoveryElement = PFAppElementForPID(targetPID);
  if (recoveryElement == nil) {
    return NO;
  }
  return AXUIElementSetMessagingTimeout(
    (__bridge AXUIElementRef)recoveryElement,
    kAXMessagingTimeoutSeconds
  ) == kAXErrorSuccess;
}

static id PFCopySelectionBaselineAnchorElement(
  CGPoint anchor,
  NSTimeInterval deadline,
  NSString **error
) {
  if (!PFPointIsFinite(anchor)) {
    PFSetSelectionBaselineError(error, @"anchor_invalid", kAXErrorSuccess);
    return nil;
  }
  AXUIElementRef systemWide = AXUIElementCreateSystemWide();
  if (systemWide == NULL) {
    PFSetSelectionBaselineError(error, @"system_wide_unavailable", kAXErrorFailure);
    return nil;
  }
  if (!PFPrepareSelectionBaselineAXOperation(
        systemWide,
        deadline,
        @"system_timeout_setup_failed",
        error
      )) {
    CFRelease(systemWide);
    return nil;
  }

  AXUIElementRef rawElement = NULL;
  AXError readError = AXUIElementCopyElementAtPosition(
    systemWide,
    (float)anchor.x,
    (float)anchor.y,
    &rawElement
  );
  BOOL finishedBeforeDeadline =
    PFSelectionBaselineOperationFinishedBeforeDeadline(deadline, error);
  BOOL restored = PFRestoreSelectionBaselineTimeout(
    systemWide,
    @"system_timeout_restore_failed",
    error
  );
  if (!restored &&
      !PFSetGlobalAXMessagingTimeout(kAXMessagingTimeoutSeconds)) {
    // A failed restore on the system-wide element may leave this process at
    // the 3 ms baseline timeout. A fresh system-wide element is the recovery
    // path; if that also fails, no later AX read is allowed to continue.
    gSelectionAXMessagingTimeoutHealthy = NO;
  }
  CFRelease(systemWide);
  if (!restored || !finishedBeforeDeadline ||
      readError != kAXErrorSuccess || rawElement == NULL) {
    if (readError != kAXErrorSuccess || rawElement == NULL) {
      PFSetSelectionBaselineError(
        error,
        @"anchor_read_failed",
        readError == kAXErrorSuccess ? kAXErrorFailure : readError
      );
    }
    if (rawElement != NULL) {
      CFRelease(rawElement);
    }
    return nil;
  }
  return CFBridgingRelease(rawElement);
}

static id PFCopySelectionBaselineFocusedElement(
  id appElement,
  pid_t targetPID,
  NSTimeInterval deadline,
  NSString **error
) {
  if (appElement == nil ||
      CFGetTypeID((__bridge CFTypeRef)appElement) != AXUIElementGetTypeID()) {
    PFSetSelectionBaselineError(error, @"app_element_invalid", kAXErrorIllegalArgument);
    return nil;
  }
  AXUIElementRef app = (__bridge AXUIElementRef)appElement;
  if (!PFPrepareSelectionBaselineAXOperation(
        app,
        deadline,
        @"app_timeout_setup_failed",
        error
      )) {
    return nil;
  }

  CFTypeRef rawFocused = NULL;
  AXError readError = AXUIElementCopyAttributeValue(
    app,
    kAXFocusedUIElementAttribute,
    &rawFocused
  );
  BOOL finishedBeforeDeadline =
    PFSelectionBaselineOperationFinishedBeforeDeadline(deadline, error);
  BOOL restored = PFRestoreSelectionBaselineTimeout(
    app,
    @"app_timeout_restore_failed",
    error
  );
  if (!restored || !finishedBeforeDeadline) {
    if (!restored && !PFRecoverSelectionApplicationTimeout(targetPID)) {
      gSelectionAXMessagingTimeoutHealthy = NO;
    }
    if (rawFocused != NULL) {
      CFRelease(rawFocused);
    }
    return nil;
  }
  if (PFSelectionAttributeErrorIsSafeAbsence(readError) ||
      (readError == kAXErrorSuccess && rawFocused == NULL)) {
    if (rawFocused != NULL) {
      CFRelease(rawFocused);
    }
    return nil;
  }
  if (readError != kAXErrorSuccess || rawFocused == NULL ||
      CFGetTypeID(rawFocused) != AXUIElementGetTypeID()) {
    PFSetSelectionBaselineError(
      error,
      @"focused_read_failed",
      readError == kAXErrorSuccess ? kAXErrorFailure : readError
    );
    if (rawFocused != NULL) {
      CFRelease(rawFocused);
    }
    return nil;
  }
  return CFBridgingRelease(rawFocused);
}

static BOOL PFSelectionBaselineErrorIsBudgetExhaustion(NSString **error) {
  return error != NULL &&
    [*error hasPrefix:@"selection_baseline_budget_exceeded"];
}

static BOOL PFSelectionBaselineMayKeepPartial(
  BOOL allowPartial,
  NSUInteger previouslyCapturedCount,
  BOOL currentValueComplete,
  BOOL budgetExhausted
) {
  return allowPartial &&
    budgetExhausted &&
    (previouslyCapturedCount > 0 || currentValueComplete);
}

static void PFAppendSelectionBaselineValue(
  id element,
  NSString *text,
  NSMutableArray *capturedElements,
  NSMutableArray *capturedTexts
) {
  for (id existing in capturedElements) {
    if (PFElementsEqual(existing, element)) {
      return;
    }
  }
  [capturedElements addObject:element];
  [capturedTexts addObject:text ?: NSNull.null];
}

static BOOL PFCaptureSelectionBaselineAncestry(
  id seedElement,
  pid_t targetPID,
  NSInteger maximumDepth,
  NSTimeInterval deadline,
  BOOL allowPartialOnBudgetExhaustion,
  NSMutableArray *capturedElements,
  NSMutableArray *capturedTexts,
  NSString **error
) {
  id current = seedElement;
  NSInteger depth = 0;
  while (current != nil && depth < maximumDepth) {
    if (CFGetTypeID((__bridge CFTypeRef)current) != AXUIElementGetTypeID()) {
      PFSetSelectionBaselineError(error, @"ancestry_element_invalid", kAXErrorIllegalArgument);
      return NO;
    }
    AXUIElementRef currentElement = (__bridge AXUIElementRef)current;
    if (!PFPrepareSelectionBaselineAXOperation(
          currentElement,
          deadline,
          @"element_timeout_setup_failed",
        error
      )) {
      if (PFSelectionBaselineMayKeepPartial(
            allowPartialOnBudgetExhaustion,
            capturedElements.count,
            NO,
            PFSelectionBaselineErrorIsBudgetExhaustion(error)
          )) {
        *error = nil;
        return YES;
      }
      return NO;
    }

    pid_t elementPID = 0;
    AXError pidError = AXUIElementGetPid(currentElement, &elementPID);
    BOOL operationsValid =
      PFSelectionBaselineOperationFinishedBeforeDeadline(deadline, error);
    NSString *text = nil;
    PFDirectSelectedTextReadStatus directStatus = PFDirectSelectedTextReadFailure;
    BOOL directValueComplete = NO;
    if (operationsValid &&
        pidError == kAXErrorSuccess &&
        elementPID == targetPID &&
        !PFIsSelfPID(elementPID)) {
      operationsValid = PFPrepareSelectionBaselineAXOperation(
        currentElement,
        deadline,
        @"element_timeout_setup_failed",
        error
      );
      if (operationsValid) {
        directStatus = PFCopyDirectSelectedText(current, &text);
        operationsValid =
          PFSelectionBaselineOperationFinishedBeforeDeadline(deadline, error);
        directValueComplete = operationsValid &&
          directStatus != PFDirectSelectedTextReadFailure;
      }
    }
    id parent = nil;
    AXError parentError = kAXErrorSuccess;
    if (operationsValid &&
        pidError == kAXErrorSuccess &&
        elementPID == targetPID &&
        directStatus != PFDirectSelectedTextReadFailure &&
        depth + 1 < maximumDepth) {
      operationsValid = PFPrepareSelectionBaselineAXOperation(
        currentElement,
        deadline,
        @"element_timeout_setup_failed",
        error
      );
      if (operationsValid) {
        parent = PFAttributeWithError(current, kAXParentAttribute, &parentError);
        operationsValid =
          PFSelectionBaselineOperationFinishedBeforeDeadline(deadline, error);
        if (PFSelectionAttributeErrorIsSafeAbsence(parentError)) {
          parentError = kAXErrorSuccess;
          parent = nil;
        }
      }
    }
    BOOL restored = PFRestoreSelectionBaselineTimeout(
      currentElement,
      @"element_timeout_restore_failed",
      error
    );
    if (!restored) {
      if (!PFRecoverSelectionApplicationTimeout(targetPID)) {
        gSelectionAXMessagingTimeoutHealthy = NO;
      }
      return NO;
    }
    if (!operationsValid) {
      if (PFSelectionBaselineMayKeepPartial(
            allowPartialOnBudgetExhaustion,
            capturedElements.count,
            directValueComplete,
            PFSelectionBaselineErrorIsBudgetExhaustion(error)
          )) {
        if (directValueComplete) {
          PFAppendSelectionBaselineValue(
            current,
            text,
            capturedElements,
            capturedTexts
          );
        }
        if (capturedElements.count > 0) {
          *error = nil;
          return YES;
        }
      }
      return NO;
    }
    if (pidError != kAXErrorSuccess || elementPID != targetPID ||
        PFIsSelfPID(elementPID)) {
      PFSetSelectionBaselineError(
        error,
        @"ancestry_pid_mismatch",
        pidError == kAXErrorSuccess ? kAXErrorInvalidUIElement : pidError
      );
      return NO;
    }
    if (directStatus == PFDirectSelectedTextReadFailure) {
      PFSetSelectionBaselineError(
        error,
        @"direct_read_failed",
        gSelectionLastAXReadError == kAXErrorSuccess
          ? kAXErrorFailure
          : gSelectionLastAXReadError
      );
      return NO;
    }
    if (parentError != kAXErrorSuccess) {
      PFSetSelectionBaselineError(error, @"parent_read_failed", parentError);
      return NO;
    }

    PFAppendSelectionBaselineValue(
      current,
      text,
      capturedElements,
      capturedTexts
    );
    current = parent;
    depth += 1;
  }
  return YES;
}

static BOOL PFCapturePreDeliveryDirectSelectionBaseline(
  CGPoint anchor,
  pid_t *resolvedTargetPID,
  NSArray **baselineElements,
  NSArray **baselineTexts,
  NSString **failureReason
) {
  if (resolvedTargetPID != NULL) {
    *resolvedTargetPID = 0;
  }
  if (baselineElements != NULL) {
    *baselineElements = nil;
  }
  if (baselineTexts != NULL) {
    *baselineTexts = nil;
  }
  if (failureReason != NULL) {
    *failureReason = nil;
  }
  if (!gSelectionAXMessagingTimeoutHealthy) {
    PFSetSelectionBaselineError(
      failureReason,
      @"global_timeout_unavailable",
      kAXErrorCannotComplete
    );
    return NO;
  }
  if (PFSelectionBaselineMaximumAXBudgetSeconds() > 0.05) {
    PFSetSelectionBaselineError(failureReason, @"budget_configuration_invalid", kAXErrorSuccess);
    return NO;
  }
  NSTimeInterval startedAt = PFMonotonicUptime();
  NSTimeInterval deadline =
    startedAt + kSelectionBaselineWallClockBudgetSeconds;
  // The active head-insert tap sees mouse-down before the target app. Freeze
  // the under-pointer ancestry first so a first drag into an unfocused Preview
  // document compares against the same control family used at mouse-up. Merge
  // the old focused ancestry only as a secondary seed for PDFKit variants that
  // expose AXSelectedText on the document rather than the hit-tested leaf.
  id anchorElement = PFCopySelectionBaselineAnchorElement(
    anchor,
    deadline,
    failureReason
  );
  if (anchorElement == nil || (failureReason != NULL && *failureReason != nil)) {
    return NO;
  }
  pid_t targetPID = 0;
  AXError anchorPIDError = AXUIElementGetPid(
    (__bridge AXUIElementRef)anchorElement,
    &targetPID
  );
  if (anchorPIDError != kAXErrorSuccess ||
      targetPID <= 0 ||
      PFIsSelfPID(targetPID)) {
    PFSetSelectionBaselineError(
      failureReason,
      @"anchor_pid_invalid",
      anchorPIDError == kAXErrorSuccess
        ? kAXErrorInvalidUIElement
        : anchorPIDError
    );
    return NO;
  }
  // Preserve the resolved AX owner even if a later ancestry read times out.
  // The caller can still use it as the frozen application identity while the
  // direct-text baseline itself remains unavailable and therefore fail-closed.
  if (resolvedTargetPID != NULL) {
    *resolvedTargetPID = targetPID;
  }
  NSMutableArray *capturedElements = [NSMutableArray array];
  NSMutableArray *capturedTexts = [NSMutableArray array];
  if (!PFCaptureSelectionBaselineAncestry(
        anchorElement,
        targetPID,
        kSelectionBaselineMaximumDepth,
        deadline,
        YES,
        capturedElements,
        capturedTexts,
        failureReason
      )) {
    return NO;
  }

  // Focus is supplemental only. Inactive Preview windows and toolbar fields
  // can make the old focused lookup time out or point at an unrelated control;
  // neither may discard a complete under-pointer snapshot. Capture into
  // temporary arrays and merge only after the entire secondary ancestry is
  // valid and every temporary timeout has been restored.
  NSString *focusedWarning = nil;
  NSTimeInterval remainingForFocus = PFSelectionBaselineRemainingBudget(
    deadline,
    PFMonotonicUptime()
  );
  id appElement = PFSelectionBaselineMayAttemptSupplementalFocus(
      remainingForFocus
    )
    ? PFAppElementForPID(targetPID)
    : nil;
  id focusedElement = appElement == nil
    ? nil
    : PFCopySelectionBaselineFocusedElement(
        appElement,
        targetPID,
        deadline,
        &focusedWarning
      );
  if (!gSelectionAXMessagingTimeoutHealthy) {
    if (failureReason != NULL) {
      *failureReason = focusedWarning ?:
        @"selection_baseline_application_timeout_recovery_failed";
    }
    return NO;
  }
  if (focusedElement != nil) {
    NSMutableArray *focusedElements = [NSMutableArray array];
    NSMutableArray *focusedTexts = [NSMutableArray array];
    NSString *focusedAncestryWarning = nil;
    BOOL focusedCaptured = PFCaptureSelectionBaselineAncestry(
      focusedElement,
      targetPID,
      kSelectionBaselineSupplementalMaximumDepth,
      deadline,
      NO,
      focusedElements,
      focusedTexts,
      &focusedAncestryWarning
    );
    if (!gSelectionAXMessagingTimeoutHealthy) {
      if (failureReason != NULL) {
        *failureReason = focusedAncestryWarning ?:
          @"selection_baseline_application_timeout_recovery_failed";
      }
      return NO;
    }
    if (focusedCaptured) {
      for (NSUInteger index = 0; index < focusedElements.count; index += 1) {
        id candidate = focusedElements[index];
        BOOL alreadyCaptured = NO;
        for (id existing in capturedElements) {
          if (PFElementsEqual(existing, candidate)) {
            alreadyCaptured = YES;
            break;
          }
        }
        if (!alreadyCaptured) {
          [capturedElements addObject:candidate];
          [capturedTexts addObject:focusedTexts[index]];
        }
      }
    }
  }
  if (capturedElements.count == 0 ||
      capturedElements.count != capturedTexts.count) {
    PFSetSelectionBaselineError(failureReason, @"snapshot_empty", kAXErrorSuccess);
    return NO;
  }
  if (baselineElements != NULL) {
    *baselineElements = [capturedElements copy];
  }
  if (baselineTexts != NULL) {
    *baselineTexts = [capturedTexts copy];
  }
  return YES;
}

static BOOL PFSelectionCommitValidationPasses(
  BOOL stateCurrent,
  BOOL requiresAnchorWindowRevalidation,
  BOOL anchoredWindowIdentityCurrent
) {
  return stateCurrent &&
    (!requiresAnchorWindowRevalidation || anchoredWindowIdentityCurrent);
}

static BOOL PFAnchoredAXContextIdentityIsCurrent(
  PFSelectionReadContext *context
) {
  if (context == nil) {
    return NO;
  }
  if (!context.requiresAnchorWindowRevalidation) {
    return YES;
  }
  if (PFFrontmostPID() != context.targetPID) {
    return NO;
  }
  id currentAnchorElement = PFElementAtCGPosition(
    context.anchor,
    context.targetPID
  );
  BOOL withinFrozenWindow =
    PFElementBelongsToPID(currentAnchorElement, context.targetPID) &&
    PFElementIsWithinFrozenWindow(
      currentAnchorElement,
      context.targetWindow,
      context.targetPID
    );
  BOOL frozenAnchorAvailable =
    PFElementBelongsToPID(context.anchorElement, context.targetPID) &&
    PFElementIsWithinFrozenWindow(
      context.anchorElement,
      context.targetWindow,
      context.targetPID
    );
  BOOL exactElementMatch = frozenAnchorAvailable &&
    PFElementsEqual(currentAnchorElement, context.anchorElement);
  NSArray *frozenAncestry = frozenAnchorAvailable
    ? PFAncestryWithinFrozenWindow(
        context.anchorElement,
        context.targetWindow,
        context.targetPID
      )
    : nil;
  NSArray *currentAncestry = withinFrozenWindow
    ? PFAncestryWithinFrozenWindow(
        currentAnchorElement,
        context.targetWindow,
        context.targetPID
      )
    : nil;
  BOOL nearbyAncestryMatch = frozenAnchorAvailable &&
    PFAncestriesShareNearbyIdentity(
      frozenAncestry,
      currentAncestry,
      context.targetWindow
    );
  return PFAnchorIdentityRevalidationPasses(
    withinFrozenWindow,
    frozenAnchorAvailable,
    exactElementMatch,
    nearbyAncestryMatch
  );
}

static BOOL PFAncestryContainsElementBeforeWindow(
  NSArray *ancestry,
  id element,
  id targetWindow
) {
  if (ancestry == nil || element == nil) {
    return NO;
  }
  for (id candidate in ancestry) {
    if (PFElementsEqual(candidate, targetWindow)) {
      break;
    }
    if (PFElementsEqual(candidate, element)) {
      return YES;
    }
  }
  return NO;
}

static BOOL PFAncestriesShareControlSubtree(
  NSArray *firstAncestry,
  NSArray *secondAncestry,
  id targetWindow
) {
  if (firstAncestry == nil || secondAncestry == nil) {
    return NO;
  }
  for (id first in firstAncestry) {
    if (PFElementsEqual(first, targetWindow)) {
      break;
    }
    if (PFAncestryContainsElementBeforeWindow(secondAncestry, first, targetWindow)) {
      return YES;
    }
  }
  return NO;
}

static BOOL PFFocusedElementIsSafeSecondaryPreferred(
  id anchorElement,
  id focusedElement,
  id targetWindow,
  pid_t targetPID,
  CGPoint anchor,
  BOOL *coveredByPrimaryAncestry
) {
  if (coveredByPrimaryAncestry != NULL) {
    *coveredByPrimaryAncestry = NO;
  }
  BOOL belongsToTargetPID = PFElementBelongsToPID(focusedElement, targetPID);
  NSArray *focusedAncestry = belongsToTargetPID
    ? PFAncestryWithinFrozenWindow(focusedElement, targetWindow, targetPID)
    : nil;
  BOOL isWithinFrozenWindow = focusedAncestry != nil;
  BOOL isFrozenWindow = PFElementsEqual(focusedElement, targetWindow);

  NSArray *anchorAncestry = PFElementBelongsToPID(anchorElement, targetPID)
    ? PFAncestryWithinFrozenWindow(anchorElement, targetWindow, targetPID)
    : nil;
  BOOL duplicatesPrimaryAncestry = PFAncestryContainsElementBeforeWindow(
    anchorAncestry,
    focusedElement,
    targetWindow
  );
  if (coveredByPrimaryAncestry != NULL) {
    *coveredByPrimaryAncestry = duplicatesPrimaryAncestry;
  }
  BOOL sharesControlSubtree = PFAncestriesShareControlSubtree(
    anchorAncestry,
    focusedAncestry,
    targetWindow
  );

  CGRect focusedFrame = CGRectNull;
  BOOL frameContainsAnchor = PFPointIsFinite(anchor) &&
    PFAXWindowBounds(focusedElement, &focusedFrame) &&
    CGRectContainsPoint(CGRectInset(focusedFrame, -2.0, -2.0), anchor);

  return PFFocusedPreferredElementCanBeUsed(
    belongsToTargetPID,
    isWithinFrozenWindow,
    isFrozenWindow,
    duplicatesPrimaryAncestry,
    sharesControlSubtree,
    frameContainsAnchor
  );
}

static BOOL PFAXElementArrayContains(NSArray *elements, id candidate) {
  for (id existing in elements) {
    if (PFElementsEqual(existing, candidate)) {
      return YES;
    }
  }
  return NO;
}

static NSString *PFSelectedTextFromPreferredElement(
  id element,
  id targetWindow,
  pid_t targetPID,
  BOOL requiresSelectionChangeEvidence,
  BOOL selectionChangeObserved,
  BOOL baselineCaptured,
  NSArray *baselineElements,
  NSArray *baselineTexts,
  BOOL *readFailed
) {
  if (readFailed != NULL) {
    *readFailed = NO;
  }
  NSArray *ancestry = PFAncestryWithinFrozenWindow(element, targetWindow, targetPID);
  if (ancestry == nil) {
    if (readFailed != NULL) {
      *readFailed = YES;
    }
    return nil;
  }

  // The point-resolved element and its containing controls own the mouse-up
  // anchor. Never treat the window root itself as a preferred text control.
  for (id current in ancestry) {
    if (PFElementsEqual(current, targetWindow)) {
      break;
    }
    if (!PFElementBelongsToPID(current, targetPID) ||
        !PFElementIsWithinFrozenWindow(current, targetWindow, targetPID)) {
      if (readFailed != NULL) {
        *readFailed = YES;
      }
      return nil;
    }
    BOOL currentReadFailed = NO;
    NSString *text = PFSelectedText(
      current,
      requiresSelectionChangeEvidence,
      selectionChangeObserved,
      baselineCaptured,
      baselineElements,
      baselineTexts,
      &currentReadFailed
    );
    if (!PFStringIsBlank(text)) {
      return text;
    }
    if (!PFPreferredSelectionShouldContinueToParent(NO, currentReadFailed)) {
      if (readFailed != NULL) {
        *readFailed = YES;
      }
      return nil;
    }
  }
  return nil;
}

static NSString *PFSelectedTextFromAnchoredFocusedElement(
  id element,
  id targetWindow,
  pid_t targetPID,
  CGPoint anchor,
  BOOL *readFailed
) {
  if (readFailed != NULL) {
    *readFailed = NO;
  }
  NSArray *ancestry = PFAncestryWithinFrozenWindow(element, targetWindow, targetPID);
  if (ancestry == nil) {
    if (readFailed != NULL) {
      *readFailed = YES;
    }
    return nil;
  }

  for (id current in ancestry) {
    if (PFElementsEqual(current, targetWindow)) {
      break;
    }
    NSString *text = nil;
    PFDirectSelectedTextReadStatus status = PFCopyAnchoredFocusedSelectedText(
      current,
      anchor,
      &text
    );
    if (status == PFDirectSelectedTextReadText && !PFStringIsBlank(text)) {
      return text;
    }
    if (status == PFDirectSelectedTextReadFailure) {
      if (readFailed != NULL) {
        *readFailed = YES;
      }
      return nil;
    }
  }
  return nil;
}

static NSString *PFSelectedTextBySearchingChildrenForAnchor(
  id root,
  id targetWindow,
  pid_t targetPID,
  CGPoint anchor,
  NSInteger *candidateCount,
  NSString **error
) {
  if (candidateCount != NULL) {
    *candidateCount = 0;
  }
  if (error != NULL) {
    *error = nil;
  }
  if (root == nil || !PFPointIsFinite(anchor)) {
    if (error != NULL) {
      *error = @"selection_anchor_invalid";
    }
    return nil;
  }

  NSMutableArray *elements = [NSMutableArray arrayWithObject:root];
  NSMutableArray<NSNumber *> *depths = [NSMutableArray arrayWithObject:@0];
  NSMutableArray *visitedElements = [NSMutableArray array];
  NSMutableArray *candidateGroupElements = [NSMutableArray array];
  NSMutableArray *candidateRanges = [NSMutableArray array];
  NSMutableArray<NSNumber *> *candidateGroupIndices = [NSMutableArray array];
  NSMutableArray<NSValue *> *candidateBounds = [NSMutableArray array];
  NSMutableArray<NSNumber *> *candidateAvailability = [NSMutableArray array];
  NSInteger visited = 0;
  BOOL traversalComplete = YES;
  NSString *traversalError = nil;
  const NSInteger maximumDepth = 15;
  const NSInteger maximumVisited = 160;

  while (elements.count > 0) {
    if (visited >= maximumVisited) {
      traversalComplete = NO;
      break;
    }
    id element = elements.firstObject;
    NSInteger depth = depths.firstObject.integerValue;
    [elements removeObjectAtIndex:0];
    [depths removeObjectAtIndex:0];
    if (PFAXElementArrayContains(visitedElements, element)) {
      continue;
    }
    [visitedElements addObject:element];
    visited += 1;

    BOOL pidValid = PFElementBelongsToPID(element, targetPID);
    NSArray *ancestry = pidValid
      ? PFAncestryWithinFrozenWindow(element, targetWindow, targetPID)
      : nil;
    BOOL ancestryValid = ancestry != nil;
    if (!PFMayReadFrozenSelectionNode(pidValid, ancestryValid)) {
      continue;
    }

    // One AX element plus all of its non-empty selected ranges is one candidate.
    // Bounds choose the element group; text is read only after that group wins.
    BOOL rangesValid = YES;
    AXError rangesError = kAXErrorSuccess;
    NSArray *selectedRanges = PFNonEmptySelectedRangeValues(
      element,
      &rangesValid,
      &rangesError
    );
    if (!rangesValid || rangesError != kAXErrorSuccess) {
      traversalComplete = NO;
      traversalError = rangesError == kAXErrorCannotComplete
        ? @"ax_timeout"
        : @"selection_ranges_invalid";
      break;
    }
    if (selectedRanges.count > 0) {
      size_t groupIndex = candidateGroupElements.count;
      [candidateGroupElements addObject:element];
      for (id rawRange in selectedRanges) {
        CGRect bounds = CGRectNull;
        BOOL available = PFBoundsForSelectedRange(element, rawRange, &bounds);
        [candidateRanges addObject:rawRange];
        [candidateGroupIndices addObject:@(groupIndex)];
        [candidateBounds addObject:[NSValue valueWithRect:NSRectFromCGRect(bounds)]];
        [candidateAvailability addObject:@(available)];
      }
    }

    BOOL childrenComplete = NO;
    NSArray *children = PFChildElements(element, &childrenComplete);
    if (!childrenComplete) {
      traversalComplete = NO;
      break;
    }
    if (children.count > 0 && depth >= maximumDepth) {
      traversalComplete = NO;
      break;
    }
    if (depth < maximumDepth) {
      for (id child in children) {
        if (!PFAXElementArrayContains(visitedElements, child)) {
          [elements addObject:child];
          [depths addObject:@(depth + 1)];
        }
      }
    }
  }

  if (!traversalComplete) {
    if (error != NULL) {
      *error = traversalError ?: (gSelectionLastAXReadError == kAXErrorCannotComplete
        ? @"ax_timeout"
        : @"selection_candidate_scan_incomplete");
    }
    return nil;
  }

  if (candidateCount != NULL) {
    *candidateCount = candidateGroupElements.count;
  }
  if (candidateGroupElements.count == 0) {
    if (error != NULL) {
      *error = @"no_selected_text";
    }
    return nil;
  }

  size_t count = candidateRanges.count;
  size_t groupCount = candidateGroupElements.count;
  CGRect *rawBounds = calloc(count, sizeof(CGRect));
  BOOL *rawAvailability = calloc(count, sizeof(BOOL));
  size_t *rawGroupIndices = calloc(count, sizeof(size_t));
  if (rawBounds == NULL || rawAvailability == NULL || rawGroupIndices == NULL) {
    free(rawBounds);
    free(rawAvailability);
    free(rawGroupIndices);
    if (error != NULL) {
      *error = @"selection_bounds_allocation_failed";
    }
    return nil;
  }
  for (size_t index = 0; index < count; index += 1) {
    rawBounds[index] = NSRectToCGRect(candidateBounds[index].rectValue);
    rawAvailability[index] = candidateAvailability[index].boolValue;
    rawGroupIndices[index] = candidateGroupIndices[index].unsignedLongLongValue;
  }

  size_t selectedGroupIndex = SIZE_MAX;
  PFSelectionAnchorResolutionStatus resolution = PFResolveSelectionAnchorCandidate(
    anchor,
    rawBounds,
    rawAvailability,
    rawGroupIndices,
    count,
    groupCount,
    &selectedGroupIndex
  );
  free(rawBounds);
  free(rawAvailability);
  free(rawGroupIndices);

  if (resolution != PFSelectionAnchorResolutionSuccess ||
      selectedGroupIndex >= groupCount) {
    if (error != NULL) {
      switch (resolution) {
        case PFSelectionAnchorResolutionNoCandidate:
          *error = @"selection_anchor_not_in_selected_bounds";
          break;
        case PFSelectionAnchorResolutionAmbiguous:
          *error = @"selection_bounds_ambiguous";
          break;
        case PFSelectionAnchorResolutionBoundsUnavailable:
          *error = @"selection_bounds_unavailable";
          break;
        case PFSelectionAnchorResolutionInvalidInput:
          *error = @"selection_bounds_invalid";
          break;
        case PFSelectionAnchorResolutionSuccess:
          *error = @"selection_candidate_invalid";
          break;
      }
    }
    return nil;
  }

  id selectedElement = candidateGroupElements[selectedGroupIndex];
  BOOL pidValid = PFElementBelongsToPID(selectedElement, targetPID);
  BOOL ancestryValid = pidValid &&
    PFElementIsWithinFrozenWindow(selectedElement, targetWindow, targetPID);
  if (!PFMayReadFrozenSelectionNode(pidValid, ancestryValid)) {
    if (error != NULL) {
      *error = @"selection_candidate_identity_changed";
    }
    return nil;
  }

  NSMutableArray *selectedRanges = [NSMutableArray array];
  for (size_t index = 0; index < count; index += 1) {
    if (candidateGroupIndices[index].unsignedLongLongValue == selectedGroupIndex) {
      [selectedRanges addObject:candidateRanges[index]];
    }
  }
  NSString *text = PFStringForStableSelectedRangeValues(
    selectedElement,
    selectedRanges
  );
  if (PFStringIsBlank(text) && error != NULL) {
    *error = @"no_selected_text";
  }
  return PFStringIsBlank(text) ? nil : text;
}

static NSString *PFSelectedTextForContext(
  PFSelectionReadContext *context,
  NSInteger *candidateCount,
  NSString **axError
) {
  if (context.targetWindowID != kCGNullWindowID) {
    NSString *resolveError = nil;
    id currentFrozenWindow = PFResolveFrozenAXWindow(
      context.targetWindowID,
      context.targetPID,
      &resolveError
    );
    if (currentFrozenWindow == nil) {
      if (axError != NULL) {
        *axError = resolveError ?: @"frozen_ax_window_missing";
      }
      return nil;
    }
    if (!PFElementsEqual(currentFrozenWindow, context.targetWindow)) {
      if (axError != NULL) {
        *axError = @"frozen_ax_window_identity_changed";
      }
      return nil;
    }
  }
  if (context.requiresAnchorWindowRevalidation &&
      !PFAnchoredAXContextIdentityIsCurrent(context)) {
    if (axError != NULL) {
      *axError = PFFrontmostPID() == context.targetPID
        ? @"anchored_ax_window_identity_changed"
        : @"anchored_target_not_frontmost";
    }
    return nil;
  }

  BOOL hadFrozenElement = context.targetElement != nil;
  BOOL hadSecondaryPreferredElement = context.secondaryPreferredElement != nil;
  BOOL hadFrozenWindow = context.targetWindow != nil;
  BOOL frozenElementValid = PFElementBelongsToPID(context.targetElement, context.targetPID);
  BOOL secondaryPreferredElementValid = PFElementBelongsToPID(
    context.secondaryPreferredElement,
    context.targetPID
  );
  BOOL frozenWindowValid = PFElementBelongsToPID(context.targetWindow, context.targetPID);
  NSString *directBaselineFailure =
    context.requiresDirectSelectionChangeEvidence &&
    !context.directSelectionChangeObserved &&
    !context.directSelectionBaselineCaptured &&
    context.directSelectionBaselineFailureReason.length > 0
      ? context.directSelectionBaselineFailureReason
      : nil;

  if (hadFrozenElement && !frozenElementValid) {
    if (axError != NULL) {
      *axError = @"frozen_target_element_invalid";
    }
    return nil;
  }
  if (hadFrozenWindow && !frozenWindowValid) {
    if (axError != NULL) {
      *axError = @"frozen_target_window_invalid";
    }
    return nil;
  }
  if (hadSecondaryPreferredElement && !secondaryPreferredElementValid) {
    if (axError != NULL) {
      *axError = @"frozen_secondary_element_invalid";
    }
    return nil;
  }
  if (!frozenWindowValid) {
    if (axError != NULL) {
      *axError = @"frozen_target_window_unavailable";
    }
    return nil;
  }
  if (frozenElementValid &&
      !PFElementIsWithinFrozenWindow(
        context.targetElement,
        context.targetWindow,
        context.targetPID
      )) {
    if (axError != NULL) {
      *axError = @"frozen_target_element_window_mismatch";
    }
    return nil;
  }
  if (secondaryPreferredElementValid &&
      !PFElementIsWithinFrozenWindow(
        context.secondaryPreferredElement,
        context.targetWindow,
        context.targetPID
      )) {
    if (axError != NULL) {
      *axError = @"frozen_secondary_element_window_mismatch";
    }
    return nil;
  }

  if (context.hasPreferredElement && frozenElementValid) {
    BOOL preferredReadFailed = NO;
    NSString *preferredText = context.primaryPreferredElementRequiresAnchorProof
      ? PFSelectedTextFromAnchoredFocusedElement(
          context.targetElement,
          context.targetWindow,
          context.targetPID,
          context.anchor,
          &preferredReadFailed
        )
      : PFSelectedTextFromPreferredElement(
          context.targetElement,
          context.targetWindow,
          context.targetPID,
          context.requiresDirectSelectionChangeEvidence,
          context.directSelectionChangeObserved,
          context.directSelectionBaselineCaptured,
          context.directSelectionBaselineElements,
          context.directSelectionBaselineTexts,
          &preferredReadFailed
        );
    if (preferredReadFailed) {
      if (axError != NULL) {
        *axError = gSelectionLastAXReadError == kAXErrorCannotComplete
          ? @"ax_timeout"
          : (directBaselineFailure ?: @"preferred_selection_read_invalid");
      }
      return nil;
    }
    if (!PFStringIsBlank(preferredText)) {
      if (candidateCount != NULL) {
        *candidateCount = 1;
      }
      return preferredText;
    }
  }

  if (context.hasSecondaryPreferredElement && secondaryPreferredElementValid) {
    BOOL secondaryReadFailed = NO;
    NSString *secondaryText = PFSelectedTextFromAnchoredFocusedElement(
      context.secondaryPreferredElement,
      context.targetWindow,
      context.targetPID,
      context.anchor,
      &secondaryReadFailed
    );
    if (secondaryReadFailed) {
      if (axError != NULL) {
        *axError = gSelectionLastAXReadError == kAXErrorCannotComplete
          ? @"ax_timeout"
          : @"secondary_preferred_selection_read_invalid";
      }
      return nil;
    }
    if (!PFStringIsBlank(secondaryText)) {
      if (candidateCount != NULL) {
        *candidateCount = 1;
      }
      return secondaryText;
    }
  }

  NSInteger resolvedCandidateCount = 0;
  NSString *selectionError = nil;
  NSString *text = PFSelectedTextBySearchingChildrenForAnchor(
    context.targetWindow,
    context.targetWindow,
    context.targetPID,
    context.anchor,
    &resolvedCandidateCount,
    &selectionError
  );
  if (candidateCount != NULL) {
    *candidateCount = resolvedCandidateCount;
  }
  if (PFStringIsBlank(text) && axError != NULL) {
    NSString *resolvedError = selectionError ?: @"no_selected_text";
    *axError = directBaselineFailure != nil &&
        [resolvedError isEqualToString:@"no_selected_text"]
      ? directBaselineFailure
      : resolvedError;
  }
  return PFStringIsBlank(text) ? nil : text;
}

static void PFEmitEmptySelectionStatusIfNeeded(void) {
  if (!gSelectionWatcherEnabled) {
    return;
  }
  NSTimeInterval now = [NSDate date].timeIntervalSince1970;
  if (now - gSelectionLastEmptyStatusAt >= 1.5) {
    PFEmit(PaperFloatEventSelectionStatus, @"selection_empty", 0, 0, 0);
    gSelectionLastEmptyStatusAt = now;
  }
}

static void PFEmitSelectionReadDiagnostic(
  NSString *status,
  NSString *reason,
  BOOL foundText,
  NSInteger candidateCount,
  NSTimeInterval durationMs,
  NSString *sourceBundleIdentifier,
  NSString *axError,
  long long generation,
  NSInteger attempt,
  NSTimeInterval triggerToReadMs,
  BOOL terminal,
  pid_t targetPID
) {
  NSString *payload = [NSString stringWithFormat:
    @"%@\t%@\t%@\t%ld\t%.1f\t%@\t%@\t%lld\t%ld\t%.1f\t%@",
    status,
    PFSanitizeDiagnosticValue(reason),
    foundText ? @"true" : @"false",
    (long)candidateCount,
    durationMs,
    PFSanitizeDiagnosticValue(sourceBundleIdentifier),
    PFSanitizeDiagnosticValue(axError),
    generation,
    (long)attempt,
    triggerToReadMs,
    terminal ? @"true" : @"false"
  ];
  PFEmitForTargetPID(PaperFloatEventSelectionStatus, payload, 0, 0, 0, targetPID);
}

static long long PFAdvanceCounter(long long *counter) {
  *counter = *counter == LLONG_MAX ? 1 : *counter + 1;
  return *counter;
}

static void PFCancelProvisionalDouble(void) {
  PFAdvanceCounter(&gSelectionProvisionalDoubleToken);
  gSelectionProvisionalDoubleGeneration = 0;
  gSelectionProvisionalDoublePending = NO;
}

static BOOL PFSelectionGenerationIsProvisionalDouble(long long generation) {
  return gSelectionProvisionalDoublePending &&
    generation > 0 &&
    generation == gSelectionProvisionalDoubleGeneration;
}

static void PFArmProvisionalDoubleCommit(
  long long generation,
  NSTimeInterval quietWindow,
  dispatch_block_t commit
) {
  PFCancelProvisionalDouble();
  if (generation <= 0 ||
      commit == nil ||
      !isfinite(quietWindow) ||
      quietWindow <= 0 ||
      quietWindow > kSelectionMultiClickMaximumQuietWindow) {
    return;
  }
  gSelectionProvisionalDoubleGeneration = generation;
  gSelectionProvisionalDoublePending = YES;
  long long token = gSelectionProvisionalDoubleToken;
  NSTimeInterval maximumDispatchDelay = (NSTimeInterval)LLONG_MAX /
    (NSTimeInterval)NSEC_PER_SEC;
  NSTimeInterval boundedDelay = quietWindow > maximumDispatchDelay
    ? maximumDispatchDelay
    : quietWindow;
  int64_t delayNanoseconds = boundedDelay >= maximumDispatchDelay
    ? INT64_MAX
    : (int64_t)llround(boundedDelay * NSEC_PER_SEC);
  dispatch_after(
    dispatch_time(DISPATCH_TIME_NOW, delayNanoseconds),
    dispatch_get_main_queue(),
    ^{
      if (!PFSelectionGenerationIsProvisionalDouble(generation) ||
          token != gSelectionProvisionalDoubleToken ||
          generation != gSelectionProvisionalDoubleGeneration) {
        return;
      }
      gSelectionProvisionalDoublePending = NO;
      gSelectionProvisionalDoubleGeneration = 0;
      commit();
    }
  );
}

static BOOL PFSelectionReadStateIsCurrent(
  long long frozenGeneration,
  long long activeGeneration,
  long long completedGeneration,
  long long frozenLifecycle,
  long long activeLifecycle,
  long long frozenReadToken,
  long long activeReadToken
) {
  return frozenGeneration > 0 &&
    frozenGeneration == activeGeneration &&
    frozenGeneration != completedGeneration &&
    frozenLifecycle == activeLifecycle &&
    frozenReadToken == activeReadToken;
}

static BOOL PFObserverFrozenIdentityMatches(
  BOOL contextAvailable,
  BOOL contextInvalid,
  long long contextGeneration,
  long long activeGeneration,
  long long lastMouseGeneration,
  pid_t contextPID,
  pid_t observerPID,
  pid_t lastMousePID,
  CGWindowID contextWindowID,
  CGWindowID lastMouseWindowID,
  BOOL elementWithinFrozenWindow
) {
  BOOL windowIdentityMatches =
    (contextWindowID != kCGNullWindowID &&
     contextWindowID == lastMouseWindowID) ||
    (contextWindowID == kCGNullWindowID &&
     lastMouseWindowID == kCGNullWindowID);
  return contextAvailable &&
    !contextInvalid &&
    contextGeneration > 0 &&
    contextGeneration == activeGeneration &&
    contextGeneration == lastMouseGeneration &&
    contextPID > 0 &&
    contextPID == observerPID &&
    contextPID == lastMousePID &&
    windowIdentityMatches &&
    elementWithinFrozenWindow;
}

static void PFClearFrozenMouseSelectionContext(void) {
  gSelectionLastMouseUpGeneration = 0;
  gSelectionLastMouseUpPID = 0;
  gSelectionLastMouseUpAnchor = CGPointZero;
  gSelectionLastMouseUpAt = 0;
  gSelectionLastMouseUpWindowID = kCGNullWindowID;
  gSelectionLastMouseGestureInvalid = NO;
  gSelectionLastMouseContext = nil;
  gSelectionLastMouseSelectionChangeGeneration = 0;
  gSelectionDirectBaselineGeneration = 0;
  gSelectionDirectBaselineCaptured = NO;
  gSelectionDirectBaselineElements = nil;
  gSelectionDirectBaselineTexts = nil;
  gSelectionDirectBaselineFailureReason = nil;
}

static BOOL PFHasPendingFrozenMouseSelectionAt(NSTimeInterval now) {
  BOOL hasFrozenIdentity = gSelectionLastMouseUpWindowID != kCGNullWindowID ||
    (gSelectionLastMouseUpPID > 0 &&
     !PFIsSelfPID(gSelectionLastMouseUpPID) &&
     PFPointIsFinite(gSelectionLastMouseUpAnchor));
  return gSelectionLastMouseUpGeneration > 0 &&
    gSelectionLastMouseUpGeneration == gSelectionGeneration &&
    gSelectionCompletedGeneration != gSelectionLastMouseUpGeneration &&
    hasFrozenIdentity &&
    !gSelectionLastMouseGestureInvalid &&
    now >= gSelectionLastMouseUpAt &&
    now - gSelectionLastMouseUpAt <= kSelectionFrozenReadLifetime;
}

static BOOL PFShouldPreservePendingFrozenReadOnActivation(
  BOOL hasPendingFrozenRead,
  pid_t activatedPID,
  pid_t frozenMousePID
) {
  return hasPendingFrozenRead &&
    activatedPID > 0 &&
    activatedPID == frozenMousePID;
}

static void PFInvalidateSelectionReads(void) {
  PFCancelProvisionalDouble();
  PFAdvanceCounter(&gSelectionReadToken);
  PFAdvanceCounter(&gSelectionObserverDebounceToken);
  gSelectionCompletedGeneration = 0;
  gSelectionPendingObserverGeneration = 0;
  gSelectionPendingObserverPID = 0;
  gSelectionLastObserverNotificationAt = 0;
  gSelectionMouseObservedSelectionChange = NO;
  PFClearFrozenMouseSelectionContext();
}

static long long PFBeginSelectionGeneration(void) {
  long long generation = PFAdvanceCounter(&gSelectionGeneration);
  PFInvalidateSelectionReads();
  gSelectionLastEmittedText = nil;
  gSelectionLastEmittedAt = 0;
  gSelectionLastEmittedGeneration = 0;
  return generation;
}

static PFSelectionReadContext *PFMakeObserverSelectionReadContext(
  long long generation,
  pid_t targetPID,
  CGPoint anchor,
  NSTimeInterval triggerUptime,
  NSString *reason,
  BOOL emitEmptyStatus,
  id seedElement
) {
  PFSelectionReadContext *context = [[PFSelectionReadContext alloc] init];
  context.generation = generation;
  context.targetPID = targetPID;
  context.anchor = anchor;
  context.triggerUptime = triggerUptime;
  context.reason = reason;
  context.emitEmptyStatus = emitEmptyStatus;
  context.targetWindowID = kCGNullWindowID;

  if (!PFElementBelongsToPID(seedElement, targetPID)) {
    return nil;
  }
  context.targetElement = seedElement;
  context.hasPreferredElement = YES;
  context.targetWindow = PFWindowForElement(context.targetElement, targetPID);
  if (context.targetWindow == nil) {
    id focusedWindow = PFFocusedWindow(PFAppElementForPID(targetPID));
    if (PFElementBelongsToPID(focusedWindow, targetPID) &&
        PFElementIsWithinFrozenWindow(context.targetElement, focusedWindow, targetPID)) {
      context.targetWindow = focusedWindow;
    }
  }
  if (context.targetWindow == nil ||
      !PFElementIsWithinFrozenWindow(context.targetElement, context.targetWindow, targetPID)) {
    return nil;
  }
  return context;
}

static PFSelectionReadContext *PFMakeFrozenMouseSelectionReadContext(
  long long generation,
  pid_t targetPID,
  CGWindowID frozenWindowID,
  NSTimeInterval triggerUptime,
  CGPoint anchor,
  NSString *reason,
  NSString **error
) {
  if (error != NULL) {
    *error = nil;
  }
  NSString *resolveError = nil;
  id anchorElement = nil;
  id targetWindow = frozenWindowID != kCGNullWindowID
    ? PFResolveFrozenAXWindow(frozenWindowID, targetPID, &resolveError)
    : PFResolveAnchoredAXWindow(
        targetPID,
        anchor,
        &anchorElement,
        &resolveError
      );
  if (targetWindow == nil) {
    if (error != NULL) {
      *error = resolveError ?: (frozenWindowID != kCGNullWindowID
        ? @"frozen_ax_window_missing"
        : @"anchored_ax_window_unavailable");
    }
    return nil;
  }

  PFSelectionReadContext *context = [[PFSelectionReadContext alloc] init];
  context.generation = generation;
  context.targetPID = targetPID;
  context.targetWindowID = frozenWindowID;
  context.anchor = anchor;
  context.triggerUptime = triggerUptime;
  context.reason = reason;
  context.emitEmptyStatus = YES;
  context.targetWindow = targetWindow;
  context.requiresAnchorWindowRevalidation = YES;
  context.requiresDirectSelectionChangeEvidence = YES;
  context.directSelectionChangeObserved =
    gSelectionLastMouseSelectionChangeGeneration == generation;
  context.directSelectionBaselineCaptured =
    gSelectionDirectBaselineGeneration == generation &&
    gSelectionDirectBaselineCaptured;
  context.directSelectionBaselineElements =
    context.directSelectionBaselineCaptured
      ? [gSelectionDirectBaselineElements copy]
      : nil;
  context.directSelectionBaselineTexts =
    context.directSelectionBaselineCaptured
      ? [gSelectionDirectBaselineTexts copy]
      : nil;
  context.directSelectionBaselineFailureReason =
    context.directSelectionBaselineCaptured
      ? nil
      : [gSelectionDirectBaselineFailureReason copy];

  if (anchorElement == nil) {
    anchorElement = PFElementAtCGPosition(anchor, targetPID);
  }
  BOOL anchorElementValid = PFElementBelongsToPID(anchorElement, targetPID) &&
    PFElementIsWithinFrozenWindow(anchorElement, targetWindow, targetPID);
  context.anchorElement = anchorElementValid ? anchorElement : nil;
  id focusedElement = PFFocusedElementForApp(
    PFAppElementForPID(targetPID),
    targetPID
  );
  BOOL focusedElementCoveredByPrimaryAncestry = NO;
  BOOL focusedElementSafe = PFFocusedElementIsSafeSecondaryPreferred(
    anchorElementValid ? anchorElement : nil,
    focusedElement,
    targetWindow,
    targetPID,
    anchor,
    &focusedElementCoveredByPrimaryAncestry
  );

  if (anchorElementValid) {
    context.targetElement = anchorElement;
    context.hasPreferredElement = YES;
    context.primaryPreferredElementRequiresAnchorProof =
      PFMousePrimaryPreferredElementRequiresAnchorProof(
        YES,
        focusedElementSafe,
        focusedElementCoveredByPrimaryAncestry
      );
    if (focusedElementSafe) {
      // Preview/PDFKit commonly owns AXSelectedText on its focused document
      // element rather than the hit-tested element under the mouse. Keep this
      // as a frozen, same-window secondary seed only when it is anchored by a
      // shared control subtree or a frame that contains the mouse-up point.
      context.secondaryPreferredElement = focusedElement;
      context.hasSecondaryPreferredElement = YES;
    }
  } else if (focusedElementSafe) {
    context.targetElement = focusedElement;
    context.hasPreferredElement = YES;
    context.primaryPreferredElementRequiresAnchorProof =
      PFMousePrimaryPreferredElementRequiresAnchorProof(NO, YES, NO);
  } else {
    // The pointer may already have moved or the app may have switched windows.
    // The frozen AX window remains a safe search root; never use focus unless
    // its frozen frame or control ancestry proves ownership of the anchor.
    context.targetElement = targetWindow;
    context.hasPreferredElement = NO;
  }
  return context;
}

static BOOL PFReadSelection(
  PFSelectionReadContext *context,
  NSInteger attempt,
  BOOL terminalOnEmpty,
  BOOL *terminal,
  long long frozenLifecycleToken,
  long long frozenReadToken
) {
  if (terminal != NULL) {
    *terminal = NO;
  }
  if (!gSelectionWatcherEnabled) {
    if (terminal != NULL) {
      *terminal = YES;
    }
    return NO;
  }
  if (!gSelectionAXMessagingTimeoutHealthy) {
    PFEmitSelectionReadDiagnostic(
      @"selection_read_empty",
      context.reason,
      NO,
      0,
      0,
      PFSourceBundleIdentifierForPID(context.targetPID),
      @"selection_ax_timeout_unavailable",
      context.generation,
      attempt,
      PFElapsedMillisecondsSince(context.triggerUptime),
      YES,
      context.targetPID
    );
    PFEmitSelectionCapabilityStatus();
    if (terminal != NULL) {
      *terminal = YES;
    }
    return NO;
  }
  NSTimeInterval startedAt = PFMonotonicUptime();
  NSString *sourceBundleIdentifier = PFSourceBundleIdentifierForPID(context.targetPID);
  NSTimeInterval triggerToReadMs = PFElapsedMillisecondsSince(context.triggerUptime);
  if (!AXIsProcessTrusted()) {
    PFEmitSelectionReadDiagnostic(
      @"selection_read_empty",
      context.reason,
      NO,
      0,
      0,
      sourceBundleIdentifier,
      @"not_trusted",
      context.generation,
      attempt,
      triggerToReadMs,
      YES,
      context.targetPID
    );
    if (terminal != NULL) {
      *terminal = YES;
    }
    if (context.emitEmptyStatus) {
      PFEmitEmptySelectionStatusIfNeeded();
    }
    return NO;
  }

  if (context.targetPID <= 0 || PFIsSelfPID(context.targetPID)) {
    PFEmitSelectionReadDiagnostic(
      @"selection_read_empty",
      context.reason,
      NO,
      0,
      0,
      sourceBundleIdentifier,
      context.targetPID <= 0 ? @"target_pid_unavailable" : @"self_process_filtered",
      context.generation,
      attempt,
      triggerToReadMs,
      YES,
      context.targetPID
    );
    if (terminal != NULL) {
      *terminal = YES;
    }
    return NO;
  }

  NSInteger candidateCount = 0;
  NSString *readError = nil;
  gSelectionLastAXReadError = kAXErrorSuccess;
  NSString *text = PFSelectedTextForContext(context, &candidateCount, &readError);
  if (PFStringIsBlank(text) && gSelectionLastAXReadError == kAXErrorCannotComplete) {
    readError = @"ax_timeout";
  }
  NSTimeInterval durationMs = PFElapsedMillisecondsSince(startedAt);
  triggerToReadMs = PFElapsedMillisecondsSince(context.triggerUptime);
  if (PFStringIsBlank(text)) {
    PFEmitSelectionReadDiagnostic(
      @"selection_read_empty",
      context.reason,
      NO,
      candidateCount,
      durationMs,
      sourceBundleIdentifier,
      readError ?: @"no_selected_text",
      context.generation,
      attempt,
      triggerToReadMs,
      terminalOnEmpty,
      context.targetPID
    );
    if (terminal != NULL) {
      *terminal = terminalOnEmpty;
    }
    if (context.emitEmptyStatus && terminalOnEmpty) {
      PFEmitEmptySelectionStatusIfNeeded();
    }
    return NO;
  }

  BOOL stateCurrent = PFSelectionReadStateIsCurrent(
    context.generation,
    gSelectionGeneration,
    gSelectionCompletedGeneration,
    frozenLifecycleToken,
    gWatcherLifecycleToken,
    frozenReadToken,
    gSelectionReadToken
  );
  if (!stateCurrent) {
    if (terminal != NULL) {
      *terminal = YES;
    }
    return NO;
  }
  BOOL anchoredWindowIdentityCurrent =
    !context.requiresAnchorWindowRevalidation ||
    PFAnchoredAXContextIdentityIsCurrent(context);
  if (!PFSelectionCommitValidationPasses(
        stateCurrent,
        context.requiresAnchorWindowRevalidation,
        anchoredWindowIdentityCurrent
      )) {
    durationMs = PFElapsedMillisecondsSince(startedAt);
    triggerToReadMs = PFElapsedMillisecondsSince(context.triggerUptime);
    PFEmitSelectionReadDiagnostic(
      @"selection_read_empty",
      context.reason,
      NO,
      candidateCount,
      durationMs,
      sourceBundleIdentifier,
      @"selection_context_changed_before_commit",
      context.generation,
      attempt,
      triggerToReadMs,
      YES,
      context.targetPID
    );
    if (terminal != NULL) {
      *terminal = YES;
    }
    return NO;
  }
  durationMs = PFElapsedMillisecondsSince(startedAt);
  triggerToReadMs = PFElapsedMillisecondsSince(context.triggerUptime);

  NSTimeInterval now = [NSDate date].timeIntervalSince1970;
  BOOL isSameGenerationDuplicate =
    gSelectionLastEmittedGeneration == context.generation &&
    gSelectionLastEmittedText != nil &&
    [gSelectionLastEmittedText isEqualToString:text] &&
    now - gSelectionLastEmittedAt <= kSelectionSameGenerationDedupWindow;
  BOOL isCompletedMouseObserverEcho =
    !PFStringIsBlank(context.suppressedDuplicateText) &&
    [context.suppressedDuplicateText isEqualToString:text];
  if (isSameGenerationDuplicate || isCompletedMouseObserverEcho) {
    PFEmitSelectionReadDiagnostic(
      @"selection_read_duplicate",
      context.reason,
      YES,
      candidateCount,
      durationMs,
      sourceBundleIdentifier,
      @"none",
      context.generation,
      attempt,
      triggerToReadMs,
      YES,
      context.targetPID
    );
    if (terminal != NULL) {
      *terminal = YES;
    }
    return YES;
  }

  gSelectionLastEmittedText = [text copy];
  gSelectionLastEmittedAt = now;
  gSelectionLastEmittedGeneration = context.generation;
  PFEmitSelectionReadDiagnostic(
    @"selection_read_found",
    context.reason,
    YES,
    candidateCount,
    durationMs,
    sourceBundleIdentifier,
    @"none",
    context.generation,
    attempt,
    triggerToReadMs,
    YES,
    context.targetPID
  );
  if (terminal != NULL) {
    *terminal = YES;
  }
  PFEmitForTargetPID(
    PaperFloatEventSelection,
    text,
    context.anchor.x,
    context.anchor.y,
    context.generation,
    context.targetPID
  );
  return YES;
}

static BOOL PFMouseSelectionRetryIsTerminal(NSInteger attempt) {
  return attempt >= 3;
}

typedef NS_ENUM(NSInteger, PFFrozenContextAttemptDecision) {
  PFFrozenContextAttemptCancel = 0,
  PFFrozenContextAttemptRetry = 1,
  PFFrozenContextAttemptStartReads = 2,
  PFFrozenContextAttemptTerminalFailure = 3
};

static PFFrozenContextAttemptDecision PFResolveFrozenContextAttemptDecision(
  NSInteger attempt,
  BOOL stateCurrent,
  BOOL contextAvailable
) {
  if (attempt < 1 || !stateCurrent) {
    return PFFrozenContextAttemptCancel;
  }
  if (contextAvailable) {
    return PFFrozenContextAttemptStartReads;
  }
  return attempt >= 3
    ? PFFrozenContextAttemptTerminalFailure
    : PFFrozenContextAttemptRetry;
}

static NSString *PFMouseSelectionReason(
  NSInteger clickCount,
  BOOL isShiftSelection
) {
  if (clickCount == 2) {
    return @"mouse_up_double_click";
  }
  if (clickCount >= 3) {
    return @"mouse_up_triple_click";
  }
  return isShiftSelection ? @"mouse_up_shift" : @"mouse_up_drag";
}

static BOOL PFObserverSelectionEmptyIsTerminal(BOOL correlatedWithMouseGesture) {
  return !correlatedWithMouseGesture;
}

static void PFScheduleSelectionRead(
  PFSelectionReadContext *context,
  NSTimeInterval delay,
  NSInteger attempt
) {
  long long lifecycleToken = gWatcherLifecycleToken;
  long long readToken = gSelectionReadToken;
  dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(delay * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
    if (!gSelectionWatcherEnabled ||
        !PFSelectionReadStateIsCurrent(
          context.generation,
          gSelectionGeneration,
          gSelectionCompletedGeneration,
          lifecycleToken,
          gWatcherLifecycleToken,
          readToken,
          gSelectionReadToken
        )) {
      return;
    }

    BOOL terminal = NO;
    BOOL found = PFReadSelection(
      context,
      attempt,
      PFMouseSelectionRetryIsTerminal(attempt),
      &terminal,
      lifecycleToken,
      readToken
    );
    if (found || terminal) {
      gSelectionCompletedGeneration = context.generation;
    }
  });
}

static void PFScheduleSelectionReadRetries(
  PFSelectionReadContext *context,
  NSTimeInterval firstReadDelay
) {
  if (firstReadDelay <= 0) {
    PFScheduleSelectionRead(context, 0, 1);
    PFScheduleSelectionRead(
      context,
      kSelectionProvisionalDoubleSecondRetryDelay,
      2
    );
    PFScheduleSelectionRead(
      context,
      kSelectionProvisionalDoubleFinalRetryDelay,
      3
    );
    return;
  }
  PFScheduleSelectionRead(context, firstReadDelay, 1);
  PFScheduleSelectionRead(context, firstReadDelay + 0.20, 2);
  PFScheduleSelectionRead(context, firstReadDelay + 0.55, 3);
}

static void PFEmitFrozenWindowFailureDeferred(
  long long generation,
  pid_t targetPID,
  NSTimeInterval triggerUptime,
  NSString *reason,
  NSString *error
) {
  long long lifecycleToken = gWatcherLifecycleToken;
  NSString *frozenReason = [reason copy];
  NSString *frozenError = [error copy];
  dispatch_async(dispatch_get_main_queue(), ^{
    if (!gSelectionWatcherEnabled ||
        lifecycleToken != gWatcherLifecycleToken ||
        generation != gSelectionGeneration) {
      return;
    }
    PFEmitSelectionReadDiagnostic(
      @"selection_read_empty",
      frozenReason,
      NO,
      0,
      0,
      PFSourceBundleIdentifierForPID(targetPID),
      frozenError,
      generation,
      1,
      PFElapsedMillisecondsSince(triggerUptime),
      YES,
      targetPID
    );
    PFEmitEmptySelectionStatusIfNeeded();
  });
}

static void PFScheduleFrozenMouseSelectionContextAttempt(
  long long generation,
  pid_t targetPID,
  CGWindowID frozenWindowID,
  NSTimeInterval triggerUptime,
  CGPoint anchor,
  NSString *reason,
  NSTimeInterval firstReadDelay,
  long long lifecycleToken,
  long long readToken,
  NSInteger attempt,
  NSTimeInterval delay
) {
  dispatch_after(
    dispatch_time(DISPATCH_TIME_NOW, (int64_t)(delay * NSEC_PER_SEC)),
    dispatch_get_main_queue(),
    ^{
      BOOL stateCurrent = gSelectionWatcherEnabled &&
        PFSelectionReadStateIsCurrent(
          generation,
          gSelectionGeneration,
          gSelectionCompletedGeneration,
          lifecycleToken,
          gWatcherLifecycleToken,
          readToken,
          gSelectionReadToken
        );
      if (PFResolveFrozenContextAttemptDecision(attempt, stateCurrent, NO) ==
          PFFrozenContextAttemptCancel) {
        return;
      }

      NSString *freezeError = nil;
      gSelectionLastAXReadError = kAXErrorSuccess;
      PFSelectionReadContext *context = PFMakeFrozenMouseSelectionReadContext(
        generation,
        targetPID,
        frozenWindowID,
        triggerUptime,
        anchor,
        reason,
        &freezeError
      );
      PFFrozenContextAttemptDecision decision =
        PFResolveFrozenContextAttemptDecision(
          attempt,
          stateCurrent,
          context != nil
        );
      if (decision == PFFrozenContextAttemptRetry) {
        NSTimeInterval retryDelay = attempt == 1 ? 0.10 : 0.25;
        PFScheduleFrozenMouseSelectionContextAttempt(
          generation,
          targetPID,
          frozenWindowID,
          triggerUptime,
          anchor,
          reason,
          firstReadDelay,
          lifecycleToken,
          readToken,
          attempt + 1,
          retryDelay
        );
        return;
      }
      if (decision == PFFrozenContextAttemptTerminalFailure) {
        if (gSelectionLastAXReadError == kAXErrorCannotComplete) {
          freezeError = @"ax_timeout";
        }
        if (generation == gSelectionLastMouseUpGeneration &&
            frozenWindowID == gSelectionLastMouseUpWindowID) {
          gSelectionLastMouseGestureInvalid = YES;
          gSelectionLastMouseContext = nil;
        }
        PFEmitSelectionReadDiagnostic(
          @"selection_read_empty",
          reason,
          NO,
          0,
          0,
          PFSourceBundleIdentifierForPID(targetPID),
          freezeError ?: @"frozen_window_unavailable",
          generation,
          attempt,
          PFElapsedMillisecondsSince(triggerUptime),
          YES,
          targetPID
        );
        PFEmitEmptySelectionStatusIfNeeded();
        return;
      }
      if (decision != PFFrozenContextAttemptStartReads || context == nil) {
        return;
      }
      if (generation != gSelectionLastMouseUpGeneration ||
          frozenWindowID != gSelectionLastMouseUpWindowID) {
        return;
      }
      gSelectionLastMouseContext = context;
      NSTimeInterval elapsed = PFMonotonicUptime() - triggerUptime;
      NSTimeInterval remainingFirstReadDelay = isfinite(elapsed) && elapsed > 0
        ? fmax(0, firstReadDelay - elapsed)
        : firstReadDelay;
      PFScheduleSelectionReadRetries(context, remainingFirstReadDelay);
    }
  );
}

static void PFScheduleFrozenMouseSelectionReadRetries(
  long long generation,
  pid_t targetPID,
  CGWindowID frozenWindowID,
  NSTimeInterval triggerUptime,
  CGPoint anchor,
  NSString *reason,
  NSTimeInterval firstReadDelay
) {
  // The event-tap callback freezes only O(1) CGEvent fields. Public CGWindow
  // metadata and AX mapping are intentionally delayed until the main queue.
  // Preview can publish its AX window one turn later, so context construction
  // itself receives a bounded 0/100/350ms retry sequence. Every attempt keeps
  // the original frozen PID/window/anchor and the original lifecycle/read token.
  long long lifecycleToken = gWatcherLifecycleToken;
  long long readToken = gSelectionReadToken;
  PFScheduleFrozenMouseSelectionContextAttempt(
    generation,
    targetPID,
    frozenWindowID,
    triggerUptime,
    anchor,
    reason,
    firstReadDelay,
    lifecycleToken,
    readToken,
    1,
    0
  );
}

static void PFStopSelectionObserver(void) {
  if (gSelectionObserver != NULL) {
    if (gSelectionObserverSelectionNotificationReady && gSelectionObserverSelectionElement != nil) {
      AXObserverRemoveNotification(
        gSelectionObserver,
        (__bridge AXUIElementRef)gSelectionObserverSelectionElement,
        kAXSelectedTextChangedNotification
      );
    }
    if (gSelectionObserverFocusNotificationRegistered && gSelectionObserverAppElement != nil) {
      AXObserverRemoveNotification(
        gSelectionObserver,
        (__bridge AXUIElementRef)gSelectionObserverAppElement,
        kAXFocusedUIElementChangedNotification
      );
    }
    if (gSelectionObserverWindowNotificationRegistered && gSelectionObserverAppElement != nil) {
      AXObserverRemoveNotification(
        gSelectionObserver,
        (__bridge AXUIElementRef)gSelectionObserverAppElement,
        kAXFocusedWindowChangedNotification
      );
    }
  }

  gSelectionObserverAppElement = nil;
  gSelectionObserverSelectionElement = nil;
  gSelectionObserverFocusNotificationRegistered = NO;
  gSelectionObserverWindowNotificationRegistered = NO;
  gSelectionObserverSelectionNotificationReady = NO;
  gSelectionObserverPID = 0;

  if (gSelectionObserverRunLoopSource != NULL) {
    CFRunLoopRemoveSource(CFRunLoopGetMain(), gSelectionObserverRunLoopSource, kCFRunLoopCommonModes);
    gSelectionObserverRunLoopSource = NULL;
  }

  if (gSelectionObserver != NULL) {
    CFRelease(gSelectionObserver);
    gSelectionObserver = NULL;
  }
}

static BOOL PFObserveAXNotification(id element, CFStringRef notification) {
  if (gSelectionObserver == NULL || element == nil ||
      CFGetTypeID((__bridge CFTypeRef)element) != AXUIElementGetTypeID()) {
    return NO;
  }

  AXError error = AXObserverAddNotification(
    gSelectionObserver,
    (__bridge AXUIElementRef)element,
    notification,
    NULL
  );
  return error == kAXErrorSuccess || error == kAXErrorNotificationAlreadyRegistered;
}

static BOOL PFInstallFocusedElementSelectionObserver(void) {
  if (gSelectionObserver == NULL || gSelectionObserverAppElement == nil || gSelectionObserverPID <= 0) {
    return NO;
  }

  if (gSelectionObserverSelectionNotificationReady && gSelectionObserverSelectionElement != nil) {
    AXObserverRemoveNotification(
      gSelectionObserver,
      (__bridge AXUIElementRef)gSelectionObserverSelectionElement,
      kAXSelectedTextChangedNotification
    );
  }
  gSelectionObserverSelectionElement = nil;
  gSelectionObserverSelectionNotificationReady = NO;

  id candidate = PFFocusedElementForApp(gSelectionObserverAppElement, gSelectionObserverPID);
  NSInteger depth = 0;
  while (candidate != nil && depth < 4) {
    if (PFObserveAXNotification(candidate, kAXSelectedTextChangedNotification)) {
      gSelectionObserverSelectionElement = candidate;
      gSelectionObserverSelectionNotificationReady = YES;
      return YES;
    }
    candidate = PFParent(candidate);
    depth += 1;
  }

  return NO;
}

static void PFEmitSelectionCapabilityStatus(void) {
  if (!gSelectionWatcherEnabled) {
    PFEmit(PaperFloatEventSelectionStatus, @"selection_disabled_by_setting", 0, 0, 0);
    return;
  }
  BOOL accessibilityTrusted = AXIsProcessTrusted();
  if (!gSelectionAXMessagingTimeoutHealthy && accessibilityTrusted) {
    PFEmit(PaperFloatEventSelectionStatus, @"selection_ax_timeout_unavailable", 0, 0, 0);
  } else if (gMouseEventTapHealthy && accessibilityTrusted && gSelectionObserverSelectionNotificationReady) {
    PFEmit(PaperFloatEventSelectionStatus, @"selection_sources_ready", 0, 0, 0);
  } else if (gMouseEventTapHealthy && accessibilityTrusted) {
    PFEmit(PaperFloatEventSelectionStatus, @"selection_mouse_ready_ax_observer_limited", 0, 0, 0);
  } else if (gMouseEventTapHealthy) {
    PFEmit(PaperFloatEventSelectionStatus, @"selection_event_ready_accessibility_limited", 0, 0, 0);
  } else if (accessibilityTrusted && gSelectionObserverSelectionNotificationReady) {
    PFEmit(PaperFloatEventSelectionStatus, @"selection_ax_observer_ready_mouse_unavailable", 0, 0, 0);
  } else if (accessibilityTrusted) {
    PFEmit(PaperFloatEventSelectionStatus, @"accessibility_selection_input_limited", 0, 0, 0);
  } else {
    PFEmit(PaperFloatEventSelectionStatus, @"selection_clipboard_only", 0, 0, 0);
  }
}

static void PFRecordSelectionChangeNotification(
  id element,
  pid_t targetPID,
  NSTimeInterval now
) {
  if (gSelectionHasMouseDownLocation &&
      gSelectionMouseGeneration > 0 &&
      gSelectionMouseGeneration == gSelectionGeneration &&
      (gSelectionMouseDownTargetPID <= 0 ||
       gSelectionMouseDownTargetPID == targetPID)) {
    gSelectionMouseObservedSelectionChange = YES;
    return;
  }

  BOOL recentPendingMouseGesture =
    gSelectionLastMouseUpGeneration > 0 &&
    gSelectionLastMouseUpGeneration == gSelectionGeneration &&
    gSelectionCompletedGeneration != gSelectionLastMouseUpGeneration &&
    !gSelectionLastMouseGestureInvalid &&
    gSelectionLastMouseUpPID == targetPID &&
    now >= gSelectionLastMouseUpAt &&
    now - gSelectionLastMouseUpAt <= kSelectionMouseObserverCorrelationWindow;
  if (!recentPendingMouseGesture) {
    return;
  }

  gSelectionLastMouseSelectionChangeGeneration =
    gSelectionLastMouseUpGeneration;
  PFSelectionReadContext *context = gSelectionLastMouseContext;
  if (context != nil &&
      PFElementIsWithinFrozenWindow(element, context.targetWindow, targetPID)) {
    context.directSelectionChangeObserved = YES;
  }
}

static void PFScheduleSelectionObserverRead(id element, pid_t targetPID) {
  if (!gSelectionWatcherEnabled || targetPID <= 0 || PFIsSelfPID(targetPID)) {
    return;
  }
  NSTimeInterval now = [NSDate date].timeIntervalSince1970;
  PFRecordSelectionChangeNotification(element, targetPID, now);
  if (gSelectionProvisionalDoublePending) {
    return;
  }

  NSTimeInterval observerTriggerUptime = PFMonotonicUptime();
  if (gSelectionHasMouseDownLocation) {
    return;
  }

  BOOL hasRecentMouseGesture =
    gSelectionLastMouseUpGeneration > 0 &&
    gSelectionLastMouseUpGeneration == gSelectionGeneration &&
    now >= gSelectionLastMouseUpAt &&
    now - gSelectionLastMouseUpAt <= kSelectionMouseObserverCorrelationWindow;
  BOOL mouseGenerationCompleted = hasRecentMouseGesture &&
    gSelectionCompletedGeneration == gSelectionLastMouseUpGeneration;
  BOOL sameMousePID = hasRecentMouseGesture &&
    gSelectionLastMouseUpPID == targetPID;
  BOOL correlatedWithMouseGesture =
    PFResolveObserverMouseCorrelation(
      hasRecentMouseGesture,
      mouseGenerationCompleted,
      gSelectionLastMouseGestureInvalid,
      sameMousePID
    ) == PFObserverMouseCorrelationReusePendingMouse;
  NSString *suppressedCompletedMouseText =
    PFCompletedMouseObserverEchoNeedsTextCheck(
      hasRecentMouseGesture,
      mouseGenerationCompleted,
      sameMousePID,
      gSelectionLastEmittedGeneration == gSelectionLastMouseUpGeneration &&
        !PFStringIsBlank(gSelectionLastEmittedText)
    )
      ? [gSelectionLastEmittedText copy]
      : nil;

  long long generation = 0;
  CGPoint anchor = CGPointZero;
  PFSelectionReadContext *context = nil;
  if (correlatedWithMouseGesture) {
    generation = gSelectionLastMouseUpGeneration;
    anchor = gSelectionLastMouseUpAnchor;
    context = gSelectionLastMouseContext;
    BOOL elementWithinFrozenWindow = context != nil &&
      PFElementIsWithinFrozenWindow(element, context.targetWindow, targetPID);
    if (!PFObserverFrozenIdentityMatches(
          context != nil,
          gSelectionLastMouseGestureInvalid,
          context == nil ? 0 : context.generation,
          gSelectionGeneration,
          gSelectionLastMouseUpGeneration,
          context == nil ? 0 : context.targetPID,
          targetPID,
          gSelectionLastMouseUpPID,
          context == nil ? kCGNullWindowID : context.targetWindowID,
          gSelectionLastMouseUpWindowID,
          elementWithinFrozenWindow
        )) {
      return;
    }
  } else {
    BOOL canReuseObserverGeneration =
      gSelectionPendingObserverGeneration > 0 &&
      gSelectionPendingObserverGeneration == gSelectionGeneration &&
      gSelectionCompletedGeneration != gSelectionPendingObserverGeneration &&
      gSelectionPendingObserverPID == targetPID &&
      now - gSelectionLastObserverNotificationAt <= kSelectionObserverGroupingWindow;

    if (canReuseObserverGeneration) {
      generation = gSelectionPendingObserverGeneration;
      anchor = gSelectionPendingObserverAnchor;
    } else {
      generation = PFBeginSelectionGeneration();
      anchor = PFCurrentCGMouseAnchor();
      gSelectionPendingObserverGeneration = generation;
      gSelectionPendingObserverPID = targetPID;
      gSelectionPendingObserverAnchor = anchor;
    }
    gSelectionLastObserverNotificationAt = now;
    gSelectionLastAXReadError = kAXErrorSuccess;
    context = PFMakeObserverSelectionReadContext(
      generation,
      targetPID,
      anchor,
      observerTriggerUptime,
      @"ax_observer",
      NO,
      element
    );
    context.suppressedDuplicateText = suppressedCompletedMouseText;
  }

  if (context == nil) {
    NSString *contextError = gSelectionLastAXReadError == kAXErrorCannotComplete
      ? @"ax_timeout"
      : @"observer_context_unavailable";
    PFEmitSelectionReadDiagnostic(
      @"selection_read_empty",
      @"ax_observer",
      NO,
      0,
      0,
      PFSourceBundleIdentifierForPID(targetPID),
      contextError,
      generation,
      1,
      PFElapsedMillisecondsSince(observerTriggerUptime),
      YES,
      targetPID
    );
    gSelectionCompletedGeneration = generation;
    return;
  }
  long long lifecycleToken = gWatcherLifecycleToken;
  long long readToken = gSelectionReadToken;
  long long debounceToken = PFAdvanceCounter(&gSelectionObserverDebounceToken);
  dispatch_after(
    dispatch_time(DISPATCH_TIME_NOW, (int64_t)(kSelectionObserverDebounceDelay * NSEC_PER_SEC)),
    dispatch_get_main_queue(),
    ^{
      if (!gSelectionWatcherEnabled ||
          !PFSelectionReadStateIsCurrent(
            context.generation,
            gSelectionGeneration,
            gSelectionCompletedGeneration,
            lifecycleToken,
            gWatcherLifecycleToken,
            readToken,
            gSelectionReadToken
          ) ||
          debounceToken != gSelectionObserverDebounceToken) {
        return;
      }

      BOOL terminal = NO;
      BOOL found = PFReadSelection(
        context,
        1,
        PFObserverSelectionEmptyIsTerminal(correlatedWithMouseGesture),
        &terminal,
        lifecycleToken,
        readToken
      );
      if (found || terminal) {
        gSelectionCompletedGeneration = context.generation;
      }
    }
  );
}

static void PFSelectionAXObserverCallback(
  AXObserverRef observer,
  AXUIElementRef element,
  CFStringRef notification,
  void *refcon
) {
  (void)observer;
  (void)refcon;

  if (!gSelectionWatcherEnabled || !gSelectionAXMessagingTimeoutHealthy) {
    return;
  }

  if (CFEqual(notification, kAXFocusedUIElementChangedNotification) ||
      CFEqual(notification, kAXFocusedWindowChangedNotification)) {
    PFAdvanceCounter(&gSelectionObserverDebounceToken);
    gSelectionPendingObserverGeneration = 0;
    gSelectionPendingObserverPID = 0;
    gSelectionLastObserverNotificationAt = 0;
    PFInstallFocusedElementSelectionObserver();
    PFEmitSelectionCapabilityStatus();
    return;
  }

  if (CFEqual(notification, kAXSelectedTextChangedNotification)) {
    if (gSelectionObserverSelectionElement == nil ||
        !CFEqual(element, (__bridge AXUIElementRef)gSelectionObserverSelectionElement)) {
      return;
    }
    pid_t targetPID = 0;
    AXError error = AXUIElementGetPid(element, &targetPID);
    if (error == kAXErrorSuccess) {
      PFScheduleSelectionObserverRead((__bridge id)element, targetPID);
    }
  }
}

static BOOL PFStartSelectionObserver(pid_t targetPID) {
  PFStopSelectionObserver();

  if (!gSelectionWatcherEnabled ||
      !gSelectionAXMessagingTimeoutHealthy ||
      !AXIsProcessTrusted() ||
      targetPID <= 0 ||
      PFIsSelfPID(targetPID)) {
    return NO;
  }

  AXObserverRef observer = NULL;
  AXError error = AXObserverCreate(targetPID, PFSelectionAXObserverCallback, &observer);
  if (error != kAXErrorSuccess || observer == NULL) {
    return NO;
  }

  gSelectionObserver = observer;
  gSelectionObserverRunLoopSource = AXObserverGetRunLoopSource(observer);
  if (gSelectionObserverRunLoopSource != NULL) {
    CFRunLoopAddSource(CFRunLoopGetMain(), gSelectionObserverRunLoopSource, kCFRunLoopCommonModes);
  } else {
    PFStopSelectionObserver();
    return NO;
  }

  gSelectionObserverPID = targetPID;
  gSelectionObserverAppElement = PFAppElementForPID(targetPID);
  gSelectionObserverFocusNotificationRegistered = PFObserveAXNotification(
    gSelectionObserverAppElement,
    kAXFocusedUIElementChangedNotification
  );
  gSelectionObserverWindowNotificationRegistered = PFObserveAXNotification(
    gSelectionObserverAppElement,
    kAXFocusedWindowChangedNotification
  );
  PFInstallFocusedElementSelectionObserver();

  return gSelectionObserverSelectionNotificationReady;
}

static void PFStartWorkspaceActivationWatcher(void) {
  if (!gSelectionWatcherEnabled || gWorkspaceActivationObserver != nil) {
    return;
  }

  gWorkspaceActivationObserver = [[[NSWorkspace sharedWorkspace] notificationCenter]
    addObserverForName:NSWorkspaceDidActivateApplicationNotification
    object:nil
    queue:[NSOperationQueue mainQueue]
    usingBlock:^(NSNotification *notification) {
      if (!gSelectionWatcherEnabled) {
        return;
      }
      NSRunningApplication *activatedApp = notification.userInfo[NSWorkspaceApplicationKey];
      pid_t targetPID = activatedApp == nil ? PFFrontmostPID() : activatedApp.processIdentifier;
      if (PFIsSelfPID(targetPID)) {
        return;
      }
      BOOL preservePendingFrozenRead =
        !gSelectionHasMouseDownLocation &&
        PFShouldPreservePendingFrozenReadOnActivation(
          PFHasPendingFrozenMouseSelectionAt(
            [NSDate date].timeIntervalSince1970
          ),
          targetPID,
          gSelectionLastMouseUpPID
        );
      if (!gSelectionHasMouseDownLocation && !preservePendingFrozenRead) {
        PFInvalidateSelectionReads();
      } else {
        PFAdvanceCounter(&gSelectionObserverDebounceToken);
        gSelectionPendingObserverGeneration = 0;
        gSelectionPendingObserverPID = 0;
        gSelectionLastObserverNotificationAt = 0;
      }
      PFStartSelectionObserver(targetPID);
      PFEmitSelectionCapabilityStatus();
    }];
}

static void PFStopWorkspaceActivationWatcher(void) {
  if (gWorkspaceActivationObserver != nil) {
    [[[NSWorkspace sharedWorkspace] notificationCenter] removeObserver:gWorkspaceActivationObserver];
    gWorkspaceActivationObserver = nil;
  }
}

static void PFEmitDoubleCopy(void) {
  NSString *text = [[NSPasteboard generalPasteboard] stringForType:NSPasteboardTypeString] ?: @"";
  CGPoint location = PFCurrentCGMouseAnchor();
  PFEmit(PaperFloatEventDoubleCopy, text, location.x, location.y, 0);
}

static void PFHandleCopyShortcut(void) {
  NSTimeInterval now = [NSDate date].timeIntervalSince1970;

  if (now - gPasteboardLastCopyShortcutAt <= kDoubleCopyWindow) {
    gPasteboardLastCopyShortcutAt = 0;
    long long lifecycleToken = gWatcherLifecycleToken;
    dispatch_after(
      dispatch_time(DISPATCH_TIME_NOW, (int64_t)(kPasteboardReadDelay * NSEC_PER_SEC)),
      dispatch_get_main_queue(),
      ^{
        if (lifecycleToken == gWatcherLifecycleToken) {
          PFEmitDoubleCopy();
        }
      }
    );
  } else {
    gPasteboardLastCopyShortcutAt = now;
  }
}

static void PFHandlePasteboardFallbackCopy(NSString *text, NSInteger delta) {
  if (!PFStringIsBlank(text)) {
    NSTimeInterval now = [NSDate date].timeIntervalSince1970;
    if (delta >= 2) {
      PFEmit(
        PaperFloatEventPasteboardStatus,
        [NSString stringWithFormat:@"pasteboard_fallback_delta_ambiguous\t%ld", (long)delta],
        0,
        0,
        0
      );
      gPasteboardLastFallbackCopyAt = now;
      gPasteboardLastFallbackText = [text copy];
      return;
    }

    BOOL followsMatchingCopy = gPasteboardLastFallbackText != nil &&
      [gPasteboardLastFallbackText isEqualToString:text] &&
      now - gPasteboardLastFallbackCopyAt <= kDoubleCopyWindow;

    if (followsMatchingCopy) {
      gPasteboardLastFallbackCopyAt = 0;
      gPasteboardLastFallbackText = nil;
      CGPoint location = PFCurrentCGMouseAnchor();
      PFEmit(PaperFloatEventDoubleCopy, text, location.x, location.y, 0);
    } else {
      gPasteboardLastFallbackCopyAt = now;
      gPasteboardLastFallbackText = [text copy];
    }
  } else {
    gPasteboardLastFallbackCopyAt = 0;
    gPasteboardLastFallbackText = nil;
  }
}

static NSString *PFEventTapDisableReason(CGEventType type) {
  return type == kCGEventTapDisabledByTimeout ? @"timeout" : @"user_input";
}

#if defined(PAPER_FLOAT_NATIVE_TESTING)
static NSInteger gPaperFloatTestTapReenableResult = -1;
#endif

static BOOL PFReenableTap(CFMachPortRef tap) {
#if defined(PAPER_FLOAT_NATIVE_TESTING)
  if (gPaperFloatTestTapReenableResult >= 0) {
    return gPaperFloatTestTapReenableResult != 0;
  }
#endif
  if (tap == NULL || !CFMachPortIsValid(tap)) {
    return NO;
  }

  CGEventTapEnable(tap, true);
  return CGEventTapIsEnabled(tap);
}

static void PFHandleKeyTapDisabled(CGEventType type) {
  NSTimeInterval disabledAtUptime = PFMonotonicUptime();
  NSString *reason = PFEventTapDisableReason(type);
  gKeyEventTapHealthy = NO;
  if (PFReenableTap(gKeyEventTap)) {
    gKeyEventTapHealthy = YES;
    PFEmitTapStateDeferred(
      PaperFloatEventPasteboardStatus,
      [NSString stringWithFormat:@"key_tap_disabled_%@_recovered", reason],
      disabledAtUptime
    );
  } else {
    PFEmitTapStateDeferred(
      PaperFloatEventPasteboardStatus,
      [NSString stringWithFormat:@"key_tap_disabled_%@_fallback", reason],
      disabledAtUptime
    );
  }
}

static void PFHandleMouseTapDisabled(CGEventType type) {
  if (!gSelectionWatcherEnabled) {
    return;
  }
  NSTimeInterval disabledAtUptime = PFMonotonicUptime();
  NSString *reason = PFEventTapDisableReason(type);
  gMouseEventTapHealthy = NO;
  gSelectionDidDrag = NO;
  gSelectionHasMouseDownLocation = NO;
  gSelectionIgnoreCurrentMouseGesture = NO;
  gSelectionMouseDownTargetPID = 0;
  gSelectionMouseDownTargetPIDAnchored = NO;
  gSelectionMouseDownWindowID = kCGNullWindowID;
  gSelectionMouseGeneration = 0;
  PFInvalidateSelectionReads();
  if (PFReenableTap(gMouseEventTap)) {
    gMouseEventTapHealthy = YES;
    PFEmitTapStateDeferred(
      PaperFloatEventSelectionStatus,
      [NSString stringWithFormat:@"selection_mouse_tap_disabled_%@_recovered", reason],
      disabledAtUptime
    );
  } else {
    NSString *fallback = gSelectionObserverSelectionNotificationReady ? @"fallback_ax" : @"unavailable";
    PFEmitTapStateDeferred(
      PaperFloatEventSelectionStatus,
      [NSString stringWithFormat:@"selection_mouse_tap_disabled_%@_%@", reason, fallback],
      disabledAtUptime
    );
  }
}

static CGEventRef PFKeyEventTapCallback(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void *userInfo) {
  (void)proxy;
  (void)userInfo;

  if (type == kCGEventTapDisabledByTimeout || type == kCGEventTapDisabledByUserInput) {
    PFHandleKeyTapDisabled(type);
    return event;
  }

  if (type != kCGEventKeyDown) {
    return event;
  }

  int64_t keyCode = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
  int64_t isRepeat = CGEventGetIntegerValueField(event, kCGKeyboardEventAutorepeat);
  CGEventFlags flags = CGEventGetFlags(event);

  BOOL selfIsFrontmost = PFIsSelfPID(PFFrontmostPID());
  if (!selfIsFrontmost && isRepeat == 0 && keyCode == 53) {
    PFEmit(PaperFloatEventPopupEscape, @"", 0, 0, 0);
  }

  CGEventFlags focusBlockingFlags = kCGEventFlagMaskCommand | kCGEventFlagMaskControl | kCGEventFlagMaskAlternate;
  if (!selfIsFrontmost && isRepeat == 0 && keyCode == 48 && (flags & focusBlockingFlags) == 0) {
    long long direction = (flags & kCGEventFlagMaskShift) != 0 ? -1 : 1;
    PFEmit(PaperFloatEventPopupFocusRequest, @"", 0, 0, direction);
  }

  if (keyCode == 8 && (flags & kCGEventFlagMaskCommand) != 0 && isRepeat == 0) {
    PFHandleCopyShortcut();
  }

  return event;
}

static CGEventRef PFMouseEventTapCallback(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void *userInfo) {
  (void)proxy;
  (void)userInfo;

  if (!gSelectionWatcherEnabled) {
    return event;
  }

  if (type == kCGEventTapDisabledByTimeout || type == kCGEventTapDisabledByUserInput) {
    PFHandleMouseTapDisabled(type);
    return event;
  }

  if (event == NULL) {
    return event;
  }

  CGPoint location = CGEventGetLocation(event);

  switch (type) {
    case kCGEventLeftMouseDown: {
      long long generation = PFBeginSelectionGeneration();
      pid_t eventTargetPID = PFEventTargetPID(event);
      pid_t targetPID = eventTargetPID;
      CGWindowID receivingWindowID = PFEventReceivingWindowID(event);
      gSelectionMouseDownLocation = location;
      gSelectionHasMouseDownLocation = YES;
      gSelectionDidDrag = NO;
      gSelectionMouseGeneration = generation;
      gSelectionMouseObservedSelectionChange = NO;
      gSelectionMouseDownWindowID = receivingWindowID;
      gSelectionMouseDownTargetPIDAnchored = NO;
      gSelectionIgnoreCurrentMouseGesture = PFIsSelfPID(eventTargetPID);
      if (!gSelectionIgnoreCurrentMouseGesture) {
        NSArray *baselineElements = nil;
        NSArray *baselineTexts = nil;
        NSString *baselineFailureReason = nil;
        pid_t anchoredTargetPID = 0;
        BOOL baselineCaptured = PFCapturePreDeliveryDirectSelectionBaseline(
          location,
          &anchoredTargetPID,
          &baselineElements,
          &baselineTexts,
          &baselineFailureReason
        );
        if (anchoredTargetPID > 0 && !PFIsSelfPID(anchoredTargetPID)) {
          targetPID = anchoredTargetPID;
          gSelectionMouseDownTargetPIDAnchored = YES;
        }
        gSelectionDirectBaselineGeneration = generation;
        gSelectionDirectBaselineCaptured = baselineCaptured;
        gSelectionDirectBaselineElements = baselineCaptured
          ? [baselineElements copy]
          : nil;
        gSelectionDirectBaselineTexts = baselineCaptured
          ? [baselineTexts copy]
          : nil;
        gSelectionDirectBaselineFailureReason = baselineCaptured
          ? nil
          : [baselineFailureReason copy];
        if (!gSelectionAXMessagingTimeoutHealthy) {
          PFEmitDeferred(
            PaperFloatEventSelectionStatus,
            @"selection_ax_timeout_unavailable",
            0,
            0,
            0
          );
        }
        PFEmitDeferred(PaperFloatEventMouseDown, @"", location.x, location.y, generation);
      }
      gSelectionMouseDownTargetPID = targetPID;
      break;
    }
    case kCGEventLeftMouseDragged:
      if (!gSelectionIgnoreCurrentMouseGesture) {
        gSelectionDidDrag = YES;
      }
      break;
    case kCGEventLeftMouseUp: {
      if (!gSelectionHasMouseDownLocation) {
        gSelectionMouseGeneration = PFBeginSelectionGeneration();
        pid_t inferredTargetPID = PFEventTargetPID(event);
        gSelectionMouseDownTargetPID = inferredTargetPID;
        gSelectionMouseDownTargetPIDAnchored = NO;
        gSelectionMouseDownWindowID = PFEventReceivingWindowID(event);
      }

      BOOL movedEnough = NO;
      if (gSelectionHasMouseDownLocation) {
        CGFloat dx = location.x - gSelectionMouseDownLocation.x;
        CGFloat dy = location.y - gSelectionMouseDownLocation.y;
        movedEnough = sqrt(dx * dx + dy * dy) >= kSelectionDragDistanceThreshold;
      }

      NSInteger clickCount = (NSInteger)CGEventGetIntegerValueField(event, kCGMouseEventClickState);
      CGEventFlags flags = CGEventGetFlags(event);
      BOOL isMultiClick = clickCount >= 2;
      BOOL isShiftSelection = (flags & kCGEventFlagMaskShift) != 0;
      BOOL shouldRead = gSelectionDidDrag || movedEnough || isMultiClick || isShiftSelection;
      pid_t mouseDownPID = gSelectionMouseDownTargetPID;
      BOOL mouseDownPIDAnchored = gSelectionMouseDownTargetPIDAnchored;
      pid_t mouseUpPID = PFEventTargetPID(event);
      CGWindowID mouseDownWindowID = gSelectionMouseDownWindowID;
      CGWindowID mouseUpWindowID = PFEventReceivingWindowID(event);
      NSTimeInterval triggerUptime = PFMonotonicUptime();
      pid_t targetPID = 0;
      CGWindowID frozenWindowID = kCGNullWindowID;
      PFSelectionWindowFreezeStatus pidStatus = PFChooseGestureTargetPID(
        mouseDownPID,
        mouseUpPID,
        mouseDownPIDAnchored,
        PFSelfPID(),
        &targetPID
      );
      PFSelectionWindowFreezeStatus windowStatus = PFChooseGestureWindowID(
        mouseDownWindowID,
        mouseUpWindowID,
        &frozenWindowID
      );
      long long generation = gSelectionMouseGeneration;
      BOOL ignoredMouseGesture = gSelectionIgnoreCurrentMouseGesture;
      BOOL mouseObservedSelectionChange =
        gSelectionMouseObservedSelectionChange;

      gSelectionDidDrag = NO;
      gSelectionHasMouseDownLocation = NO;
      gSelectionIgnoreCurrentMouseGesture = NO;
      gSelectionMouseGeneration = 0;
      gSelectionMouseObservedSelectionChange = NO;
      gSelectionMouseDownTargetPID = 0;
      gSelectionMouseDownTargetPIDAnchored = NO;
      gSelectionMouseDownWindowID = kCGNullWindowID;

      if (shouldRead) {
        NSString *reason = PFMouseSelectionReason(clickCount, isShiftSelection);
        pid_t correlationPID = targetPID > 0
          ? targetPID
          : (mouseDownPID > 0 ? mouseDownPID : mouseUpPID);
        gSelectionLastMouseUpGeneration = generation;
        gSelectionLastMouseUpPID = correlationPID;
        gSelectionLastMouseUpAnchor = location;
        gSelectionLastMouseUpAt = [NSDate date].timeIntervalSince1970;
        gSelectionLastMouseUpWindowID = frozenWindowID;
        gSelectionLastMouseContext = nil;
        gSelectionLastMouseSelectionChangeGeneration =
          mouseObservedSelectionChange ? generation : 0;

        PFSelectionWindowFreezeStatus failureStatus = PFSelectionWindowFreezeSuccess;
        PFSelectionWindowResolutionStrategy windowResolution =
          PFChooseSelectionWindowResolutionStrategy(
            pidStatus,
            windowStatus,
            &failureStatus
          );
        NSTimeInterval firstReadDelay = clickCount == 2 ? 0 : 0.10;
        dispatch_block_t commitFrozenSelection = ^{
          if (windowResolution == PFSelectionWindowResolutionReject) {
            gSelectionLastMouseGestureInvalid = YES;
            if (failureStatus != PFSelectionWindowFreezeSelfTarget &&
                !ignoredMouseGesture) {
              PFEmitFrozenWindowFailureDeferred(
                generation,
                correlationPID,
                triggerUptime,
                reason,
                PFSelectionWindowFreezeError(failureStatus)
              );
            }
          } else {
            gSelectionLastMouseGestureInvalid = NO;
            PFScheduleFrozenMouseSelectionReadRetries(
              generation,
              targetPID,
              frozenWindowID,
              triggerUptime,
              location,
              reason,
              firstReadDelay
            );
          }
        };
        if (clickCount == 2) {
          long long lifecycleToken = gWatcherLifecycleToken;
          long long readToken = gSelectionReadToken;
          gSelectionLastMouseGestureInvalid = NO;
          PFArmProvisionalDoubleCommit(
            generation,
            gSelectionMultiClickQuietWindow,
            ^{
              if (!gSelectionWatcherEnabled ||
                  !PFSelectionReadStateIsCurrent(
                    generation,
                    gSelectionGeneration,
                    gSelectionCompletedGeneration,
                    lifecycleToken,
                    gWatcherLifecycleToken,
                    readToken,
                    gSelectionReadToken
                  ) ||
                  generation != gSelectionLastMouseUpGeneration ||
                  frozenWindowID != gSelectionLastMouseUpWindowID) {
                return;
              }
              commitFrozenSelection();
            }
          );
        } else {
          commitFrozenSelection();
        }
      }
      break;
    }
    default:
      break;
  }

  return event;
}

#if defined(PAPER_FLOAT_NATIVE_TESTING)
typedef void (*PaperFloatTestMultiClickCallback)(void *context, int eventType);

void paper_float_test_multi_click_schedule_double(
  void *context,
  PaperFloatTestMultiClickCallback callback,
  double quietWindow
) {
  PFRunOnMainQueue(^{
    if (callback == NULL) {
      PFCancelProvisionalDouble();
      return;
    }
    long long generation = PFBeginSelectionGeneration();
    PFArmProvisionalDoubleCommit(
      generation,
      (NSTimeInterval)quietWindow,
      ^{
        callback(context, 2);
      }
    );
  });
}

void paper_float_test_multi_click_begin_mouse_down(void) {
  PFRunOnMainQueue(^{
    (void)PFBeginSelectionGeneration();
  });
}

void paper_float_test_multi_click_commit_triple(
  void *context,
  PaperFloatTestMultiClickCallback callback
) {
  PFRunOnMainQueue(^{
    if (callback != NULL) {
      callback(context, 3);
    }
  });
}

void paper_float_test_multi_click_observer_attempt(
  void *context,
  PaperFloatTestMultiClickCallback callback
) {
  PFRunOnMainQueue(^{
    if (!gSelectionProvisionalDoublePending && callback != NULL) {
      callback(context, 4);
    }
  });
}

void paper_float_test_multi_click_invalidate(void) {
  PFRunOnMainQueue(^{
    PFInvalidateSelectionReads();
  });
}

int paper_float_test_multi_click_pending(void) {
  __block int pending = 0;
  dispatch_block_t readPending = ^{
    pending = gSelectionProvisionalDoublePending ? 1 : 0;
  };
  if ([NSThread isMainThread]) {
    readPending();
  } else {
    dispatch_sync(dispatch_get_main_queue(), readPending);
  }
  return pending;
}

double paper_float_test_multi_click_quiet_window_for_interval(
  double systemInterval
) {
  return PFMultiClickQuietWindowForSystemInterval(
    (NSTimeInterval)systemInterval
  );
}

double paper_float_test_configured_multi_click_quiet_window(void) {
  __block NSTimeInterval result = 0;
  dispatch_block_t readConfiguredWindow = ^{
    result = PFConfiguredMultiClickQuietWindow();
  };
  if ([NSThread isMainThread]) {
    readConfiguredWindow();
  } else {
    dispatch_sync(dispatch_get_main_queue(), readConfiguredWindow);
  }
  return result;
}

void paper_float_test_set_multi_click_system_interval(double systemInterval) {
  gPaperFloatTestMultiClickSystemIntervalOverrideEnabled = YES;
  gPaperFloatTestMultiClickSystemIntervalOverride = (NSTimeInterval)systemInterval;
}

int paper_float_test_selection_retry_terminal(int attempt) {
  return PFMouseSelectionRetryIsTerminal((NSInteger)attempt) ? 1 : 0;
}

int paper_float_test_frozen_context_attempt_decision(
  int attempt,
  int stateCurrent,
  int contextAvailable
) {
  return (int)PFResolveFrozenContextAttemptDecision(
    (NSInteger)attempt,
    stateCurrent != 0,
    contextAvailable != 0
  );
}

int paper_float_test_observer_empty_terminal(int correlatedWithMouseGesture) {
  return PFObserverSelectionEmptyIsTerminal(correlatedWithMouseGesture != 0) ? 1 : 0;
}

int paper_float_test_mouse_selection_reason(
  int clickCount,
  int isShiftSelection,
  char *output,
  size_t outputCapacity
) {
  if (output != NULL && outputCapacity > 0) {
    output[0] = '\0';
  }
  if (output == NULL || outputCapacity == 0) {
    return 0;
  }
  NSString *reason = PFMouseSelectionReason(
    (NSInteger)clickCount,
    isShiftSelection != 0
  );
  const char *rawReason = reason.UTF8String;
  size_t requiredCapacity = strlen(rawReason) + 1;
  if (requiredCapacity > outputCapacity) {
    return 0;
  }
  memcpy(output, rawReason, requiredCapacity);
  return 1;
}

static int PFTestResolveSelectionAnchor(
  double anchorX,
  double anchorY,
  const double *candidateBounds,
  const int *boundsAvailable,
  const size_t *candidateGroupIndices,
  size_t candidateCount,
  size_t groupCount,
  long *selectedGroupIndex
) {
  if (selectedGroupIndex != NULL) {
    *selectedGroupIndex = -1;
  }
  if (candidateCount > 0 &&
      (candidateBounds == NULL || boundsAvailable == NULL ||
       candidateGroupIndices == NULL)) {
    return (int)PFSelectionAnchorResolutionInvalidInput;
  }

  CGRect *rawBounds = candidateCount > 0
    ? calloc(candidateCount, sizeof(CGRect))
    : NULL;
  BOOL *rawAvailability = candidateCount > 0
    ? calloc(candidateCount, sizeof(BOOL))
    : NULL;
  if (candidateCount > 0 && (rawBounds == NULL || rawAvailability == NULL)) {
    free(rawBounds);
    free(rawAvailability);
    return (int)PFSelectionAnchorResolutionInvalidInput;
  }
  for (size_t index = 0; index < candidateCount; index += 1) {
    rawBounds[index] = CGRectMake(
      candidateBounds[index * 4],
      candidateBounds[index * 4 + 1],
      candidateBounds[index * 4 + 2],
      candidateBounds[index * 4 + 3]
    );
    rawAvailability[index] = boundsAvailable[index] != 0;
  }

  size_t rawSelectedIndex = SIZE_MAX;
  PFSelectionAnchorResolutionStatus status = PFResolveSelectionAnchorCandidate(
    CGPointMake(anchorX, anchorY),
    rawBounds,
    rawAvailability,
    candidateGroupIndices,
    candidateCount,
    groupCount,
    &rawSelectedIndex
  );
  free(rawBounds);
  free(rawAvailability);
  if (status == PFSelectionAnchorResolutionSuccess &&
      selectedGroupIndex != NULL &&
      rawSelectedIndex <= LONG_MAX) {
    *selectedGroupIndex = (long)rawSelectedIndex;
  }
  return (int)status;
}

int paper_float_test_resolve_selection_anchor(
  double anchorX,
  double anchorY,
  const double *candidateBounds,
  const int *boundsAvailable,
  size_t candidateCount,
  long *selectedIndex
) {
  size_t *identityGroups = candidateCount > 0
    ? calloc(candidateCount, sizeof(size_t))
    : NULL;
  if (candidateCount > 0 && identityGroups == NULL) {
    if (selectedIndex != NULL) {
      *selectedIndex = -1;
    }
    return (int)PFSelectionAnchorResolutionInvalidInput;
  }
  for (size_t index = 0; index < candidateCount; index += 1) {
    identityGroups[index] = index;
  }
  int status = PFTestResolveSelectionAnchor(
    anchorX,
    anchorY,
    candidateBounds,
    boundsAvailable,
    identityGroups,
    candidateCount,
    candidateCount,
    selectedIndex
  );
  free(identityGroups);
  return status;
}

int paper_float_test_resolve_selection_anchor_groups(
  double anchorX,
  double anchorY,
  const double *candidateBounds,
  const int *boundsAvailable,
  const size_t *candidateGroupIndices,
  size_t candidateCount,
  size_t groupCount,
  long *selectedGroupIndex
) {
  return PFTestResolveSelectionAnchor(
    anchorX,
    anchorY,
    candidateBounds,
    boundsAvailable,
    candidateGroupIndices,
    candidateCount,
    groupCount,
    selectedGroupIndex
  );
}

int paper_float_test_combine_selected_range_texts(
  const char *const *parts,
  const int *partsAvailable,
  size_t partCount,
  char *output,
  size_t outputCapacity
) {
  if (output != NULL && outputCapacity > 0) {
    output[0] = '\0';
  }
  if (partCount == 0 || parts == NULL || partsAvailable == NULL ||
      output == NULL || outputCapacity == 0) {
    return 0;
  }
  NSMutableArray *rawParts = [NSMutableArray arrayWithCapacity:partCount];
  for (size_t index = 0; index < partCount; index += 1) {
    NSString *part = partsAvailable[index] != 0 && parts[index] != NULL
      ? [NSString stringWithUTF8String:parts[index]]
      : nil;
    [rawParts addObject:part ?: (id)[NSNull null]];
  }
  NSString *combined = PFCombineSelectedRangeTextParts(rawParts);
  const char *rawCombined = combined == nil ? NULL : combined.UTF8String;
  if (rawCombined == NULL) {
    return 0;
  }
  size_t requiredCapacity = strlen(rawCombined) + 1;
  if (requiredCapacity > outputCapacity) {
    return 0;
  }
  memcpy(output, rawCombined, requiredCapacity);
  return 1;
}

int paper_float_test_preferred_parent_fallback_allowed(
  int rawAXError,
  int parentHasText
) {
  AXError error = (AXError)rawAXError;
  BOOL anchorReadFailed = error != kAXErrorSuccess &&
    !PFSelectionAttributeErrorIsSafeAbsence(error);
  return parentHasText != 0 &&
    PFPreferredSelectionShouldContinueToParent(NO, anchorReadFailed);
}

int paper_float_test_stable_range_read_can_emit(
  int identityValidBeforeRead,
  int allRangeTextRead,
  int identityValidAfterRead
) {
  return PFStableSelectedRangeReadCanEmit(
    identityValidBeforeRead != 0,
    allRangeTextRead != 0,
    identityValidAfterRead != 0
  );
}

int paper_float_test_preferred_direct_read_decision(
  int firstStatus,
  int secondStatus,
  int textsEqual
) {
  return (int)PFResolvePreferredDirectReadDecision(
    (PFDirectSelectedTextReadStatus)firstStatus,
    (PFDirectSelectedTextReadStatus)secondStatus,
    textsEqual != 0
  );
}

int paper_float_test_anchored_focused_direct_read_status(
  int selectedRangesStable,
  int anchorMatchesSelectedRangeBounds,
  int firstStatus,
  int secondStatus,
  int textsEqual
) {
  return (int)PFResolveAnchoredFocusedDirectReadStatus(
    selectedRangesStable != 0,
    anchorMatchesSelectedRangeBounds != 0,
    (PFDirectSelectedTextReadStatus)firstStatus,
    (PFDirectSelectedTextReadStatus)secondStatus,
    textsEqual != 0
  );
}

int paper_float_test_mouse_primary_requires_anchor_proof(
  int anchorElementValid,
  int focusedElementSafe,
  int focusedElementCoveredByPrimaryAncestry
) {
  return PFMousePrimaryPreferredElementRequiresAnchorProof(
    anchorElementValid != 0,
    focusedElementSafe != 0,
    focusedElementCoveredByPrimaryAncestry != 0
  );
}

int paper_float_test_direct_selected_text_may_emit(
  int decision,
  int requiresSelectionChangeEvidence,
  int selectionChangeObserved,
  int baselineComparable,
  int textMatchesBaseline
) {
  return PFDirectSelectedTextMayEmit(
    (PFPreferredDirectReadDecision)decision,
    requiresSelectionChangeEvidence != 0,
    selectionChangeObserved != 0,
    baselineComparable != 0,
    textMatchesBaseline != 0
  );
}

double paper_float_test_selection_baseline_maximum_ax_budget_seconds(void) {
  return PFSelectionBaselineMaximumAXBudgetSeconds();
}

double paper_float_test_selection_baseline_operation_timeout(
  double remainingSeconds
) {
  return PFSelectionBaselineOperationTimeoutForRemaining(remainingSeconds);
}

int paper_float_test_selection_baseline_supplemental_focus_allowed(
  double remainingSeconds
) {
  return PFSelectionBaselineMayAttemptSupplementalFocus(remainingSeconds) ? 1 : 0;
}

int paper_float_test_selection_baseline_maximum_depth(void) {
  return (int)kSelectionBaselineMaximumDepth;
}

int paper_float_test_selection_baseline_may_keep_partial(
  int allowPartial,
  unsigned long previouslyCapturedCount,
  int currentValueComplete,
  int budgetExhausted
) {
  return PFSelectionBaselineMayKeepPartial(
    allowPartial != 0,
    (NSUInteger)previouslyCapturedCount,
    currentValueComplete != 0,
    budgetExhausted != 0
  ) ? 1 : 0;
}

int paper_float_test_selection_baseline_timeout_was_accepted(int error) {
  return PFSelectionBaselineTimeoutWasAccepted((AXError)error) ? 1 : 0;
}

int paper_float_test_anchor_identity_revalidation(
  int withinFrozenWindow,
  int frozenAnchorAvailable,
  int exactElementMatch,
  int nearbyAncestryMatch
) {
  return PFAnchorIdentityRevalidationPasses(
    withinFrozenWindow != 0,
    frozenAnchorAvailable != 0,
    exactElementMatch != 0,
    nearbyAncestryMatch != 0
  );
}

int paper_float_test_focused_preferred_element_can_be_used(
  int belongsToTargetPID,
  int isWithinFrozenWindow,
  int isFrozenWindow,
  int duplicatesPrimaryAncestry,
  int sharesControlSubtree,
  int frameContainsAnchor
) {
  return PFFocusedPreferredElementCanBeUsed(
    belongsToTargetPID != 0,
    isWithinFrozenWindow != 0,
    isFrozenWindow != 0,
    duplicatesPrimaryAncestry != 0,
    sharesControlSubtree != 0,
    frameContainsAnchor != 0
  );
}

int paper_float_test_visible_work_area_for_anchor(
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
) {
  if (selectedDisplayID != NULL) {
    *selectedDisplayID = 0;
  }
  if (visibleWorkArea != NULL) {
    memset(visibleWorkArea, 0, sizeof(double) * 4);
  }
  if ((displayCount > 0 && (displayIDs == NULL || displayBounds == NULL)) ||
      (screenCount > 0 &&
       (screenDisplayIDs == NULL || screenFrames == NULL ||
        screenVisibleFrames == NULL))) {
    return (int)PFVisibleWorkAreaInvalidInput;
  }

  PFDisplayGeometry *displays = displayCount > 0
    ? calloc(displayCount, sizeof(PFDisplayGeometry))
    : NULL;
  PFScreenGeometry *screens = screenCount > 0
    ? calloc(screenCount, sizeof(PFScreenGeometry))
    : NULL;
  if ((displayCount > 0 && displays == NULL) ||
      (screenCount > 0 && screens == NULL)) {
    free(displays);
    free(screens);
    return (int)PFVisibleWorkAreaInvalidInput;
  }
  for (size_t index = 0; index < displayCount; index += 1) {
    displays[index].displayID = displayIDs[index];
    displays[index].bounds = CGRectMake(
      displayBounds[index * 4],
      displayBounds[index * 4 + 1],
      displayBounds[index * 4 + 2],
      displayBounds[index * 4 + 3]
    );
  }
  for (size_t index = 0; index < screenCount; index += 1) {
    screens[index].displayID = screenDisplayIDs[index];
    screens[index].frame = CGRectMake(
      screenFrames[index * 4],
      screenFrames[index * 4 + 1],
      screenFrames[index * 4 + 2],
      screenFrames[index * 4 + 3]
    );
    screens[index].visibleFrame = CGRectMake(
      screenVisibleFrames[index * 4],
      screenVisibleFrames[index * 4 + 1],
      screenVisibleFrames[index * 4 + 2],
      screenVisibleFrames[index * 4 + 3]
    );
  }

  CGDirectDisplayID rawSelectedDisplayID = kCGNullDirectDisplay;
  CGRect rawVisibleWorkArea = CGRectNull;
  PFVisibleWorkAreaStatus status = PFResolveVisibleWorkAreaForAnchor(
    CGPointMake(anchorX, anchorY),
    displays,
    displayCount,
    screens,
    screenCount,
    &rawSelectedDisplayID,
    &rawVisibleWorkArea
  );
  free(displays);
  free(screens);
  if (status == PFVisibleWorkAreaSuccess) {
    if (selectedDisplayID != NULL) {
      *selectedDisplayID = rawSelectedDisplayID;
    }
    if (visibleWorkArea != NULL) {
      visibleWorkArea[0] = rawVisibleWorkArea.origin.x;
      visibleWorkArea[1] = rawVisibleWorkArea.origin.y;
      visibleWorkArea[2] = rawVisibleWorkArea.size.width;
      visibleWorkArea[3] = rawVisibleWorkArea.size.height;
    }
  }
  return (int)status;
}

void paper_float_test_emit_selection_read_diagnostic(
  int outcome,
  long long generation,
  int attempt,
  double triggerToReadMs,
  int targetPID
) {
  NSString *status = outcome == 0 ? @"selection_read_found" : @"selection_read_empty";
  NSString *reason = outcome == 0
    ? @"test_success"
    : (outcome == 1 ? @"test_empty" : @"test_timeout");
  NSString *sourceBundleIdentifier = outcome == 1
    ? @"unknown_bundle_id"
    : @"dev.paperfloat.diagnostic-fixture";
  NSString *axError = outcome == 0
    ? @"none"
    : (outcome == 1 ? @"no_selected_text" : @"ax_timeout");
  PFEmitSelectionReadDiagnostic(
    status,
    reason,
    outcome == 0,
    outcome == 0 ? 2 : 0,
    outcome == 2 ? 250.0 : 4.5,
    sourceBundleIdentifier,
    axError,
    generation,
    (NSInteger)attempt,
    triggerToReadMs,
    outcome != 1,
    (pid_t)targetPID
  );
}

int paper_float_test_inject_tap_disabled(
  int tapKind,
  int timeout,
  int reenableSucceeds,
  int observerReady
) {
  __block int injected = 0;
  dispatch_block_t injectTapDisabled = ^{
    if (tapKind != 1 && tapKind != 2) {
      return;
    }
    if (tapKind == 2 &&
        (!gSelectionWatcherEnabled || gSelectionMultiClickQuietWindow <= 0)) {
      return;
    }
    NSInteger previousOverride = gPaperFloatTestTapReenableResult;
    BOOL previousObserverReady = gSelectionObserverSelectionNotificationReady;
    gPaperFloatTestTapReenableResult = reenableSucceeds != 0 ? 1 : 0;
    gSelectionObserverSelectionNotificationReady = observerReady != 0;
    CGEventType type = timeout != 0
      ? kCGEventTapDisabledByTimeout
      : kCGEventTapDisabledByUserInput;
    if (tapKind == 1) {
      PFHandleKeyTapDisabled(type);
    } else {
      PFHandleMouseTapDisabled(type);
    }
    injected = 1;
    gSelectionObserverSelectionNotificationReady = previousObserverReady;
    gPaperFloatTestTapReenableResult = previousOverride;
  };
  // This feature-only hook is intentionally synchronous. Rust holds its
  // acceptance-session epoch lock through this call, so authorization cannot
  // be cleared or re-armed between the check and native execution.
  if ([NSThread isMainThread]) {
    injectTapDisabled();
  } else {
    dispatch_sync(dispatch_get_main_queue(), injectTapDisabled);
  }
  return injected;
}

uint32_t paper_float_test_receiving_window_id(CGEventRef event) {
  return PFEventReceivingWindowID(event);
}

int paper_float_test_may_read_frozen_selection_node(
  int pidValid,
  int ancestryValid
) {
  return PFMayReadFrozenSelectionNode(pidValid != 0, ancestryValid != 0);
}

int paper_float_test_choose_gesture_window_id(
  uint32_t mouseDownWindowID,
  uint32_t mouseUpWindowID,
  uint32_t *selectedWindowID
) {
  CGWindowID selected = kCGNullWindowID;
  PFSelectionWindowFreezeStatus status = PFChooseGestureWindowID(
    mouseDownWindowID,
    mouseUpWindowID,
    &selected
  );
  if (selectedWindowID != NULL) {
    *selectedWindowID = selected;
  }
  return (int)status;
}

int paper_float_test_choose_gesture_target_pid(
  int mouseDownPID,
  int mouseUpPID,
  int selfPID,
  int *selectedPID
) {
  pid_t selected = 0;
  PFSelectionWindowFreezeStatus status = PFChooseGestureTargetPID(
    (pid_t)mouseDownPID,
    (pid_t)mouseUpPID,
    NO,
    (pid_t)selfPID,
    &selected
  );
  if (selectedPID != NULL) {
    *selectedPID = (int)selected;
  }
  return (int)status;
}

int paper_float_test_choose_anchored_gesture_target_pid(
  int mouseDownPID,
  int mouseUpPID,
  int selfPID,
  int *selectedPID
) {
  pid_t selected = 0;
  PFSelectionWindowFreezeStatus status = PFChooseGestureTargetPID(
    (pid_t)mouseDownPID,
    (pid_t)mouseUpPID,
    YES,
    (pid_t)selfPID,
    &selected
  );
  if (selectedPID != NULL) {
    *selectedPID = (int)selected;
  }
  return (int)status;
}

int paper_float_test_choose_selection_window_resolution_strategy(
  int pidStatus,
  int windowStatus,
  int *failureStatus
) {
  PFSelectionWindowFreezeStatus failure = PFSelectionWindowFreezeSuccess;
  PFSelectionWindowResolutionStrategy strategy =
    PFChooseSelectionWindowResolutionStrategy(
      (PFSelectionWindowFreezeStatus)pidStatus,
      (PFSelectionWindowFreezeStatus)windowStatus,
      &failure
    );
  if (failureStatus != NULL) {
    *failureStatus = (int)failure;
  }
  return (int)strategy;
}

int paper_float_test_selection_commit_validation(
  int stateCurrent,
  int requiresAnchorWindowRevalidation,
  int anchoredWindowIdentityCurrent
) {
  return PFSelectionCommitValidationPasses(
    stateCurrent != 0,
    requiresAnchorWindowRevalidation != 0,
    anchoredWindowIdentityCurrent != 0
  );
}

int paper_float_test_selection_window_freeze(
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
  if (matchedAXIndex != NULL) {
    *matchedAXIndex = -1;
  }
  if ((windowCount > 0 &&
       (windowIDs == NULL || ownerPIDs == NULL || layers == NULL ||
        onscreen == NULL || windowBounds == NULL)) ||
      (axCount > 0 && axBounds == NULL)) {
    return (int)PFSelectionWindowFreezeMetadataMissing;
  }

  PFWindowMetadataSnapshot *snapshots = windowCount > 0
    ? calloc(windowCount, sizeof(PFWindowMetadataSnapshot))
    : NULL;
  if (windowCount > 0 && snapshots == NULL) {
    return (int)PFSelectionWindowFreezeMetadataMissing;
  }
  for (size_t index = 0; index < windowCount; index += 1) {
    snapshots[index].windowID = windowIDs[index];
    snapshots[index].ownerPID = (pid_t)ownerPIDs[index];
    snapshots[index].layer = (NSInteger)layers[index];
    snapshots[index].isOnscreen = onscreen[index] != 0;
    snapshots[index].bounds = CGRectMake(
      windowBounds[index * 4],
      windowBounds[index * 4 + 1],
      windowBounds[index * 4 + 2],
      windowBounds[index * 4 + 3]
    );
  }

  CGRect validatedBounds = CGRectNull;
  PFSelectionWindowFreezeStatus status = PFValidateWindowMetadataSnapshots(
    frozenWindowID,
    (pid_t)targetPID,
    (pid_t)selfPID,
    snapshots,
    windowCount,
    &validatedBounds
  );
  free(snapshots);
  if (status != PFSelectionWindowFreezeSuccess) {
    return (int)status;
  }

  size_t matchCount = 0;
  size_t matchIndex = 0;
  for (size_t index = 0; index < axCount; index += 1) {
    CGRect candidateBounds = CGRectMake(
      axBounds[index * 4],
      axBounds[index * 4 + 1],
      axBounds[index * 4 + 2],
      axBounds[index * 4 + 3]
    );
    if (PFRectIsFiniteAndUsable(candidateBounds) &&
        PFBoundsMatch(candidateBounds, validatedBounds)) {
      matchIndex = index;
      matchCount += 1;
    }
  }
  if (matchCount == 0) {
    return (int)PFSelectionWindowFreezeAXWindowMissing;
  }
  if (matchCount != 1) {
    return (int)PFSelectionWindowFreezeAXWindowAmbiguous;
  }
  if (matchedAXIndex != NULL) {
    *matchedAXIndex = (long)matchIndex;
  }
  return (int)PFSelectionWindowFreezeSuccess;
}

int paper_float_test_observer_context_matches(
  long long frozenGeneration,
  long long activeGeneration,
  uint32_t contextWindowID,
  uint32_t lastMouseWindowID,
  int contextPID,
  int observerPID,
  int lastMousePID,
  int contextInvalid,
  int elementWithinFrozenWindow
) {
  return PFObserverFrozenIdentityMatches(
    YES,
    contextInvalid != 0,
    frozenGeneration,
    activeGeneration,
    frozenGeneration,
    (pid_t)contextPID,
    (pid_t)observerPID,
    (pid_t)lastMousePID,
    contextWindowID,
    lastMouseWindowID,
    elementWithinFrozenWindow != 0
  );
}

int paper_float_test_observer_mouse_correlation_decision(
  int recentMouseGesture,
  int mouseGenerationCompleted,
  int mouseGestureInvalid,
  int samePID
) {
  return (int)PFResolveObserverMouseCorrelation(
    recentMouseGesture != 0,
    mouseGenerationCompleted != 0,
    mouseGestureInvalid != 0,
    samePID != 0
  );
}

int paper_float_test_completed_mouse_echo_needs_text_check(
  int recentMouseGesture,
  int mouseGenerationCompleted,
  int samePID,
  int completedMouseTextAvailable
) {
  return PFCompletedMouseObserverEchoNeedsTextCheck(
    recentMouseGesture != 0,
    mouseGenerationCompleted != 0,
    samePID != 0,
    completedMouseTextAvailable != 0
  );
}

int paper_float_test_generation_is_current(
  long long frozenGeneration,
  long long activeGeneration,
  long long frozenLifecycle,
  long long activeLifecycle
) {
  return PFSelectionReadStateIsCurrent(
    frozenGeneration,
    activeGeneration,
    0,
    frozenLifecycle,
    activeLifecycle,
    5,
    5
  );
}
#endif

static void PFStopTap(CFMachPortRef *tap, CFRunLoopSourceRef *source) {
  if (*tap != NULL) {
    CGEventTapEnable(*tap, false);
  }

  if (*source != NULL) {
    CFRunLoopRemoveSource(CFRunLoopGetMain(), *source, kCFRunLoopCommonModes);
    CFRelease(*source);
    *source = NULL;
  }

  if (*tap != NULL) {
    CFMachPortInvalidate(*tap);
    CFRelease(*tap);
    *tap = NULL;
  }
}

static void PFStartPasteboardWatcher(void) {
  gKeyEventTapHealthy = NO;
  CGEventMask eventMask = CGEventMaskBit(kCGEventKeyDown);
  gKeyEventTap = CGEventTapCreate(
    kCGSessionEventTap,
    kCGHeadInsertEventTap,
    kCGEventTapOptionListenOnly,
    eventMask,
    PFKeyEventTapCallback,
    NULL
  );

  if (gKeyEventTap != NULL) {
    gKeyRunLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, gKeyEventTap, 0);
    if (gKeyRunLoopSource != NULL) {
      CFRunLoopAddSource(CFRunLoopGetMain(), gKeyRunLoopSource, kCFRunLoopCommonModes);
      CGEventTapEnable(gKeyEventTap, true);
      gKeyEventTapHealthy = CGEventTapIsEnabled(gKeyEventTap);
    }
  }

  if (gKeyEventTapHealthy) {
    PFEmit(PaperFloatEventPasteboardStatus, @"ready", 0, 0, 0);
  } else {
    PFStopTap(&gKeyEventTap, &gKeyRunLoopSource);
    PFEmit(PaperFloatEventPasteboardStatus, @"key_tap_unavailable_fallback_ready", 0, 0, 0);
  }

  gPasteboardLastChangeCount = [NSPasteboard generalPasteboard].changeCount;
  gPasteboardTimer = [NSTimer scheduledTimerWithTimeInterval:kPasteboardPollInterval repeats:YES block:^(NSTimer *timer) {
    (void)timer;
    NSInteger changeCount = [NSPasteboard generalPasteboard].changeCount;
    if (changeCount != gPasteboardLastChangeCount) {
      NSInteger delta = changeCount > gPasteboardLastChangeCount
        ? changeCount - gPasteboardLastChangeCount
        : 1;
      gPasteboardLastChangeCount = changeCount;
      if (!gKeyEventTapHealthy) {
        NSString *text = [[NSPasteboard generalPasteboard] stringForType:NSPasteboardTypeString] ?: @"";
        PFHandlePasteboardFallbackCopy(text, delta);
      }
      PFEmit(PaperFloatEventPasteboardChange, @"", 0, 0, (long long)delta);
    }
  }];
}

static void PFStartSelectionWatcher(void) {
  if (!gSelectionWatcherEnabled) {
    PFEmitSelectionCapabilityStatus();
    return;
  }
  gSelectionMultiClickQuietWindow = PFConfiguredMultiClickQuietWindow();
  if (gSelectionMultiClickQuietWindow <= 0) {
    PFEmit(
      PaperFloatEventSelectionStatus,
      @"selection_multi_click_quiet_window_invalid",
      0,
      0,
      0
    );
    return;
  }
  BOOL accessibilityTrusted = AXIsProcessTrusted();
  gMouseEventTapHealthy = NO;
  gSelectionAXMessagingTimeoutHealthy =
    PFSetGlobalAXMessagingTimeout(kAXMessagingTimeoutSeconds);
  PFStartWorkspaceActivationWatcher();

  CGEventMask eventMask =
    CGEventMaskBit(kCGEventLeftMouseDown) |
    CGEventMaskBit(kCGEventLeftMouseDragged) |
    CGEventMaskBit(kCGEventLeftMouseUp);

  gMouseEventTap = CGEventTapCreate(
    kCGSessionEventTap,
    kCGHeadInsertEventTap,
    kCGEventTapOptionDefault,
    eventMask,
    PFMouseEventTapCallback,
    NULL
  );

  if (gMouseEventTap != NULL) {
    gMouseRunLoopSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, gMouseEventTap, 0);
    if (gMouseRunLoopSource != NULL) {
      CFRunLoopAddSource(CFRunLoopGetMain(), gMouseRunLoopSource, kCFRunLoopCommonModes);
      CGEventTapEnable(gMouseEventTap, true);
      gMouseEventTapHealthy = CGEventTapIsEnabled(gMouseEventTap);
    }
  }

  if (!gMouseEventTapHealthy) {
    PFStopTap(&gMouseEventTap, &gMouseRunLoopSource);
  }

  if (accessibilityTrusted && gSelectionAXMessagingTimeoutHealthy) {
    PFStartSelectionObserver(PFFrontmostPID());
  }
  PFEmitSelectionCapabilityStatus();
}

static void PFStopWatchers(void) {
  BOOL hadSelectionResources =
    gSelectionWatcherEnabled ||
    gSelectionObserver != NULL ||
    gMouseEventTap != NULL ||
    gWorkspaceActivationObserver != nil;
  gSelectionWatcherEnabled = NO;
  PFAdvanceCounter(&gWatcherLifecycleToken);
  [gPasteboardTimer invalidate];
  gPasteboardTimer = nil;
  PFStopWorkspaceActivationWatcher();
  PFStopSelectionObserver();
  PFStopTap(&gKeyEventTap, &gKeyRunLoopSource);
  PFStopTap(&gMouseEventTap, &gMouseRunLoopSource);
  gKeyEventTapHealthy = NO;
  gMouseEventTapHealthy = NO;
  gSelectionMultiClickQuietWindow = 0;
  gPasteboardLastCopyShortcutAt = 0;
  gPasteboardLastFallbackCopyAt = 0;
  gPasteboardLastFallbackText = nil;
  gSelectionDidDrag = NO;
  gSelectionHasMouseDownLocation = NO;
  gSelectionIgnoreCurrentMouseGesture = NO;
  gSelectionMouseDownTargetPID = 0;
  gSelectionMouseDownTargetPIDAnchored = NO;
  gSelectionMouseDownWindowID = kCGNullWindowID;
  gSelectionMouseGeneration = 0;
  gSelectionLastEmittedText = nil;
  gSelectionLastEmittedAt = 0;
  gSelectionLastEmittedGeneration = 0;
  PFInvalidateSelectionReads();
  if (hadSelectionResources) {
    (void)PFSetGlobalAXMessagingTimeout(0);
  }
}

#if defined(PAPER_FLOAT_NATIVE_TESTING)
int paper_float_test_pending_frozen_activation_decision(
  int completed,
  int invalid,
  int windowIDAvailable,
  double elapsedSeconds,
  int activatedPID
) {
  long long previousGeneration = gSelectionGeneration;
  long long previousCompletedGeneration = gSelectionCompletedGeneration;
  gSelectionGeneration = 88;
  gSelectionCompletedGeneration = completed != 0 ? 88 : 0;
  gSelectionLastMouseUpGeneration = 88;
  gSelectionLastMouseUpPID = 400;
  gSelectionLastMouseUpAnchor = CGPointMake(30, 40);
  gSelectionLastMouseUpAt = 100;
  gSelectionLastMouseUpWindowID = windowIDAvailable != 0 ? 101 : kCGNullWindowID;
  gSelectionLastMouseGestureInvalid = invalid != 0;
  BOOL result = PFShouldPreservePendingFrozenReadOnActivation(
    PFHasPendingFrozenMouseSelectionAt(100 + elapsedSeconds),
    (pid_t)activatedPID,
    gSelectionLastMouseUpPID
  );
  PFClearFrozenMouseSelectionContext();
  gSelectionGeneration = previousGeneration;
  gSelectionCompletedGeneration = previousCompletedGeneration;
  return result;
}

int paper_float_test_frozen_state_cleanup(int stopWatchers) {
  PFSelectionReadContext *seedContext = [[PFSelectionReadContext alloc] init];
  seedContext.generation = 77;
  seedContext.targetPID = 400;
  seedContext.targetWindowID = 101;
  gSelectionLastMouseUpGeneration = 77;
  gSelectionLastMouseUpPID = 400;
  gSelectionLastMouseUpAnchor = CGPointMake(-20, 30);
  gSelectionLastMouseUpAt = 10;
  gSelectionLastMouseUpWindowID = 101;
  gSelectionLastMouseGestureInvalid = YES;
  gSelectionLastMouseContext = seedContext;
  gSelectionMouseDownWindowID = 202;

  if (stopWatchers != 0) {
    PFStopWatchers();
  } else {
    (void)PFBeginSelectionGeneration();
  }

  BOOL frozenContextCleared =
    gSelectionLastMouseUpGeneration == 0 &&
    gSelectionLastMouseUpPID == 0 &&
    CGPointEqualToPoint(gSelectionLastMouseUpAnchor, CGPointZero) &&
    gSelectionLastMouseUpAt == 0 &&
    gSelectionLastMouseUpWindowID == kCGNullWindowID &&
    !gSelectionLastMouseGestureInvalid &&
    gSelectionLastMouseContext == nil;
  BOOL gestureStateCorrect = stopWatchers == 0 ||
    gSelectionMouseDownWindowID == kCGNullWindowID;
  gSelectionMouseDownWindowID = kCGNullWindowID;
  return frozenContextCleared && gestureStateCorrect;
}
#endif

static void PFEmitWatcherContextRelease(
  void *context,
  PaperFloatWatcherCallback callback,
  long long lifecycleToken
) {
  if (callback != NULL && context != NULL) {
    callback(
      context,
      PaperFloatEventWatcherContextRelease,
      "",
      0,
      0,
      lifecycleToken,
      0
    );
  }
}

static void PFReleaseSupersededWatcherContext(
  void *context,
  PaperFloatWatcherCallback callback
) {
  // This context was never installed. Token zero releases the foreign owner
  // without pretending that the currently installed watcher lifecycle ended.
  PFEmitWatcherContextRelease(context, callback, 0);
}

static void PFReleaseCurrentWatcherContext(void) {
  PaperFloatWatcherCallback oldCallback = gCallback;
  void *oldContext = gContext;

  // Clear the active slot before invoking foreign code. A re-entrant start/stop can only
  // enqueue work after this serial main-queue operation and can never observe this context again.
  gCallback = NULL;
  gContext = NULL;

  PFEmitWatcherContextRelease(oldContext, oldCallback, gWatcherLifecycleToken);
}

static NSObject *PFReplaceWatcherSubmissionToken(void) {
  NSObject *token = [[NSObject alloc] init];
  os_unfair_lock_lock(&gWatcherSubmissionLock);
  gWatcherSubmissionToken = token;
  os_unfair_lock_unlock(&gWatcherSubmissionLock);
  return token;
}

static BOOL PFWatcherSubmissionIsCurrent(NSObject *token) {
  os_unfair_lock_lock(&gWatcherSubmissionLock);
  BOOL isCurrent = gWatcherSubmissionToken == token;
  os_unfair_lock_unlock(&gWatcherSubmissionLock);
  return isCurrent;
}

static void PFRunOnMainQueue(dispatch_block_t block) {
  if ([NSThread isMainThread]) {
    block();
  } else {
    dispatch_async(dispatch_get_main_queue(), block);
  }
}

static void PFWriteNativeResourceSnapshot(PaperFloatNativeResourceSnapshot *snapshot) {
  memset(snapshot, 0, sizeof(*snapshot));
  snapshot->version = 1;
  snapshot->selection_observer_count = gSelectionObserver != NULL ? 1 : 0;
  snapshot->selection_observer_source_count =
    gSelectionObserverRunLoopSource != NULL ? 1 : 0;
  snapshot->key_event_tap_count = gKeyEventTap != NULL ? 1 : 0;
  snapshot->key_event_tap_source_count = gKeyRunLoopSource != NULL ? 1 : 0;
  snapshot->mouse_event_tap_count = gMouseEventTap != NULL ? 1 : 0;
  snapshot->mouse_event_tap_source_count = gMouseRunLoopSource != NULL ? 1 : 0;
  snapshot->pasteboard_timer_count = gPasteboardTimer != nil ? 1 : 0;
  snapshot->workspace_activation_observer_count =
    gWorkspaceActivationObserver != nil ? 1 : 0;
  snapshot->callback_count = gCallback != NULL ? 1 : 0;
  snapshot->context_count = gContext != NULL ? 1 : 0;

  // An effective source set is one callback/context owner with at least one live
  // ingress source. The count is derived from live resources, never start counters.
  uint32_t totalSourceCount =
    snapshot->selection_observer_source_count +
    snapshot->key_event_tap_source_count +
    snapshot->mouse_event_tap_source_count +
    snapshot->pasteboard_timer_count +
    snapshot->workspace_activation_observer_count;
  snapshot->total_source_count = totalSourceCount;
  snapshot->effective_source_set_count =
    snapshot->callback_count == 1 &&
    snapshot->context_count == 1 &&
    totalSourceCount > 0
      ? 1
      : 0;
  snapshot->watcher_lifecycle_token = gWatcherLifecycleToken;
}

int paper_float_native_resource_snapshot(PaperFloatNativeResourceSnapshot *snapshot) {
  if (snapshot == NULL) {
    return 0;
  }

  if ([NSThread isMainThread]) {
    PFWriteNativeResourceSnapshot(snapshot);
  } else {
    dispatch_sync(dispatch_get_main_queue(), ^{
      PFWriteNativeResourceSnapshot(snapshot);
    });
  }
  return 1;
}

static int PFCopyVisibleWorkAreaForAnchor(
  CGPoint anchor,
  PaperFloatNativeVisibleWorkArea *workArea
) {
  memset(workArea, 0, sizeof(*workArea));
  if (!PFPointIsFinite(anchor)) {
    return 0;
  }

  uint32_t displayCount = 0;
  CGError displayError = CGGetActiveDisplayList(0, NULL, &displayCount);
  if (displayError != kCGErrorSuccess || displayCount == 0) {
    return 0;
  }

  CGDirectDisplayID *displayIDs = calloc(displayCount, sizeof(CGDirectDisplayID));
  if (displayIDs == NULL) {
    return 0;
  }
  uint32_t resolvedDisplayCount = displayCount;
  displayError = CGGetActiveDisplayList(displayCount, displayIDs, &resolvedDisplayCount);
  if (displayError != kCGErrorSuccess || resolvedDisplayCount != displayCount) {
    free(displayIDs);
    return 0;
  }

  PFDisplayGeometry *displays = calloc(displayCount, sizeof(PFDisplayGeometry));
  NSArray<NSScreen *> *nativeScreens = NSScreen.screens;
  PFScreenGeometry *screens = nativeScreens.count > 0
    ? calloc(nativeScreens.count, sizeof(PFScreenGeometry))
    : NULL;
  if (displays == NULL || screens == NULL) {
    free(displayIDs);
    free(displays);
    free(screens);
    return 0;
  }

  for (uint32_t index = 0; index < displayCount; index += 1) {
    displays[index].displayID = displayIDs[index];
    displays[index].bounds = CGDisplayBounds(displayIDs[index]);
  }
  for (NSUInteger index = 0; index < nativeScreens.count; index += 1) {
    NSScreen *screen = nativeScreens[index];
    id rawScreenNumber = screen.deviceDescription[@"NSScreenNumber"];
    screens[index].displayID = [rawScreenNumber isKindOfClass:[NSNumber class]]
      ? [(NSNumber *)rawScreenNumber unsignedIntValue]
      : kCGNullDirectDisplay;
    screens[index].frame = NSRectToCGRect(screen.frame);
    screens[index].visibleFrame = NSRectToCGRect(screen.visibleFrame);
  }

  CGDirectDisplayID selectedDisplayID = kCGNullDirectDisplay;
  CGRect resolved = CGRectNull;
  PFVisibleWorkAreaStatus status = PFResolveVisibleWorkAreaForAnchor(
    anchor,
    displays,
    displayCount,
    screens,
    nativeScreens.count,
    &selectedDisplayID,
    &resolved
  );
  free(displayIDs);
  free(displays);
  free(screens);
  if (status != PFVisibleWorkAreaSuccess) {
    return 0;
  }

  workArea->version = 1;
  workArea->display_id = selectedDisplayID;
  workArea->x = resolved.origin.x;
  workArea->y = resolved.origin.y;
  workArea->width = resolved.size.width;
  workArea->height = resolved.size.height;
  return 1;
}

int paper_float_visible_work_area_for_anchor(
  double anchorX,
  double anchorY,
  PaperFloatNativeVisibleWorkArea *workArea
) {
  if (workArea == NULL) {
    return 0;
  }

  __block int result = 0;
  dispatch_block_t readWorkArea = ^{
    result = PFCopyVisibleWorkAreaForAnchor(
      CGPointMake(anchorX, anchorY),
      workArea
    );
  };
  if ([NSThread isMainThread]) {
    readWorkArea();
  } else {
    dispatch_sync(dispatch_get_main_queue(), readWorkArea);
  }
  return result;
}

double paper_float_multi_click_quiet_window_seconds(void) {
  __block NSTimeInterval result = 0;
  dispatch_block_t readCapturedWindow = ^{
    NSTimeInterval captured = gSelectionMultiClickQuietWindow;
    if (isfinite(captured) &&
        captured > 0 &&
        captured <= kSelectionMultiClickMaximumQuietWindow) {
      result = captured;
    }
  };
  if ([NSThread isMainThread]) {
    readCapturedWindow();
  } else {
    dispatch_sync(dispatch_get_main_queue(), readCapturedWindow);
  }
  return result;
}

int paper_float_frontmost_external_pid(void) {
  __block pid_t result = 0;
  dispatch_block_t readFrontmost = ^{
    pid_t candidate = PFFrontmostPID();
    if (candidate > 0 && !PFIsSelfPID(candidate)) {
      result = candidate;
    }
  };

  if ([NSThread isMainThread]) {
    readFrontmost();
  } else {
    dispatch_sync(dispatch_get_main_queue(), readFrontmost);
  }
  return (int)result;
}

void paper_float_restore_application_focus(int rawPID) {
  pid_t targetPID = (pid_t)rawPID;
  PFRunOnMainQueue(^{
    if (targetPID <= 0 || !PFIsSelfPID(PFFrontmostPID())) {
      return;
    }

    NSRunningApplication *target =
      [NSRunningApplication runningApplicationWithProcessIdentifier:targetPID];
    if (target == nil || target.terminated || PFIsSelfPID(target.processIdentifier)) {
      return;
    }
    (void)[target activateWithOptions:0];
  });
}

void paper_float_watchers_stop(void) {
  NSObject *submissionToken = PFReplaceWatcherSubmissionToken();
  PFRunOnMainQueue(^{
    if (!PFWatcherSubmissionIsCurrent(submissionToken)) {
      return;
    }
    PFStopWatchers();
    PFReleaseCurrentWatcherContext();
  });
}

void paper_float_watchers_start(
  void *context,
  PaperFloatWatcherCallback callback,
  int selectionEnabled
) {
  NSObject *submissionToken = PFReplaceWatcherSubmissionToken();
  PFRunOnMainQueue(^{
    if (!PFWatcherSubmissionIsCurrent(submissionToken)) {
      PFReleaseSupersededWatcherContext(context, callback);
      return;
    }
    PFStopWatchers();
    PFReleaseCurrentWatcherContext();
    gContext = context;
    gCallback = callback;
    gSelectionWatcherEnabled = selectionEnabled != 0;
    PFStartPasteboardWatcher();
    if (gSelectionWatcherEnabled) {
      PFStartSelectionWatcher();
    } else {
      PFEmitSelectionCapabilityStatus();
    }
  });
}
