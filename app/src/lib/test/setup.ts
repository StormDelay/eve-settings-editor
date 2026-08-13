// Component-test setup. Mocks the ONE boundary a browser test cannot cross —
// Tauri's `invoke` — and leaves everything above it real, so `api.ts` builds
// the actual request shape and a test can assert on it. Mocking `api.ts` itself
// would hide exactly the bugs worth catching (wrong command name, wrong
// argument name, a Mutation built from the wrong field).
import { afterEach, vi } from "vitest";
import { cleanup } from "@testing-library/svelte";
import { resetSubject } from "../subject.svelte";
import { resetNames } from "../names.svelte";
import { resetRoster, resetAccountsSession } from "../accounts.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => calls.dispatch(cmd, args),
}));

// jsdom implements pointer EVENTS but not pointer CAPTURE, so any component
// that captures a drag — the layout canvas, the probe viewer — throws the
// moment a test presses on it.
//
// Stubbed here rather than guarded at the call sites: capture is real
// behaviour every real browser provides, and an optional call in the source
// would be test scaffolding leaking into the product. `hasPointerCapture`
// answers false, which is the truth once nothing is captured, and sends the
// release paths down the branch that skips.
if (!Element.prototype.setPointerCapture) {
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.releasePointerCapture = () => {};
  Element.prototype.hasPointerCapture = () => false;
}

// jsdom implements no layout, so it ships no `scrollIntoView` either. The
// window panel calls it to keep the selected row visible; without the stub that
// throws asynchronously, which surfaces as an unhandled error that fails the
// run while every test still reports green.
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {};
}

// jsdom has no ResizeObserver, and Svelte 5 implements `bind:clientWidth` with
// one — so mounting anything that measures itself (the layout canvas) throws
// during render, not at the point of measurement, which makes it look like a
// component bug rather than a missing browser API.
//
// A no-op is the honest stub: jsdom lays nothing out, so every element measures
// 0 and there is never a resize to report. Components that read a width must
// already tolerate 0 (a real browser reports it that way on the first frame
// too), and `canvasScale` does.
if (typeof globalThis.ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}

// @testing-library only auto-registers this when vitest runs with `globals`,
// which this config deliberately does not. Without it every render stays in the
// document and the next test's queries match two copies of everything.
//
// `resetSubject` is here rather than in each mounting suite's own `afterEach`,
// which is what `02-shell.md` §6.3 asks for. Same guarantee, one line, and it
// cannot be forgotten by the next suite that mounts the shell — which is the
// actual failure mode §6.3 is worried about, since the symptom (a test that
// passes alone and fails in sequence) gets blamed on the layout change rather
// than on the missing reset.
afterEach(() => {
  cleanup();
  calls.reset();
  resetSubject();
  // The other two module-level rune stores, for the same reason. `page.spec.ts`
  // documented this failure mode against exactly these two and worked around it
  // by waiting for every mount call to land; clearing them is the fix it was
  // working around.
  resetNames();
  resetRoster();
  // The capture and launcher runes, for the same reason. The Accounts sheet is
  // dismissable now, so these deliberately outlive an unmount — which is exactly
  // what makes them leak between tests without this.
  resetAccountsSession();
});

export interface InvokeCall {
  cmd: string;
  args: Record<string, unknown> | undefined;
}

class Calls {
  log: InvokeCall[] = [];
  private handlers = new Map<string, (args: Record<string, unknown> | undefined) => unknown>();

  dispatch(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
    this.log.push({ cmd, args });
    const h = this.handlers.get(cmd);
    if (!h) {
      // Unstubbed commands resolve to undefined rather than throwing: a
      // component under test usually fires several calls on mount and only one
      // of them is the subject.
      return Promise.resolve(undefined);
    }
    try {
      return Promise.resolve(h(args));
    } catch (e) {
      return Promise.reject(e);
    }
  }

  /// Make `cmd` return `value` (or throw, if `value` is an Error).
  stub(cmd: string, value: unknown | ((args: Record<string, unknown> | undefined) => unknown)) {
    this.handlers.set(cmd, typeof value === "function" ? (value as never) : () => value);
  }

  stubbed(cmd: string): boolean {
    return this.handlers.has(cmd);
  }

  reset() {
    this.log = [];
    this.handlers.clear();
  }

  /// Every call to `cmd`, in order.
  of(cmd: string): InvokeCall[] {
    return this.log.filter((c) => c.cmd === cmd);
  }

  /// The single call to `cmd`; fails loudly on none or several, because "it
  /// fired twice" is itself a bug worth failing on.
  only(cmd: string): InvokeCall {
    const hits = this.of(cmd);
    if (hits.length !== 1) {
      throw new Error(
        `expected exactly one \`${cmd}\` call, got ${hits.length}` +
          ` (all calls: ${this.log.map((c) => c.cmd).join(", ") || "none"})`,
      );
    }
    return hits[0];
  }

  /// Assert `cmd` was never sent.
  never(cmd: string) {
    const hits = this.of(cmd);
    if (hits.length) throw new Error(`expected no \`${cmd}\` call, got ${hits.length}`);
  }
}

export const calls = new Calls();
