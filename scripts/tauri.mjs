// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

const args = process.argv.slice(2);
const separatorIndex = args.indexOf("--");
const cliEnd = separatorIndex >= 0 ? separatorIndex : args.length;
const cliArgs = args.slice(0, cliEnd);

if (cliArgs.includes("build")) {
  const feature = "duckdb-bundled";
  const optionIndex = cliArgs.findIndex(
    (arg) => arg === "--features" || arg === "-f",
  );
  const equalsIndex = cliArgs.findIndex((arg) => arg.startsWith("--features="));

  if (optionIndex >= 0 && args[optionIndex + 1]) {
    const features = new Set(args[optionIndex + 1].split(",").filter(Boolean));
    features.add(feature);
    args[optionIndex + 1] = [...features].join(",");
  } else if (equalsIndex >= 0) {
    const features = new Set(
      args[equalsIndex].slice("--features=".length).split(",").filter(Boolean),
    );
    features.add(feature);
    args[equalsIndex] = `--features=${[...features].join(",")}`;
  } else {
    args.splice(cliEnd, 0, "--features", feature);
  }
}

const tauriBin = createRequire(import.meta.url).resolve(
  "@tauri-apps/cli/tauri.js",
);
const result = spawnSync(process.execPath, [tauriBin, ...args], {
  stdio: "inherit",
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

process.exit(result.status ?? 1);
