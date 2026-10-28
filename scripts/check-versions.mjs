import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const root = fileURLToPath(new URL("../", import.meta.url));

// Read only the simple version fields maintained by this repository, not arbitrary TOML.
export function productVersions(directory = root) {
  const read = path => readFileSync(resolve(directory, path), "utf8");
  const packageJson = JSON.parse(read("package.json"));
  const cargo = read("apps/macos/Cargo.toml").split(/^\[/m).find(section => section.startsWith("package]"));
  const lock = read("apps/macos/Cargo.lock").split("[[package]]").find(section => /^name = "pi-watch"$/m.test(section));
  const plist = read("apps/macos/resources/Info.plist");
  const bundleVersion = key => plist.match(new RegExp(`<key>${key}</key>\\s*<string>([^<]+)</string>`))?.[1];
  return {
    package: packageJson.version,
    cargo: cargo?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1],
    lock: lock?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1],
    bundle: bundleVersion("CFBundleShortVersionString"),
    build: bundleVersion("CFBundleVersion"),
  };
}

export function checkVersions(directory = root) {
  const versions = productVersions(directory);
  if (!/^\d+\.\d+\.\d+$/.test(versions.package)) {
    throw new Error("The native bundle requires a three-component numeric product version.");
  }
  for (const [component, version] of Object.entries(versions)) {
    if (version !== versions.package) {
      throw new Error(`${component} version '${version ?? "missing"}' differs from package.json '${versions.package}'.`);
    }
  }
  return versions.package;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    console.log(`Product versions agree: ${checkVersions()}`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
