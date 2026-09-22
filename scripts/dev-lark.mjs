import { mkdir, readFile, writeFile, access } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { spawn } from "node:child_process";

const root = fileURLToPath(new URL("../", import.meta.url));
const configDir = path.join(root, ".local/desktop/config");
const dataDir = path.join(root, ".local/desktop/data");
await mkdir(configDir, { recursive: true });
await mkdir(dataDir, { recursive: true });
async function seed(name, value) {
  const target = path.join(configDir, name);
  try {
    await access(target);
  } catch {
    await writeFile(target, JSON.stringify(value, null, 2) + "\n", { mode: 0o600, flag: "wx" });
  }
}
await seed("config.json", {
  globalShortcut: process.platform === "darwin" ? "Command+Option+N" : "Ctrl+Space",
  closeToTray: true,
  autostart: false,
  defaultViewMode: "split",
  dataDir,
  locale: "zh-CN",
});
try {
  const connection = JSON.parse(
    await readFile(path.join(root, "lark/connection.local.json"), "utf8"),
  );
  await seed("lark-sync.json", connection);
} catch (error) {
  if (error.code !== "ENOENT") throw error;
  console.info("未配置飞书连接，可在应用设置中填写 Base token 和数据表 ID。");
}
const cli = path.join(root, "node_modules/@tauri-apps/cli/tauri.js");
const child = spawn(
  process.execPath,
  [cli, "dev", "--config", "src-tauri/tauri.larknote.conf.json"],
  {
    cwd: root,
    stdio: "inherit",
    env: {
      ...process.env,
      FLORAL_NOTEPAPER_CONFIG_DIR: configDir,
      FLORAL_NOTEPAPER_DATA_DIR: dataDir,
    },
  },
);
child.on("error", (error) => {
  console.error(error.message);
  process.exitCode = 1;
});
child.on("exit", (code) => {
  process.exitCode = code ?? 1;
});
