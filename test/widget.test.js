import assert from "node:assert/strict";
import test from "node:test";
import { createWatchWidget, renderWatchLine, watchStyler } from "../src/widget.ts";

const plain = (inputCount, doneCount) => renderWatchLine(null, inputCount, doneCount);
const identity = { fg: (_color, text) => text, bold: text => text };
const styled = {
  fg: (color, text) => `<${color}>${text}</${color}>`,
  bold: text => `!${text}!`,
};

test("unstyled line keeps one shape for both states", () => {
  assert.equal(plain(1, 2), "◉ watch  ⏳ 1 input · ✓ 2 done   ctrl+' visit");
  assert.equal(plain(2, 0), "◉ watch  ⏳ 2 input   ctrl+' visit");
  assert.equal(plain(0, 1), "◉ watch  ✓ 1 done   ctrl+' visit");
});

test("waiting state styles label, counts, and hint separately", () => {
  assert.equal(
    renderWatchLine(styled, 2, 1),
    "!<accent>◉</accent> watch!" +
      "  " +
      "<accent>!⏳ 2 input!</accent>" +
      "<dim> · </dim>" +
      "<success>!✓ 1 done!</success>" +
      "<dim>   ctrl+' visit</dim>",
  );
  assert.equal(
    renderWatchLine(styled, 1, 0),
    "!<accent>◉</accent> watch!" +
      "  " +
      "<accent>!⏳ 1 input!</accent>" +
      "<dim>   ctrl+' visit</dim>",
  );
});

test("acknowledged state renders the whole line dim without emphasis", () => {
  assert.equal(renderWatchLine(styled, 0, 2), "<dim>◉ watch  ✓ 2 done   ctrl+' visit</dim>");
});

test("theme adapter passes fg and bold through", () => {
  const theme = { fg: (color, text) => `fg:${color}:${text}`, bold: text => `bold:${text}` };
  assert.equal(watchStyler(theme).fg("accent", "x"), "fg:accent:x");
  assert.equal(watchStyler(theme).bold("x"), "bold:x");
});

test("widget factory renders exactly one themed line", () => {
  const widget = createWatchWidget(1, 2);
  assert.equal(typeof widget, "function");
  const component = widget({}, { fg: (_color, text) => text, bold: text => text });
  const lines = component.render(80);
  assert.deepEqual(lines, ["◉ watch  ⏳ 1 input · ✓ 2 done   ctrl+' visit"]);
  assert.equal(typeof component.invalidate, "function");
});
