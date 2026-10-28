import assert from "node:assert/strict";
import test from "node:test";
import { projectLabel, paneMetadata, paneTitle } from "../src/metadata.js";

test("project labels distinguish superprojects, submodules, standalone repositories, and non-Git directories", () => {
  assert.equal(projectLabel("/work/super/repo/src", "/work/super/repo", "/work/super", false), "↳/super/repo/src");
  assert.equal(projectLabel("/work/super", "/work/super", "", true), "↳/super");
  assert.equal(projectLabel("/work/repo/src", "/work/repo", "", false), "repo/src");
  assert.equal(projectLabel("/work/plain", "", "", false), "plain");
});

const listing = (osFocused, tabFocused, paneFocused, title = "Pane") => JSON.stringify([
  { is_focused: osFocused, tabs: [{ is_focused: tabFocused, windows: [{ id: 123, title, is_focused: paneFocused }] }] },
]);

test("focus requires the OS window, tab, and pane to all be focused", () => {
  assert.equal(paneMetadata(listing(true, true, true), "123").focused, true);
  for (const flags of [[false, true, true], [true, false, true], [true, true, false]]) {
    assert.equal(paneMetadata(listing(...flags), "123").focused, false);
  }
  assert.equal(paneMetadata(listing(true, true, undefined), "123").focused, null);
});

test("missing and malformed pane data degrade safely", () => {
  for (const output of ["invalid", "[]", "null", "{}", '[{"tabs":{}}]']) {
    assert.deepEqual(paneMetadata(output, "123"), { title: null, focused: null });
  }
  assert.deepEqual(paneMetadata(listing(true, true, true), "999"), { title: null, focused: null });
});

test("pane titles are bounded and stripped of control characters", () => {
  assert.equal(paneTitle(listing(true, true, true, "My\u0000 pane\n"), "123"), "My pane");
  assert.equal(paneTitle(listing(true, true, true, "x".repeat(300)), "123").length, 200);
  assert.equal(paneTitle(listing(true, true, true, "\n"), "123"), null);
});
