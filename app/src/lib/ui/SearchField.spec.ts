// Component test: vitest + jsdom.
import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import SearchField from "./SearchField.svelte";
import "$lib/test/setup";

describe("SearchField", () => {
  // Five boxes, two verbs and three placeholder conventions collapse into one
  // rule precisely because the placeholder is built rather than passed.
  test("filter builds 'Filter {nouns}…'", () => {
    render(SearchField, { nouns: "windows" });
    expect(screen.getByPlaceholderText("Filter windows…")).toBeTruthy();
  });

  test("search builds 'Search {nouns}'", () => {
    render(SearchField, { verb: "search", nouns: "commands and keys" });
    expect(screen.getByPlaceholderText("Search commands and keys")).toBeTruthy();
  });

  test("a shortcut is appended in parentheses", () => {
    render(SearchField, { verb: "search", nouns: "the tree", shortcut: "Ctrl+F" });
    expect(screen.getByPlaceholderText("Search the tree (Ctrl+F)")).toBeTruthy();
  });

  test("a count renders alone, and with a total renders 'n of m'", () => {
    const { unmount } = render(SearchField, { nouns: "rows", count: 3 });
    expect(screen.getByText("3")).toBeTruthy();
    unmount();

    render(SearchField, { nouns: "rows", count: 3, total: 12 });
    expect(screen.getByText("3 of 12")).toBeTruthy();
  });

  // Absent rather than dimmed when empty, matching the {#if searching} guard it
  // replaces — and unlike the invisible `.mini` clear button it replaces, it is
  // actually visible when it is there.
  test("the clear button is absent while the box is empty", () => {
    render(SearchField, { nouns: "rows" });
    expect(screen.queryByRole("button", { name: "Clear (Esc)" })).toBeNull();
  });

  test("clearing empties the box and reports it", async () => {
    const onclear = vi.fn();
    render(SearchField, { nouns: "rows", value: "abc", onclear });

    const input = screen.getByPlaceholderText("Filter rows…") as HTMLInputElement;
    expect(input.value).toBe("abc");

    await fireEvent.click(screen.getByRole("button", { name: "Clear (Esc)" }));

    expect(onclear).toHaveBeenCalledTimes(1);
    expect(input.value).toBe("");
  });

  test("Escape clears too", async () => {
    const onclear = vi.fn();
    render(SearchField, { nouns: "rows", value: "abc", onclear });

    const input = screen.getByPlaceholderText("Filter rows…") as HTMLInputElement;
    await fireEvent.keyDown(input, { key: "Escape" });

    expect(onclear).toHaveBeenCalledTimes(1);
    expect(input.value).toBe("");
  });
});
