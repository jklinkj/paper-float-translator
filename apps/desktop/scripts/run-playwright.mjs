import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "../../..");
const previewUrl = "http://127.0.0.1:4173";
const viteEntrypoint = resolve(repositoryRoot, "node_modules/vite/bin/vite.js");
const playwrightEntrypoint = resolve(
  repositoryRoot,
  "node_modules/@playwright/test/cli.js"
);
const viteConfig = resolve(repositoryRoot, "apps/desktop/vite.config.ts");

const delay = (milliseconds) =>
  new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));

async function endpointIsReady() {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 1_000);

  try {
    const response = await fetch(previewUrl, { signal: controller.signal });
    return response.ok;
  } catch {
    return false;
  } finally {
    clearTimeout(timeout);
  }
}

async function waitForPreview(server) {
  let exitResult;
  server.once("exit", (code, signal) => {
    exitResult = { code, signal };
  });

  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (exitResult) {
      throw new Error(
        `Vite preview exited before becoming ready (code=${String(exitResult.code)}, signal=${String(exitResult.signal)}).`
      );
    }
    if (await endpointIsReady()) {
      return;
    }
    await delay(250);
  }

  throw new Error("Vite preview did not become ready within 30 seconds.");
}

function waitForExit(child) {
  return new Promise((resolveExit, rejectExit) => {
    child.once("error", rejectExit);
    child.once("exit", (code, signal) => resolveExit({ code, signal }));
  });
}

async function stopOwnedPreview(server) {
  if (!server || server.exitCode !== null || server.signalCode !== null) {
    return;
  }

  const exit = waitForExit(server).catch(() => undefined);
  server.kill();
  await Promise.race([exit, delay(2_000)]);

  if (server.exitCode !== null || server.signalCode !== null) {
    return;
  }

  if (process.platform === "win32" && server.pid) {
    const taskkill = spawn(
      "taskkill.exe",
      ["/PID", String(server.pid), "/T", "/F"],
      { stdio: "ignore", windowsHide: true }
    );
    await waitForExit(taskkill).catch(() => undefined);
    return;
  }

  server.kill("SIGKILL");
  await exit;
}

let ownedPreview;

try {
  if (!(await endpointIsReady())) {
    ownedPreview = spawn(
      process.execPath,
      [
        viteEntrypoint,
        "preview",
        "--config",
        viteConfig,
        "--host",
        "127.0.0.1",
        "--port",
        "4173"
      ],
      {
        cwd: repositoryRoot,
        stdio: "inherit",
        windowsHide: true
      }
    );
    await waitForPreview(ownedPreview);
  }

  const playwright = spawn(
    process.execPath,
    [playwrightEntrypoint, "test", ...process.argv.slice(2)],
    {
      cwd: repositoryRoot,
      stdio: "inherit",
      windowsHide: true
    }
  );
  const result = await waitForExit(playwright);
  process.exitCode = result.code ?? 1;
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
} finally {
  await stopOwnedPreview(ownedPreview);
}
