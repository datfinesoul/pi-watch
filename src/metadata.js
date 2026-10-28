import { basename, relative } from "node:path";

export function projectLabel(cwd, root, superRoot, hasSubmodules) {
  const base = superRoot || root;
  if (!base) return basename(cwd) || cwd;
  const suffix = relative(base, cwd);
  return `${superRoot || hasSubmodules ? "↳/" : ""}${basename(base)}${suffix ? `/${suffix}` : ""}`;
}

export function paneMetadata(output, pane) {
  try {
    for (const window of JSON.parse(output)) {
      for (const tab of window.tabs ?? []) {
        for (const candidate of tab.windows ?? []) {
          if (String(candidate.id) === pane) {
            const flags = [window.is_focused, tab.is_focused, candidate.is_focused];
            return {
              title: typeof candidate.title === "string" ? candidate.title.replace(/[\u0000-\u001f\u007f]/g, "").slice(0, 200) || null : null,
              focused: flags.every(flag => typeof flag === "boolean") ? flags.every(flag => flag) : null,
            };
          }
        }
      }
    }
  } catch { /* Missing or malformed Kitty response. */ }
  return { title: null, focused: null };
}

export function paneTitle(output, pane) {
  return paneMetadata(output, pane).title;
}
