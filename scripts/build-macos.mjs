import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const targets = new Set(["aarch64-apple-darwin", "x86_64-apple-darwin", "universal-apple-darwin"]);

export function selectTarget(args, arch = process.arch) {
  if (args.length === 0) {
    if (arch === "arm64") return "aarch64-apple-darwin";
    if (arch === "x64") return "x86_64-apple-darwin";
    throw new Error("不支持此 CPU 架构，请明确选择 Mac target。");
  }
  if (args.length !== 2 || args[0] !== "--target" || !targets.has(args[1])) {
    throw new Error(
      "用法：npm run build:mac -- --target <aarch64-apple-darwin|x86_64-apple-darwin|universal-apple-darwin>",
    );
  }
  return args[1];
}

export function buildArgs(target) {
  if (!targets.has(target)) throw new Error("无效的 Mac target");
  return [
    path.join(root, "node_modules/@tauri-apps/cli/tauri.js"),
    "build",
    "--ci",
    "--config",
    "src-tauri/tauri.larknote.conf.json",
    "--config",
    "src-tauri/tauri.larknote.macos-preview.conf.json",
    "--target",
    target,
    "--bundles",
    "dmg",
  ];
}

export function previewEnvironment(source) {
  const env = { ...source, APPLE_SIGNING_IDENTITY: "-" };
  // This entrypoint intentionally makes ad-hoc-signed preview builds. Do not
  // accidentally use a developer's notarization credentials or CI secrets.
  for (const key of [
    "APPLE_ID",
    "APPLE_PASSWORD",
    "APPLE_TEAM_ID",
    "APPLE_API_ISSUER",
    "APPLE_API_KEY",
    "APPLE_API_KEY_PATH",
    "APPLE_CERTIFICATE",
    "APPLE_CERTIFICATE_PASSWORD",
  ]) {
    delete env[key];
  }
  return env;
}

function run(command, args, env, capture = false) {
  const result = spawnSync(command, args, {
    cwd: root,
    env,
    encoding: "utf8",
    stdio: capture ? "pipe" : "inherit",
  });
  if (result.error || result.status !== 0) {
    throw new Error(
      `${command} 执行失败${result.error ? `：${result.error.message}` : `（退出码 ${result.status}）`}`,
    );
  }
  return result.stdout?.trim() ?? "";
}

function main() {
  const target = selectTarget(process.argv.slice(2));
  if (process.platform !== "darwin")
    throw new Error("Mac 安装包必须在 macOS 构建机上生成；当前不是 macOS，未开始构建。");
  const env = previewEnvironment(process.env);
  const version = run("sw_vers", ["-productVersion"], env, true);
  if (Number(version.split(".")[0]) < 15)
    throw new Error("此预览版沿用上游要求：macOS 15 或更高版本。");
  run("xcrun", ["--find", "clang"], env, true);
  if (!existsSync(buildArgs(target)[0])) throw new Error("请先在项目目录执行 npm ci。");
  const rustTargets =
    target === "universal-apple-darwin"
      ? ["aarch64-apple-darwin", "x86_64-apple-darwin"]
      : [target];
  run("rustup", ["target", "add", ...rustTargets], env);
  run(process.execPath, buildArgs(target), env);

  const bundleDir = path.join(root, "src-tauri/target", target, "release/bundle");
  const app = path.join(bundleDir, "macos/花笺飞书便签.app");
  const executable = path.join(app, "Contents/MacOS/floral-notepaper");
  const identifier = run(
    "/usr/libexec/PlistBuddy",
    ["-c", "Print :CFBundleIdentifier", path.join(app, "Contents/Info.plist")],
    env,
    true,
  );
  if (identifier !== "com.larknote.floral.desktop")
    throw new Error("生成的应用不是 LarkNote，拒绝交付原版花笺安装包。");
  const archs = run("lipo", ["-archs", executable], env, true).split(/\s+/);
  const expected =
    target === "universal-apple-darwin"
      ? ["arm64", "x86_64"]
      : [target.startsWith("aarch64") ? "arm64" : "x86_64"];
  if (expected.some((arch) => !archs.includes(arch)))
    throw new Error("生成的应用架构与目标不一致。");
  run("codesign", ["--verify", "--deep", "--strict", app], env);
  const pkg = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
  const candidates = readdirSync(path.join(bundleDir, "dmg")).filter(
    (file) => file.startsWith(`花笺飞书便签_${pkg.version}_`) && file.endsWith(".dmg"),
  );
  if (candidates.length !== 1) throw new Error("没有找到唯一的当前版本 DMG，拒绝交付不明制品。");
  const source = path.join(bundleDir, "dmg", candidates[0]);
  run("hdiutil", ["verify", source], env);
  const outputDir = path.join(root, "local-build/macos", target);
  mkdirSync(outputDir, { recursive: true });
  const filename = `LarkNote_${pkg.version}_${target}.dmg`;
  copyFileSync(source, path.join(outputDir, filename));
  const sha256 = createHash("sha256").update(readFileSync(source)).digest("hex");
  writeFileSync(path.join(outputDir, "SHA256SUMS"), `${sha256}  ${filename}\n`);
  console.info(
    `\nMac 预览安装包：${path.join(outputDir, filename)}\n已校验应用标识、CPU 架构、本地签名及 DMG 完整性。未进行 Apple 公证。`,
  );
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
