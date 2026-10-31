import { spawnSync } from "node:child_process";
import { checkVersions, root } from "./check-versions.mjs";
import { resolve } from "node:path";

const platform = process.platform;
if (platform !== "darwin" && platform !== "linux") {
  console.error("The Pi Watch app requires macOS or Linux. Native validation was not run.");
  process.exit(1);
}

const manifest = platform === "darwin" ? "apps/macos/Cargo.toml" : "apps/linux/Cargo.toml";
// macOS validates the optional popovers feature and creates a signed bundle.
// Linux has no optional features and ships a plain cargo release binary.
const checks = platform === "darwin" ? [
  ["cargo", ["fmt", "--manifest-path", manifest, "--check"]],
  ["cargo", ["clippy", "--manifest-path", manifest, "--locked", "--all-targets", "--all-features", "--", "-D", "warnings"]],
  ["cargo", ["test", "--manifest-path", manifest, "--locked", "--all-targets", "--all-features"]],
  ["cargo", ["test", "--manifest-path", manifest, "--locked", "--all-targets"]],
  ["sh", ["scripts/bundle.sh"]],
] : [
  ["cargo", ["fmt", "--manifest-path", manifest, "--check"]],
  ["cargo", ["clippy", "--manifest-path", manifest, "--locked", "--all-targets", "--all-features", "--", "-D", "warnings"]],
  ["cargo", ["test", "--manifest-path", manifest, "--locked", "--all-targets", "--all-features"]],
  ["cargo", ["test", "--manifest-path", manifest, "--locked", "--all-targets"]],
  ["cargo", ["build", "--manifest-path", manifest, "--locked", "--release"]],
];

try {
  checkVersions();
  const env = { ...process.env, CARGO_TARGET_DIR: resolve(root, "target") };
  for (const [command, args] of checks) {
    console.log(`\n> ${command} ${args.join(" ")}`);
    const result = spawnSync(command, args, { cwd: root, env, stdio: "inherit" });
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error(`${command} failed (${result.signal ?? result.status}).`);
  }
} catch (error) {
  console.error(`Native validation failed: ${error.message}`);
  process.exitCode = 1;
}
