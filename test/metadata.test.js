import assert from "node:assert/strict";
import test from "node:test";
import { paneFocused } from "../src/metadata.ts";

const listing = (...flags) => JSON.stringify([
  { is_focused: flags[0], tabs: [{ is_focused: flags[1], windows: [{ id: 123, title: "private", env: { secret: "private" }, is_focused: flags[2] }] }] },
]);

test("focus requires the OS window, tab, and pane to all be focused", () => {
  assert.equal(paneFocused(listing(true, true, true), "123"), true);
  for (const flags of [[false, true, true], [true, false, true], [true, true, false]]) {
    assert.equal(paneFocused(listing(...flags), "123"), false);
  }
  assert.equal(paneFocused(listing(true, true, undefined), "123"), null);
});

test("missing and malformed pane data degrade safely", () => {
  for (const output of ["invalid", "[]", "null", "{}", '[{"tabs":{}}]']) {
    assert.equal(paneFocused(output, "123"), null);
  }
  assert.equal(paneFocused(listing(true, true, true), "999"), null);
});
