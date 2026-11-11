/** Retain only the reporting pane's focus flags, never its title or environment. */
export function paneFocused(output, pane) {
  try {
    for (const window of JSON.parse(output)) {
      for (const tab of window.tabs ?? []) {
        for (const candidate of tab.windows ?? []) {
          if (String(candidate.id) !== pane) continue;
          const flags = [window.is_focused, tab.is_focused, candidate.is_focused];
          return flags.every(flag => typeof flag === "boolean") ? flags.every(Boolean) : null;
        }
      }
    }
  } catch { /* Missing or malformed Kitty response. */ }
  return null;
}
