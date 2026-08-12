const STARTUP_STATE_RETRY_ATTEMPTS = 8;
const STARTUP_STATE_RETRY_DELAY_MS = 25;

type Sleep = () => Promise<void>;

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return typeof error === "string" ? error : "";
}

export function isTransientSettingsStateRace(error: unknown): boolean {
  const message = errorMessage(error);
  return message.includes("state not managed") && message.includes("get_settings");
}

export async function loadSettingsAfterStartup<T>(
  load: () => Promise<T>,
  sleep: Sleep = () =>
    new Promise((resolve) => {
      window.setTimeout(resolve, STARTUP_STATE_RETRY_DELAY_MS);
    }),
  attempts = STARTUP_STATE_RETRY_ATTEMPTS
): Promise<T> {
  const boundedAttempts = Math.max(1, Math.floor(attempts));
  let lastError: unknown;

  for (let attempt = 0; attempt < boundedAttempts; attempt += 1) {
    try {
      return await load();
    } catch (error) {
      lastError = error;
      if (!isTransientSettingsStateRace(error) || attempt + 1 >= boundedAttempts) {
        throw error;
      }
      await sleep();
    }
  }

  throw lastError;
}
