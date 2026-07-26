import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

// Component tests only. The pure-module tests keep running under `node --test`
// with no framework at all (see package.json `test`). The two suites split by
// file extension so neither runner picks up the other's files: `*.test.ts` is
// node --test, `*.spec.ts` is vitest. `npm test` runs both.
export default defineConfig({
  plugins: [svelte({ hot: false })],
  resolve: {
    // Svelte 5 ships separate server and browser builds; jsdom needs the
    // browser one or `mount` has no DOM to attach to.
    conditions: ["browser"],
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.spec.ts"],
    setupFiles: ["./src/lib/test/setup.ts"],
    restoreMocks: true,
  },
});
