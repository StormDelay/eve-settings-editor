import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

// One runner for both suites. `*.test.ts` is a pure-module test (plain data in,
// plain data out); `*.spec.ts` mounts a component. The naming split is kept
// because it says at a glance what a file costs to run — it is no longer two
// runners. See src/lib/test/check.ts for why the second one went away.
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
    include: ["src/**/*.{test,spec}.ts"],
    setupFiles: ["./src/lib/test/setup.ts"],
    restoreMocks: true,
  },
});
