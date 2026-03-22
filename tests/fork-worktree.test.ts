import { beforeEach, describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
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

function extractWorktreePath(output: string, cwd: string): string {
  const line = output
    .split("\n")
    .find((entry) => entry.startsWith("worktree ready: "));
  if (!line) {
    throw new Error(`No worktree path found in output:\n${output}`);
  }
  return resolve(cwd, line.replace("worktree ready: ", "").trim());
}

describe("fork and worktree commands", () => {
  let sandbox = "";
  let cwd = "";

  beforeEach(() => {
    sandbox = mkdtempSync(join(tmpdir(), "jjk-worktree-"));
    cwd = join(sandbox, "repo");
    mkdirSync(cwd);
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("checkout switches to an existing jjk branch", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);

    await runCli(["checkout", "jjk/green"], cwd);

    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/green",
    );
  });

  test("fork --worktree creates a sibling worktree from the current state", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    const output = await captureCli(["fork", "--worktree"], cwd);
    const worktreePath = extractWorktreePath(output, cwd);

    expect(output).toContain("branch: jjk/green_fork");
    expect(existsSync(worktreePath)).toBe(true);
    expect(existsSync(join(worktreePath, ".jjk", "repo.json"))).toBe(true);
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd: worktreePath }).stdout).toBe(
      "jjk/green_fork",
    );
  });

  test("fork <state> --worktree creates a worktree from the selected state commit", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    const green = loadRepo(cwd).states.find((state) => state.label === "green");

    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);

    const output = await captureCli(["fork", "green", "--worktree"], cwd);
    const worktreePath = extractWorktreePath(output, cwd);

    expect(output).toContain("branch: jjk/green_fork");
    expect(run(["git", "rev-parse", "HEAD"], { cwd: worktreePath }).stdout).toBe(green?.commit);
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd: worktreePath }).stdout).toBe(
      "jjk/green_fork",
    );
  });

  test("worktree creates a branch-local worktree from a selected state", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    const output = await captureCli(["worktree", "green"], cwd);
    const worktreePath = extractWorktreePath(output, cwd);

    expect(output).toContain("branch: jjk/green_worktree");
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd: worktreePath }).stdout).toBe(
      "jjk/green_worktree",
    );
  });
});
