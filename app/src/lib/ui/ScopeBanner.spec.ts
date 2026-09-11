// Component test: vitest + jsdom.
import { describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/svelte";
import ScopeBanner from "./ScopeBanner.svelte";
import "$lib/test/setup";

describe("ScopeBanner", () => {
  test("states the scope", () => {
    render(ScopeBanner, { label: "Chat layout — account-wide" });
    expect(screen.getByRole("status").textContent).toContain("Chat layout — account-wide");
  });

  // Matching the four {#if sharedLabel} guards it replaces: an empty label means
  // there is no shared scope to report, not an empty box where one would go.
  test("renders nothing at all when the label is empty", () => {
    const { container } = render(ScopeBanner, { label: "" });
    expect(container.textContent).toBe("");
    expect(screen.queryByRole("status")).toBeNull();
  });

  // It is a statement of scope, not a warning. --warn was doing both jobs inside
  // ChatSplit's 135 lines — the legend and a real negative-area warning — and a
  // reader had no way to tell which was which.
  test("it is an info message, never an alert", () => {
    render(ScopeBanner, { label: "Overview — account-wide" });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByRole("status")).toBeTruthy();
  });

  test("a caller's class survives, so existing spec hooks keep working", () => {
    const { container } = render(ScopeBanner, { label: "Shared", class: "account-legend" });
    expect(container.querySelector(".account-legend")).toBeTruthy();
  });
});
