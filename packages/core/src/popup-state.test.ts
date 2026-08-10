import { describe, expect, it } from "vitest";
import {
  HIDDEN_POPUP_STATE,
  POPUP_PROTOCOL_VERSION,
  assertPopupProtocol,
  reconcilePopupState,
  type PopupState
} from "./types";

function snapshot(revision: number, selectionRevision = 1): PopupState {
  return {
    protocolVersion: POPUP_PROTOCOL_VERSION,
    revision,
    selectionRevision,
    visible: true,
    status: "selection_ready",
    selectedText: "same text",
    cleanedText: "same text",
    pinned: false
  };
}

describe("popup snapshot reconciliation", () => {
  it("keeps an event received before an older late-subscription snapshot", () => {
    const event = snapshot(9);
    const staleSnapshot = snapshot(8);

    const afterEvent = reconcilePopupState(null, event);
    const afterSnapshot = reconcilePopupState(afterEvent, staleSnapshot);

    expect(afterSnapshot).toBe(event);
  });

  it("accepts a newer post-subscription snapshot", () => {
    const event = snapshot(8);
    const latestSnapshot = snapshot(9);

    expect(reconcilePopupState(event, latestSnapshot)).toBe(latestSnapshot);
  });

  it("rejects incompatible protocols and inconsistent visibility", () => {
    expect(() =>
      assertPopupProtocol({
        ...snapshot(1),
        protocolVersion: 2 as typeof POPUP_PROTOCOL_VERSION
      })
    ).toThrow(/Unsupported popup protocol/);
    expect(() => assertPopupProtocol({ ...snapshot(1), visible: false })).toThrow(/visibility/);
  });

  it("requires typed error recovery that matches the failure category", () => {
    const errorState: PopupState = {
      ...snapshot(1),
      status: "error",
      error: "Input Monitoring is denied.",
      errorKind: "permission",
      retryable: false,
      recoveryAction: "open_input_monitoring"
    };

    expect(() => assertPopupProtocol(errorState)).not.toThrow();
    expect(() => assertPopupProtocol({ ...errorState, recoveryAction: "open_accessibility" })).not.toThrow();
    expect(() => assertPopupProtocol({ ...errorState, recoveryAction: "retry_translation" })).toThrow(/retryability/);
    expect(() =>
      assertPopupProtocol({ ...errorState, errorKind: "configuration", recoveryAction: "open_input_monitoring" })
    ).toThrow(/Configuration/);
    expect(() =>
      assertPopupProtocol({ ...errorState, errorKind: "selection", recoveryAction: "open_settings" })
    ).toThrow(/Selection/);
  });

  it("ignores 1,000 out-of-order revisions", () => {
    let current: PopupState = HIDDEN_POPUP_STATE;
    for (let revision = 1; revision <= 1_000; revision += 1) {
      current = reconcilePopupState(current, snapshot(revision, revision));
    }
    for (let revision = 999; revision >= 1; revision -= 1) {
      current = reconcilePopupState(current, snapshot(revision, revision));
    }

    expect(current.revision).toBe(1_000);
    expect(current.selectionRevision).toBe(1_000);
  });
});
