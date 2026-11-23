import assert from "node:assert/strict";
import test from "node:test";
import { createWatchWidget, renderWatchLine, watchStyler } from "../src/widget.ts";

const hint = "[ctrl+`]";
const summary = ({ kitty = [0, 0], ext = [0, 0], shortcut = kitty[0] + kitty[1] > 0 } = {}) => ({
  kitty: { input: kitty[0], done: kitty[1] },
  ext: { input: ext[0], done: ext[1] },
  showShortcut: shortcut,
});
const plain = (value) => renderWatchLine(null, value);
const identity = { fg: (_color, text) => text, bold: text => text };
const styled = {
  fg: (color, text) => `<${color}>${text}</${color}>`,
  bold: text => `!${text}!`,
};

test("unstyled line shows both groups and omits empty groups", () => {
  assert.equal(plain(summary({ kitty: [1, 2] })), `◉ watch : ${hint} ⏳ 1 ✓ 2`);
  assert.equal(plain(summary({ ext: [1, 0], shortcut: false })), "◉ watch : [ext] ⏳ 1");
  assert.equal(plain(summary({ kitty: [0, 0], ext: [0, 0] })), "◉ watch :");
  assert.equal(plain(summary({ kitty: [1, 0], ext: [0, 1] })), `◉ watch : ${hint} ⏳ 1 [ext] ✓ 1`);
});

test("the shortcut hint appears only when requested", () => {
  assert.equal(plain(summary({ kitty: [1, 0], shortcut: false })), "◉ watch : ⏳ 1");
  assert.equal(plain(summary({ kitty: [1, 0], shortcut: true })), `◉ watch : ${hint} ⏳ 1`);
  assert.equal(plain(summary({ ext: [1, 0] })), "◉ watch : [ext] ⏳ 1");
});

test("waiting state styles the circle, hint, and counts", () => {
  assert.equal(
    renderWatchLine(styled, summary({ kitty: [1, 0], ext: [2, 0] })),
    "<accent>!◉!</accent>" +
      "! watch :!" +
      ` <dim>${hint}</dim>` +
      " <accent>!⏳ 1!</accent>" +
      " <dim>[ext]</dim>" +
      " <accent>!⏳ 2!</accent>",
  );
});

test("Kitty done is green and ext done is orange", () => {
  assert.equal(
    renderWatchLine(styled, summary({ kitty: [0, 1] })),
    "<success>!◉!</success>! watch :!" +
      ` <dim>${hint}</dim>` +
      " <success>!✓ 1!</success>",
  );
  assert.equal(
    renderWatchLine(styled, summary({ ext: [0, 1], shortcut: false })),
    "<warning>!◉!</warning>! watch :!" +
      " <dim>[ext]</dim>" +
      " <warning>!✓ 1!</warning>",
  );
  assert.equal(
    renderWatchLine(styled, summary({ kitty: [0, 1], ext: [0, 2] })),
    "<success>!◉!</success>! watch :!" +
      ` <dim>${hint}</dim>` +
      " <success>!✓ 1!</success>" +
      " <dim>[ext]</dim>" +
      " <warning>!✓ 2!</warning>",
  );
});

test("ext-only done is orange even when Kitty input exists", () => {
  assert.equal(
    renderWatchLine(styled, summary({ kitty: [1, 0], ext: [0, 1] })),
    "<warning>!◉!</warning>! watch :!" +
      ` <dim>${hint}</dim>` +
      " <accent>!⏳ 1!</accent>" +
      " <dim>[ext]</dim>" +
      " <warning>!✓ 1!</warning>",
  );
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
  const value = summary({ kitty: [1, 2] });
  const widget = createWatchWidget(value, tools);
  assert.equal(typeof widget, "function");
  const component = widget({}, { fg: (_color, text) => text, bold: text => text });
  const line = component.render(80)[0];
  const content = renderWatchLine(null, value);
  assert.equal(line, `${content} ${"─".repeat(80 - Array.from(content).length - 1)}`);
  assert.equal(typeof component.invalidate, "function");
});

test("render pads the rule with dim styling when the theme colors it", () => {
  // The measure double strips the fake color markers, mirroring how visibleWidth ignores ANSI.
  const styledTools = {
    truncate: (text, width) => Array.from(text).slice(0, width).join(""),
    measure: text => Array.from(text.replace(/<\/?[a-z]+>/g, "")).length,
  };
  const widget = createWatchWidget(summary({ kitty: [1, 0] }), styledTools);
  const component = widget({}, { fg: (color, text) => `<${color}>${text}</${color}>`, bold: text => text });
  // The width stays above the styled line's raw length so the naive truncate double,
  // which is not ANSI-aware, never cuts a marker. Real truncation is pi-tui's job.
  const line = component.render(120)[0];
  assert.ok(line.includes("</accent> <dim>"), "content renders before the dim rule");
  assert.ok(line.endsWith("</dim>"), "the rule renders dim");
});

test("render truncates without a rule when no room remains", () => {
  // The double counts code points because the styled line is plain in this theme.
  // Real ANSI-aware truncation comes from pi-tui's truncateToWidth in the extension.
  const component = createWatchWidget(summary({ kitty: [0, 1], shortcut: false }), tools)(
    {},
    { fg: (_color, text) => text, bold: text => text },
  );
  const line = component.render(10)[0];
  assert.ok(Array.from(line).length <= 10);
  assert.ok(!line.includes("─"));
});

test("render reads the theme each frame so a theme change takes effect", () => {
  // Pi's theme object keeps its identity while its tokens change, so a theme switch
  // reaches the widget only when the line is styled during render.
  let token = "before";
  const liveTheme = { fg: (_color, text) => `${token}:${text}`, bold: text => text };
  const component = createWatchWidget(summary({ kitty: [1, 0] }), tools)({}, liveTheme);
  assert.ok(component.render(200)[0].startsWith("before:"));
  token = "after";
  assert.ok(component.render(200)[0].startsWith("after:"));
});
