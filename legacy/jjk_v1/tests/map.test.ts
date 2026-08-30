import { beforeEach, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace } from "../src/store";
import { run } from "../src/shell";

async function captureCli(argv: string[], cwd: string): Promise<string> {
  const output: string[] = [];
  const originalLog = console.log;
  console.log = (...args: unknown[]) => {
    output.push(args.join(" "));
  };

  try {
    await runCli(argv, cwd);
  } finally {
    console.log = originalLog;
  }

  return output.join("\n");
}

describe("map command", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-map-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("skips gitignored paths while scanning", async () => {
    writeFileSync(join(cwd, ".gitignore"), ".worktrees/\nignored/\n");

    mkdirSync(join(cwd, "visible"), { recursive: true });
    writeFileSync(join(cwd, "visible", "package.json"), "{}\n");

    mkdirSync(join(cwd, "ignored"), { recursive: true });
    writeFileSync(join(cwd, "ignored", "package.json"), "{}\n");

    const output = await captureCli(["map"], cwd);

    expect(output).toContain("visible");
    expect(output).not.toContain("ignored");
  });
});
