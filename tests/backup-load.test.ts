import { beforeEach, describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, loadRepo } from "../src/store";
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

describe("backup and load commands", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-backup-load-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("load restores a saved backup and undo restores the pre-load workspace", async () => {
    const filePath = join(cwd, "notes.txt");

    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    const green = loadRepo(cwd).states.at(-1)!;

    await runCli(["backup", "before-purple"], cwd);
    expect(existsSync(join(cwd, ".jjk", "backups", "before-purple.json"))).toBe(true);

    Bun.write(filePath, "purple\n");
    await runCli(["save", "purple"], cwd);
    const purple = loadRepo(cwd).states.at(-1)!;

    await runCli(["load", "before-purple"], cwd);

    let repo = loadRepo(cwd);
    expect(repo.states.find((state) => state.id === purple.id)).toBeUndefined();
    expect(run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout).toBe(green.commit);
    expect((await captureCli(["current"], cwd))).toContain(green.label);

    await runCli(["undo"], cwd);

    repo = loadRepo(cwd);
    expect(repo.states.find((state) => state.id === purple.id)).toBeDefined();
    expect(run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout).toBe(purple.commit);
    expect((await captureCli(["current"], cwd))).toContain(purple.label);
  });

  test("backup accepts an explicit output path and reports saved file size", async () => {
    const filePath = join(cwd, "notes.txt");
    const backupPath = join(cwd, "snapshots", "custom-backup.json");

    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    const output = await captureCli(["backup", "snapshots/custom-backup.json"], cwd);

    expect(existsSync(backupPath)).toBe(true);
    expect(output).toContain("backup saved: snapshots/custom-backup.json");
    expect(output).toMatch(/\((\d+ B|\d+\.\d KB|\d+\.\d MB)\)/);
  });
});
