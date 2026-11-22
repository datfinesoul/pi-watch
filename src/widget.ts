/** One-line status widget rendering for cross-session input and completion counts. */
import type { Theme } from "@earendil-works/pi-coding-agent";

export type WatchColor = "accent" | "success" | "warning" | "dim";

export interface WatchStyler {
  fg(color: WatchColor, text: string): string;
  bold(text: string): string;
}

export interface WatchComponent {
  render(width: number): readonly string[];
  invalidate(): void;
}

export interface WatchCounts {
  input: number;
  done: number;
}

/** Focus-checked Kitty peers, peers without focus checking, and the shortcut state. */
export interface WatchSummary {
  kitty: WatchCounts;
  ext: WatchCounts;
  showShortcut: boolean;
}

/** ANSI-aware width tools from the host, injected so tests stay dependency-free. */
export interface WatchWidthTools {
  truncate(text: string, width: number): string;
  measure(text: string): number;
}

export type WatchWidgetFactory = (tui: unknown, theme: Theme) => WatchComponent;

/** Adapter from the host theme to the narrow styling surface the renderer uses. */
export function watchStyler(theme: Theme): WatchStyler {
  return {
    fg: (color, text) => theme.fg(color, text),
    bold: text => theme.bold(text),
  };
}

interface Segment {
  text: string;
  color?: WatchColor;
  bold?: boolean;
}

/**
 * One-line `{circle} watch : ...` status line.
 *
 * The Kitty group holds peers with a working focus check. The ext group holds
 * peers without one. A Kitty completion turns the circle green. An ext-only
 * completion turns it orange. Green wins when both groups have completions.
 * Empty groups are omitted. The shortcut hint is omitted when it would not
 * navigate, or when the Kitty group is empty.
 */
export function renderWatchLine(style: WatchStyler | null, summary: WatchSummary): string {
  const { kitty, ext, showShortcut } = summary;
  const circle: WatchColor =
    kitty.done > 0 ? "success" :
    ext.done > 0 ? "warning" :
    kitty.input > 0 || ext.input > 0 ? "accent" :
    "dim";
  const segments: Segment[] = [];
  if (showShortcut) segments.push({ text: "[ctrl+`]", color: "dim" });
  if (kitty.input > 0) segments.push({ text: `⏳ ${kitty.input}`, color: "accent", bold: true });
  if (kitty.done > 0) segments.push({ text: `✓ ${kitty.done}`, color: "success", bold: true });
  if (ext.input > 0 || ext.done > 0) {
    segments.push({ text: "[ext]", color: "dim" });
    if (ext.input > 0) segments.push({ text: `⏳ ${ext.input}`, color: "accent", bold: true });
    if (ext.done > 0) segments.push({ text: `✓ ${ext.done}`, color: "warning", bold: true });
  }
  const render = (segment: Segment) => {
    if (!style) return segment.text;
    const text = segment.bold ? style.bold(segment.text) : segment.text;
    return segment.color ? style.fg(segment.color, text) : text;
  };
  const head = style
    ? `${style.fg(circle, style.bold("◉"))}${style.bold(" watch :")}`
    : "◉ watch :";
  const body = segments.map(render).join(" ");
  return body ? `${head} ${body}` : head;
}

/**
 * Widget factory for `ui.setWidget`: captures the summary and styles the line on
 * each render. Pi's theme object keeps its identity across theme changes, so
 * reading it per frame makes a switch take effect without a new widget. The
 * render truncates to the terminal width with the host's `truncateToWidth` so a
 * narrow pane cannot exceed the renderer's width limit, and fills the remaining
 * width with a dim rule so the row reads as its own section.
 */
export function createWatchWidget(summary: WatchSummary, tools: WatchWidthTools): WatchWidgetFactory {
  return (_tui, theme) => {
    const style = watchStyler(theme);
    return {
      render(width: number) {
        const content = tools.truncate(renderWatchLine(style, summary), width);
        const rule = Math.max(0, width - tools.measure(content) - 1);
        return [rule > 0 ? `${content} ${style.fg("dim", "─".repeat(rule))}` : content];
      },
      // Rendering holds no cache, so a theme change needs no extra work here.
      invalidate() {},
    };
  };
}
