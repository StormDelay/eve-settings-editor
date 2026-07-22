// @ts-ignore: node:test and node:assert/strict available at runtime via npm test
import { test } from "node:test";
// @ts-ignore: node:assert/strict available at runtime via npm test
import assert from "node:assert/strict";
import { stateLabel, EXCEPTION_STATES, DEFAULT_BACKGROUND_ORDER } from "./states.ts";

test("stateLabel resolves a known id", () => {
  assert.equal(stateLabel(51), "Pilot is a criminal");
});

test("stateLabel returns null for the unrendered id 68", () => {
  assert.equal(stateLabel(68), null);
});

test("stateLabel returns null for an unknown id", () => {
  assert.equal(stateLabel(9999), null);
});

test("the exception vocabulary includes the wreck states and excludes 68", () => {
  assert.ok(EXCEPTION_STATES.includes(36));
  assert.ok(EXCEPTION_STATES.includes(37));
  assert.ok(!EXCEPTION_STATES.includes(68));
});

test("the default background order carries the unrendered id 68", () => {
  assert.ok(DEFAULT_BACKGROUND_ORDER.includes(68));
});
