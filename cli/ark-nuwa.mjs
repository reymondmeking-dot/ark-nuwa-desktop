#!/usr/bin/env node
// ark-nuwa CLI — 跨平台 Tauri + Vite 项目脚手架
// Node >= 18, ESM, 支持 macOS / Windows / Linux
// Author: ReyMao

import { spawnSync } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const ROOT = resolve(__dirname, "..");
const IS_WIN = process.platform === "win32";
const PLATFORM_HINT =
  IS_WIN
    ? "Windows → 默认产物: .msi / .exe (NSIS)"
    : process.platform === "darwin"
      ? "macOS → 默认产物: .dmg (universal 需 rustup targets)"
      : "Linux → 默认产物: .deb / .AppImage";

const HELP = `ark-nuwa — Tauri 2 + Vite + Rust 桌面应用 CLI (作者: ReyMao)

用法:
  ark-nuwa <command> [options]

命令:
  dev              启动 Tauri 开发模式 (pnpm tauri dev)
  build            平台化生产构建 (macOS: .dmg / Windows: .msi + .exe / Linux: .deb+.AppImage)
                   可选参数：--target <triple> 覆盖默认 target
  build:frontend   仅构建前端 (vite build)
  test             运行 Rust 后端测试 (cargo test)
  lint             运行 clippy 严格模式 (-D warnings)
  version          打印 package.json 中的版本号
  --help, -h       打印本帮助

平台检测:
  当前 process.platform = ${process.platform}
  ${PLATFORM_HINT}

示例:
  pnpm ark-nuwa dev
  pnpm ark-nuwa build
  pnpm ark-nuwa build --target aarch64-apple-darwin
  pnpm ark-nuwa test
  pnpm ark-nuwa lint
`;

function run(cmd, args, opts = {}) {
  // Windows: 合并成单字符串走 shell，避免 Node 24 的 DEP0190（args + shell:true 组合的转义弃用告警）
  // POSIX: 直接 spawn，二进制路径无需 shell
  const [file, spawnArgs, useShell] = IS_WIN
    ? [[cmd, ...args].map((a) => (/[\s"]/.test(a) ? `"${a.replace(/"/g, '\\"')}"` : a)).join(" "), [], true]
    : [cmd, args, false];
  const res = spawnSync(file, spawnArgs, {
    stdio: "inherit",
    shell: useShell,
    cwd: opts.cwd || ROOT,
    env: process.env,
  });
  if (res.error) {
    console.error(`[ark-nuwa] 无法启动 "${cmd}": ${res.error.message}`);
    process.exit(1);
  }
  if (res.status !== 0) process.exit(res.status ?? 1);
}

function readPkgVersion() {
  try {
    const pkg = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8"));
    return pkg.version || "unknown";
  } catch {
    return "unknown";
  }
}

function parseFlags(argv) {
  const flags = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith("--")) {
      const next = argv[i + 1];
      if (next && !next.startsWith("--")) {
        flags[a.slice(2)] = next;
        i++;
      } else {
        flags[a.slice(2)] = true;
      }
    }
  }
  return flags;
}

function cmdDev() {
  run("pnpm", ["tauri", "dev"]);
}

function cmdBuild(rest) {
  const { target } = parseFlags(rest);
  const args = ["tauri", "build", ...(target ? ["--target", String(target)] : [])];
  console.log(`[ark-nuwa] 平台=${process.platform}  执行: pnpm ${args.join(" ")}`);
  run("pnpm", args);
}

function cmdBuildFrontend() {
  run("pnpm", ["build:frontend"]);
}

function cmdTest() {
  const manifest = join(ROOT, "src-tauri", "Cargo.toml");
  if (!existsSync(manifest)) {
    console.error(`[ark-nuwa] 未找到 ${manifest}`);
    process.exit(1);
  }
  run("cargo", ["test", "--manifest-path", manifest]);
}

function cmdLint() {
  const manifest = join(ROOT, "src-tauri", "Cargo.toml");
  if (!existsSync(manifest)) {
    console.error(`[ark-nuwa] 未找到 ${manifest}`);
    process.exit(1);
  }
  run("cargo", [
    "clippy",
    "--manifest-path",
    manifest,
    "--all-targets",
    "--",
    "-D",
    "warnings",
  ]);
}

function cmdVersion() {
  console.log(`ark-nuwa v${readPkgVersion()} (node ${process.version}, ${process.platform}-${process.arch})`);
}

function main() {
  const [, , sub, ...rest] = process.argv;
  if (!sub || sub === "--help" || sub === "-h" || sub === "help") {
    process.stdout.write(HELP);
    return;
  }
  switch (sub) {
    case "dev":
      return cmdDev();
    case "build":
      return cmdBuild(rest);
    case "build:frontend":
      return cmdBuildFrontend();
    case "test":
      return cmdTest();
    case "lint":
      return cmdLint();
    case "version":
    case "--version":
    case "-v":
      return cmdVersion();
    default:
      console.error(`[ark-nuwa] 未知命令: ${sub}\n`);
      process.stdout.write(HELP);
      process.exit(2);
  }
}

main();
