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
  const plain = `◉ watch  ${body}   ctrl+' visit`;
  if (!style) return plain;
  if (!waiting) return style.fg("dim", plain);
  const segments = [
    style.bold(`${style.fg("accent", "◉")} watch`),
    "  ",
    style.fg("accent", style.bold(input)),
    done ? style.fg("dim", " · ") : "",
    done ? style.fg("success", style.bold(done)) : "",
    style.fg("dim", "   ctrl+' visit"),
  ];
  return segments.join("");
}

/** Widget factory for `ui.setWidget`: captures the counts and styles lazily per theme. */
export function createWatchWidget(inputCount: number, doneCount: number): WatchWidgetFactory {
  return (_tui, theme) => {
    const line = renderWatchLine(watchStyler(theme), inputCount, doneCount);
    return {
      render: () => [line],
      invalidate() {},
    };
  };
}
