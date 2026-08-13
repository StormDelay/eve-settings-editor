// Component test: vitest + jsdom.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/svelte";
import Chip from "./Chip.svelte";
import { text } from "./snippet";
import "$lib/test/setup";

const style = /<style>([\s\S]*)<\/style>/.exec(
  readFileSync(resolve(import.meta.dirname, "Chip.svelte"), "utf8"),
)?.[1] ?? "";

describe("Chip", () => {
  // The direct guard on v0.34's `.chip.ghost { border-style: dashed; opacity:
  // 0.85 }`. A proposed pairing is the one thing on the card that needs an
  // answer, and it was drawn as a settled chip MINUS contrast — which is why it
  // was reported as not visible enough. Dashed is kept; the subtraction is not.
  test("declares no opacity at all — 'proposed' is never dimness", () => {
    expect(style).not.toMatch(/opacity:/);
  });

  test("proposed is carried by a dashed border", () => {
    expect(style).toMatch(/\.proposed\s*\{[^}]*border-style:\s*dashed/);
  });

  test("a proposed chip defaults to the info tone, so it reads louder than settled neighbours", () => {
    render(Chip, { state: "proposed", title: "From your launcher log", children: text("Astra") });
    expect(screen.getByTitle("From your launcher log").className).toMatch(/\binfo\b/);
  });

  test("a settled chip defaults to neutral", () => {
    render(Chip, { children: text("Astra") });
    const el = screen.getByText("Astra").closest("span.chip");
    expect(el?.className).not.toMatch(/\binfo\b/);
  });

  // The title is the accessible description of why this chip differs. The copy
  // already exists at the call site, so requiring it costs nothing.
  test("proposed without a title throws in dev", () => {
    expect(() => render(Chip, { state: "proposed", children: text("Astra") })).toThrow(/proposed/);
  });
});
