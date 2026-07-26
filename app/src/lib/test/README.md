# Frontend tests

Two suites, split by file extension so neither runner picks up the other's
files. `npm test` runs both.

| Suite | Files | Runner | For |
|---|---|---|---|
| pure modules | `src/lib/*.test.ts` | `node --test`, no framework | logic extracted out of components, plus the IPC contract test |
| components | `src/lib/*.spec.ts` | `vitest` + `jsdom` + `@testing-library/svelte` | mounting a component, firing events, asserting on the DOM and on what it sends over IPC |

Run one suite on its own with `npm run test:ui` (components) or
`node --test "src/lib/**/*.test.ts"` (pure modules).

## What gets mocked

Exactly one thing: `invoke` from `@tauri-apps/api/core`, in `setup.ts`.

Everything above it stays real, so `api.ts` builds the actual request and a test
can assert on the command name and argument names it produced. Mocking `api.ts`
itself would hide the bugs most worth catching — a wrong command name, a wrong
argument name, a `Mutation` assembled from the wrong field.

```ts
import { calls } from "$lib/test/setup";

calls.stub("setup_preview", plan);          // fixed reply
calls.stub("setup_preview", (args) => …);   // or compute one, or return a Promise
calls.only("setup_apply").args;             // the single call, or fail loudly
calls.of("setup_preview");                  // every call, in order
calls.never("setup_apply");                 // assert nothing was written
```

An unstubbed command resolves to `undefined` rather than throwing: a component
usually fires several calls on mount and only one of them is the subject.
`calls` is reset after every test, along with the rendered DOM.

## Two traps

**`await` every `fireEvent`.** It returns a promise that flushes Svelte's tick.
Without the await, the DOM property the click set is visible immediately but the
component has not re-rendered, so the next assertion reads a half-updated
screen. This costs real debugging time — the symptom is an assertion that fails
against a value the component demonstrably does produce.

**Do not assert on the element you just clicked to prove the component
reacted.** Clicking a checkbox sets `.checked` in the DOM by itself, so
`await waitFor(() => expect(box.checked).toBe(true))` passes whether or not the
component did anything. Assert on something the component controls — another
row, a `disabled` attribute, an IPC call.

## Queries

Prefer roles and visible text, but scope them: labels repeat across panels
("x" and "y" belong to both the fighter UI and the notification badge; a
filename appears in both the source dropdown and the target list). Both existing
spec files find the enclosing section or group first and query inside it. A
global `getByText` that works today breaks the moment a second panel reuses the
word.
