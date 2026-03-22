import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, saveState } from "../src/store";
import { run } from "../src/shell";
import { shortStateId } from "../src/utils";

describe("status command", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-status-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("status reports current lane, worktree state, and latest saved state", async () => {
    Bun.write(join(cwd, "notes.txt"), "alpha\n");
    const state = saveState(cwd, {
      kind: "step",
      description: "baseline ready",
    }).state;
    Bun.write(join(cwd, "notes.txt"), "alpha\nbeta\n");

    const output: string[] = [];
    const originalLog = console.log;
    console.log = (...args: unknown[]) => {
      output.push(args.join(" "));
    };

    try {
      await runCli(["status"], cwd);
    } finally {
      console.log = originalLog;
    }

    const text = output.join("\n");
    expect(text).toContain("current lane: main");
    expect(text).toContain("worktree: dirty");
    expect(text).toContain(`latest state: ${shortStateId(state.id)} [step] baseline ready`);
  });
});
