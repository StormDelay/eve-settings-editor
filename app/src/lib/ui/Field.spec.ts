// Component test: vitest + jsdom.
import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import Field from "./Field.svelte";
import "$lib/test/setup";

describe("Field", () => {
  // Two views style <optgroup> today, so grouping is a required capability
  // rather than a speculative one.
  test("grouped select options render as optgroups, in source order", () => {
    render(Field, {
      kind: "select",
      label: "Target",
      value: "b",
      options: [
        { value: "a", label: "Alpha", group: "Live" },
        { value: "b", label: "Bravo", group: "Live" },
        { value: "c", label: "Charlie", group: "Archived" },
      ],
    });

    const groups = [...document.querySelectorAll("optgroup")];
    expect(groups.map((g) => g.getAttribute("label"))).toEqual(["Live", "Archived"]);
    expect(groups[0].querySelectorAll("option").length).toBe(2);
  });

  test("ungrouped options render bare, with no optgroup wrapper", () => {
    render(Field, {
      kind: "select",
      ariaLabel: "Target",
      value: "a",
      options: [{ value: "a", label: "Alpha" }],
    });

    expect(document.querySelector("optgroup")).toBeNull();
  });

  // The generated id is what makes <label for> pair without every one of ~40
  // call sites inventing one.
  test("a label pairs with the control through a generated id", () => {
    render(Field, { label: "Name", value: "Astra" });

    const input = screen.getByLabelText("Name") as HTMLInputElement;
    expect(input.value).toBe("Astra");
    expect(input.id).toBeTruthy();
  });

  test("an explicit id is used as given", () => {
    render(Field, { id: "chosen", label: "Name", value: "" });
    expect((screen.getByLabelText("Name") as HTMLInputElement).id).toBe("chosen");
  });

  test("a number field carries its min, max and step", () => {
    render(Field, { kind: "number", ariaLabel: "Width", value: 5, min: 0, max: 10, step: 1 });

    const input = screen.getByLabelText("Width") as HTMLInputElement;
    expect([input.min, input.max, input.step]).toEqual(["0", "10", "1"]);
  });

  test("a checkbox reflects its value and reports a change", async () => {
    const onchange = vi.fn();
    render(Field, { kind: "checkbox", label: "Bold", value: true, onchange });

    const box = screen.getByLabelText("Bold") as HTMLInputElement;
    expect(box.checked).toBe(true);

    await fireEvent.click(box);
    expect(onchange).toHaveBeenCalledTimes(1);
  });

  // An error nobody can associate with its field is an error nobody can fix.
  test("an error renders the message and wires it to the control", () => {
    render(Field, { label: "Width", value: "abc", error: "Must be a number" });

    const input = screen.getByLabelText("Width");
    expect(input.getAttribute("aria-invalid")).toBe("true");

    const described = input.getAttribute("aria-describedby");
    expect(described).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("Must be a number");
    expect(document.getElementById(described!)).toBeTruthy();
  });

  test("no error means no aria-invalid and no description", () => {
    render(Field, { label: "Width", value: "12" });

    const input = screen.getByLabelText("Width");
    expect(input.hasAttribute("aria-invalid")).toBe(false);
    expect(input.hasAttribute("aria-describedby")).toBe(false);
  });

  test("a disabled field says why", () => {
    render(Field, { label: "Width", value: "", disabled: true, disabledReason: "No file open" });

    const input = screen.getByLabelText("Width") as HTMLInputElement;
    expect(input.disabled).toBe(true);
    expect(input.getAttribute("title")).toBe("No file open");
  });
});
