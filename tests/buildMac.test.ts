import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { buildArgs, previewEnvironment, selectTarget } from "../scripts/build-macos.mjs";

describe("Mac preview packaging", () => {
  test("selects Apple Silicon or Intel and supports an explicit universal build", () => {
    expect(selectTarget([], "arm64")).toBe("aarch64-apple-darwin");
    expect(selectTarget([], "x64")).toBe("x86_64-apple-darwin");
    expect(selectTarget(["--target", "universal-apple-darwin"])).toBe("universal-apple-darwin");
    expect(() => selectTarget(["--target", "x86_64-unknown-linux-gnu"])).toThrow();
    expect(() => selectTarget(["--no-sign"])).toThrow();
  });

  test("merges LarkNote identity after the platform config and requests a DMG", () => {
    const args = buildArgs("aarch64-apple-darwin");
    expect(args).toEqual(expect.arrayContaining(["--ci", "--bundles", "dmg"]));
    expect(args.filter((arg) => arg.endsWith(".conf.json"))).toEqual([
      "src-tauri/tauri.larknote.conf.json",
      "src-tauri/tauri.larknote.macos-preview.conf.json",
    ]);
    const config = JSON.parse(readFileSync("src-tauri/tauri.larknote.conf.json", "utf8"));
    expect(config.identifier).toBe("com.larknote.floral.desktop");
    expect(config.build.beforeBuildCommand).toBe("npm run build");
    expect(config.bundle?.resources).toBeUndefined();
  });

  test("does not use or mutate Apple release credentials for preview builds", () => {
    const original = {
      PATH: "/test",
      APPLE_ID: "example",
      APPLE_PASSWORD: "example",
      APPLE_CERTIFICATE: "example",
    };
    const env = previewEnvironment(original);
    expect(env.PATH).toBe("/test");
    expect(env.APPLE_SIGNING_IDENTITY).toBe("-");
    expect(env.APPLE_ID).toBeUndefined();
    expect(env.APPLE_PASSWORD).toBeUndefined();
    expect(env.APPLE_CERTIFICATE).toBeUndefined();
    expect(original.APPLE_ID).toBe("example");
  });

  test.skipIf(process.platform === "darwin")(
    "fails clearly on Linux instead of claiming a Mac installer exists",
    () => {
      const result = spawnSync(process.execPath, ["scripts/build-macos.mjs"], { encoding: "utf8" });
      expect(result.status).toBe(1);
      expect(result.stderr).toContain("未开始构建");
    },
  );
});
