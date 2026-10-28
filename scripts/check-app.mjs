import { spawnSync } from "node:child_process";
import { checkVersions, root } from "./check-versions.mjs";
import { resolve } from "node:path";

if (process.platform !== "darwin") {
  console.error("The Pi Watch app requires macOS. Native validation was not run.");
  process.exit(1);
}

try {
  checkVersions();
  const env = { ...process.env, CARGO_TARGET_DIR: resolve(root, "target") };
  const manifest = "apps/macos/Cargo.toml";
  const checks = [
    ["cargo", ["fmt", "--manifest-path", manifest, "--check"]],
    ["cargo", ["clippy", "--manifest-path", manifest, "--locked", "--all-targets", "--all-features", "--", "-D", "warnings"]],
    ["cargo", ["test", "--manifest-path", manifest, "--locked", "--all-targets", "--all-features"]],
    ["cargo", ["test", "--manifest-path", manifest, "--locked", "--all-targets"]],
    ["sh", ["scripts/bundle.sh"]],
  ];
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
