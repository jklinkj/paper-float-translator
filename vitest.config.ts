import { defineConfig } from "vitest/config";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export default defineConfig({
  test: {
    include: ["packages/**/*.test.ts", "apps/**/*.test.ts"],
    environment: "node"
  },
  resolve: {
    alias: {
      "@paper-float-translator/core": resolve(__dirname, "packages/core/src/index.ts"),
      "@paper-float-translator/deepseek": resolve(__dirname, "packages/deepseek/src/index.ts"),
      "@paper-float-translator/storage": resolve(__dirname, "packages/storage/src/index.ts")
    }
  }
});
