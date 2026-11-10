import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { registerReporting } from "../../src/reporting.ts";

export default function (pi: ExtensionAPI) {
  registerReporting(pi);
}
