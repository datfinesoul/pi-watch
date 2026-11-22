/** One-line status widget rendering for cross-session input and completion counts. */
import type { Theme } from "@earendil-works/pi-coding-agent";

export type WatchColor = "accent" | "success" | "dim";

export interface WatchStyler {
  fg(color: WatchColor, text: string): string;
  bold(text: string): string;
}

export interface WatchComponent {
  render(width: number): readonly string[];
  invalidate(): void;
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

/**
 * O1 "labeled color" line: a bold `watch` label with state colors.
 * Waiting counts render bold in the accent color, done counts bold in the success
 * color, and a fully acknowledged state renders dim without emphasis.
 */
export function renderWatchLine(style: WatchStyler | null, inputCount: number, doneCount: number): string {
  const waiting = inputCount > 0;
  const input = waiting ? `⏳ ${inputCount} input` : "";
  const done = doneCount > 0 ? `✓ ${doneCount} done` : "";
  const body = waiting ? (done ? `${input} · ${done}` : input) : done;
  const plain = `◉ watch  ${body}  [ctrl+']`;
  if (!style) return plain;
  if (!waiting) return style.fg("dim", plain);
  const segments = [
    style.bold(`${style.fg("accent", "◉")} watch`),
    "  ",
    style.fg("accent", style.bold(input)),
    done ? style.fg("dim", " · ") : "",
    done ? style.fg("success", style.bold(done)) : "",
    style.fg("dim", "  [ctrl+']"),
  ];
  return segments.join("");
}

/**
 * Widget factory for `ui.setWidget`: captures the counts and styles lazily per theme.
 * The render truncates to the terminal width with the host's `truncateToWidth` so a
 * narrow pane cannot exceed the renderer's width limit, and fills the remaining
 * width with a dim rule so the row reads as its own section.
 */
export function createWatchWidget(inputCount: number, doneCount: number, tools: WatchWidthTools): WatchWidgetFactory {
  return (_tui, theme) => {
    const style = watchStyler(theme);
    const line = renderWatchLine(style, inputCount, doneCount);
    return {
      render(width: number) {
        const content = tools.truncate(line, width);
        const rule = Math.max(0, width - tools.measure(content) - 1);
        return [rule > 0 ? `${content} ${style.fg("dim", "─".repeat(rule))}` : content];
      },
      invalidate() {},
    };
  };
}
