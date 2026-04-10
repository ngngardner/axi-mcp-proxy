#!/usr/bin/env node

const { execFileSync } = require("child_process");
const path = require("path");

const PLATFORMS = {
  "linux-x64": "@axi-mcp-proxy/linux-x64",
  "linux-arm64": "@axi-mcp-proxy/linux-arm64",
  "darwin-x64": "@axi-mcp-proxy/darwin-x64",
  "darwin-arm64": "@axi-mcp-proxy/darwin-arm64",
  "win32-x64": "@axi-mcp-proxy/win32-x64",
  "win32-arm64": "@axi-mcp-proxy/win32-arm64",
};

const platformKey = `${process.platform}-${process.arch}`;
const pkg = PLATFORMS[platformKey];

if (!pkg) {
  console.error(
    `axi-mcp-proxy: unsupported platform ${platformKey}. ` +
      `Supported: ${Object.keys(PLATFORMS).join(", ")}`
  );
  process.exit(1);
}

let binDir;
try {
  binDir = path.dirname(require.resolve(`${pkg}/package.json`));
} catch {
  console.error(
    `axi-mcp-proxy: platform package ${pkg} is not installed. ` +
      `If you're on a supported platform, try reinstalling.`
  );
  process.exit(1);
}

const ext = process.platform === "win32" ? ".exe" : "";
const bin = path.join(binDir, `axi-mcp-proxy${ext}`);

try {
  execFileSync(bin, process.argv.slice(2), { stdio: "inherit" });
} catch (e) {
  if (e.status !== null) process.exit(e.status);
  throw e;
}
