import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
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

describe("branch command", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-branch-command-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("lists tracked jjk branches with the current branch highlighted", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);

    const output = await captureCli(["branch"], cwd);

    expect(output).toContain("  jjk/green");
    expect(output).toContain("* jjk/purple");
    expect(output).toContain("  main");
  });

  test("creates a jjk branch at the current state without switching or creating a new state", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    const before = loadRepo(cwd);
    const currentState = before.states.at(-1)!;
    const output = await captureCli(["branch", "review lane"], cwd);
    const after = loadRepo(cwd);

    expect(output).toContain("created branch jjk/review_lane");
    expect(after.states).toHaveLength(before.states.length);
    expect(after.branchLaneMap["jjk/review_lane"]).toBe("jjk/review_lane");
    expect(after.lanes["jjk/review_lane"]?.currentStateId).toBe(currentState.id);
    expect(run(["git", "rev-parse", "--verify", "refs/heads/jjk/review_lane"], { cwd }).stdout).toBe(
      currentState.commit,
    );
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/green",
    );
  });
});
