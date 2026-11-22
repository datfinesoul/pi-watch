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
  assert.equal(plain(1, 2), "◉ watch  ⏳ 1 input · ✓ 2 done  [ctrl+']");
  assert.equal(plain(2, 0), "◉ watch  ⏳ 2 input  [ctrl+']");
  assert.equal(plain(0, 1), "◉ watch  ✓ 1 done  [ctrl+']");
});

test("waiting state styles label, counts, and hint separately", () => {
  assert.equal(
    renderWatchLine(styled, 2, 1),
    "!<accent>◉</accent> watch!" +
      "  " +
      "<accent>!⏳ 2 input!</accent>" +
      "<dim> · </dim>" +
      "<success>!✓ 1 done!</success>" +
      "<dim>  [ctrl+']</dim>",
  );
  assert.equal(
    renderWatchLine(styled, 1, 0),
    "!<accent>◉</accent> watch!" +
      "  " +
      "<accent>!⏳ 1 input!</accent>" +
      "<dim>  [ctrl+']</dim>",
  );
});

test("acknowledged state renders the whole line dim without emphasis", () => {
  assert.equal(renderWatchLine(styled, 0, 2), "<dim>◉ watch  ✓ 2 done  [ctrl+']</dim>");
});

test("theme adapter passes fg and bold through", () => {
  const theme = { fg: (color, text) => `fg:${color}:${text}`, bold: text => `bold:${text}` };
  assert.equal(watchStyler(theme).fg("accent", "x"), "fg:accent:x");
  assert.equal(watchStyler(theme).bold("x"), "bold:x");
});

const tools = {
  truncate: (text, width) => Array.from(text).slice(0, width).join(""),
  measure: text => Array.from(text).length,
};

test("widget factory renders one themed line with a dim rule", () => {
  const widget = createWatchWidget(1, 2, tools);
  assert.equal(typeof widget, "function");
  const component = widget({}, { fg: (_color, text) => text, bold: text => text });
  const line = component.render(80)[0];
  const content = "◉ watch  ⏳ 1 input · ✓ 2 done  [ctrl+']";
  assert.equal(line, `${content} ${"─".repeat(80 - Array.from(content).length - 1)}`);
  assert.equal(typeof component.invalidate, "function");
});

test("render pads the rule with dim styling when the theme colors it", () => {
  // The measure double strips the fake color markers, mirroring how visibleWidth ignores ANSI.
  const styledTools = {
    truncate: (text, width) => Array.from(text).slice(0, width).join(""),
    measure: text => Array.from(text.replace(/<\/?[a-z]+>/g, "")).length,
  };
  const widget = createWatchWidget(1, 2, styledTools);
  const component = widget({}, { fg: (color, text) => `<${color}>${text}</${color}>`, bold: text => text });
  // The width stays above the styled line's raw length so the naive truncate double,
  // which is not ANSI-aware, never cuts a marker. Real truncation is pi-tui's job.
  const line = component.render(120)[0];
  assert.ok(line.includes("[ctrl+']</dim> <dim>"), "content renders before the dim rule");
  assert.ok(line.endsWith("</dim>"), "the rule renders dim");
});

test("render truncates without a rule when no room remains", () => {
  // The double counts code points because the styled line is plain in this theme.
  // Real ANSI-aware truncation comes from pi-tui's truncateToWidth in the extension.
  const component = createWatchWidget(0, 1, tools)({}, { fg: (_color, text) => text, bold: text => text });
  const line = component.render(10)[0];
  assert.equal(line, "◉ watch  ✓");
});
