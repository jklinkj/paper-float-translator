import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig, externalizeDepsPlugin } from "electron-vite";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const workspaceAliases = {
  "@paper-float-translator/core": resolve(__dirname, "../../packages/core/src/index.ts"),
  "@paper-float-translator/deepseek": resolve(__dirname, "../../packages/deepseek/src/index.ts"),
  "@paper-float-translator/storage": resolve(__dirname, "../../packages/storage/src/index.ts")
};

const rendererAliases = {
  ...workspaceAliases,
  "@paper-float-translator/core": resolve(__dirname, "../../packages/core/src/renderer.ts")
};

export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin({ exclude: Object.keys(workspaceAliases) })],
    resolve: {
      alias: workspaceAliases
    },
    build: {
      outDir: "dist/main",
      rollupOptions: {
        input: resolve(__dirname, "electron/main.ts"),
        external: ["keytar"]
      }
    }
  },
  preload: {
    plugins: [externalizeDepsPlugin({ exclude: Object.keys(workspaceAliases) })],
    resolve: {
      alias: workspaceAliases
    },
    build: {
      outDir: "dist/preload",
      rollupOptions: {
        input: resolve(__dirname, "preload/index.ts")
      }
    }
  },
  renderer: {
    root: resolve(__dirname, "renderer"),
    plugins: [react()],
    resolve: {
      alias: rendererAliases
    },
    build: {
      outDir: resolve(__dirname, "dist/renderer"),
      emptyOutDir: true,
      rollupOptions: {
        input: resolve(__dirname, "renderer/index.html")
      }
    }
  }
});
