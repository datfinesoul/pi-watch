import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { truncateToWidth, visibleWidth } from "@earendil-works/pi-tui";
import { registerReporting } from "../../src/reporting.ts";

export default function (pi: ExtensionAPI) {
  registerReporting(pi, { truncate: truncateToWidth, measure: visibleWidth });
}
