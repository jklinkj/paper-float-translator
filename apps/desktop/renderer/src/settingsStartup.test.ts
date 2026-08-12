import { describe, expect, it, vi } from "vitest";
import { isTransientSettingsStateRace, loadSettingsAfterStartup } from "./settingsStartup";

const STATE_RACE =
  "state not managed for field `state` on command `get_settings`. You must call `.manage()` before using this command";

describe("settings startup", () => {
  it("recognizes only the bounded Tauri get_settings initialization race", () => {
    expect(isTransientSettingsStateRace(STATE_RACE)).toBe(true);
    expect(isTransientSettingsStateRace(new Error(STATE_RACE))).toBe(true);
    expect(isTransientSettingsStateRace("读取设置文件失败")).toBe(false);
    expect(isTransientSettingsStateRace(null)).toBe(false);
  });

  it("returns immediately when state is already managed", async () => {
    const load = vi.fn().mockResolvedValue({ ready: true });
    const sleep = vi.fn().mockResolvedValue(undefined);

    await expect(loadSettingsAfterStartup(load, sleep)).resolves.toEqual({ ready: true });
    expect(load).toHaveBeenCalledTimes(1);
    expect(sleep).not.toHaveBeenCalled();
  });

  it("retries the transient state race and then succeeds", async () => {
    const load = vi
      .fn()
      .mockRejectedValueOnce(STATE_RACE)
      .mockRejectedValueOnce(new Error(STATE_RACE))
      .mockResolvedValue({ ready: true });
    const sleep = vi.fn().mockResolvedValue(undefined);

    await expect(loadSettingsAfterStartup(load, sleep)).resolves.toEqual({ ready: true });
    expect(load).toHaveBeenCalledTimes(3);
    expect(sleep).toHaveBeenCalledTimes(2);
  });

  it("does not retry real settings failures", async () => {
    const load = vi.fn().mockRejectedValue("设置文件已损坏");
    const sleep = vi.fn().mockResolvedValue(undefined);

    await expect(loadSettingsAfterStartup(load, sleep)).rejects.toBe("设置文件已损坏");
    expect(load).toHaveBeenCalledTimes(1);
    expect(sleep).not.toHaveBeenCalled();
  });

  it("stops after the configured attempt bound", async () => {
    const load = vi.fn().mockRejectedValue(STATE_RACE);
    const sleep = vi.fn().mockResolvedValue(undefined);

    await expect(loadSettingsAfterStartup(load, sleep, 3)).rejects.toBe(STATE_RACE);
    expect(load).toHaveBeenCalledTimes(3);
    expect(sleep).toHaveBeenCalledTimes(2);
  });
});
