import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const root = fileURLToPath(new URL("../", import.meta.url));

// Read only the simple version fields maintained by this repository, not arbitrary TOML.
export function productVersions(directory = root) {
  const read = path => readFileSync(resolve(directory, path), "utf8");
  const packageJson = JSON.parse(read("package.json"));
  const cargoManifest = path =>
    read(path).split(/^\[/m).find(section => section.startsWith("package]"))
      ?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  const cargoLock = path =>
    read(path).split("[[package]]").find(section => /^name = "pi-watch"$/m.test(section))
      ?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  const plist = read("apps/macos/resources/Info.plist");
  const bundleVersion = key => plist.match(new RegExp(`<key>${key}</key>\\s*<string>([^<]+)</string>`))?.[1];
  return {
    package: packageJson.version,
    cargo: cargoManifest("apps/macos/Cargo.toml"),
    lock: cargoLock("apps/macos/Cargo.lock"),
    bundle: bundleVersion("CFBundleShortVersionString"),
    build: bundleVersion("CFBundleVersion"),
    linuxCargo: cargoManifest("apps/linux/Cargo.toml"),
    linuxLock: cargoLock("apps/linux/Cargo.lock"),
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
