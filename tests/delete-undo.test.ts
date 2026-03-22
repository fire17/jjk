import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { listStates, loadRepo, initSafeSpace, saveState } from "../src/store";
import { run } from "../src/shell";
import { stateDisplayBranch } from "../src/utils";

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

describe("delete and undo commands", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-delete-undo-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("delete hides a state from normal see and recover restores it", async () => {
    const filePath = join(cwd, "notes.txt");

    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    const green = loadRepo(cwd).states.at(-1)!;

    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);
    const purple = loadRepo(cwd).states.at(-1)!;

    await runCli(["delete", purple.id], cwd);

    const visibleLabels = listStates(cwd).map((state) => state.label);
    const allStates = listStates(cwd, { includeDeleted: true });
    const deletedPurple = allStates.find((state) => state.id === purple.id);
    const currentText = await captureCli(["current"], cwd);
    const hiddenSee = await captureCli(["see"], cwd);
    const deletedSee = await captureCli(["see", "--deleted"], cwd);

    expect(visibleLabels).not.toContain("purple");
    expect(deletedPurple?.metadata?.deletedBranch).toBe("deleted/purple");
    expect(deletedPurple?.metadata?.deletedLocation?.branch).toBe("jjk/purple");
    expect(deletedPurple?.metadata?.deletedLocation?.continuationBranch).toBe("jjk/purple");
    expect(currentText).toContain(green.label);
    expect(hiddenSee).not.toContain("purple");
    expect(deletedSee).toContain("purple");
    expect(deletedSee).toContain("deleted/purple");

    await runCli(["recover", purple.id], cwd);

    const recoveredPurple = loadRepo(cwd).states.find((state) => state.id === purple.id);
    expect(listStates(cwd).map((state) => state.label)).toContain("purple");
    expect(stateDisplayBranch(recoveredPurple!)).toBe("jjk/purple");
    expect(recoveredPurple?.parentStateId).toBe(green.id);
    expect(recoveredPurple?.metadata?.deletedAt).toBeUndefined();
  });

  test("undo removes an empty current state without confirmation", async () => {
    const filePath = join(cwd, "notes.txt");

    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    const green = loadRepo(cwd).states.at(-1)!;

    await runCli(["save", "empty_checkpoint"], cwd);
    const empty = loadRepo(cwd).states.at(-1)!;

    expect(empty.stats.changedFiles).toBe(0);

    await runCli(["undo"], cwd);

    const repo = loadRepo(cwd);
    expect(repo.states.find((state) => state.id === empty.id)).toBeUndefined();
    expect((await captureCli(["current"], cwd))).toContain(green.label);
  });

  test("undo without -rm rewinds to the previous state without erasing the saved state", async () => {
    const filePath = join(cwd, "notes.txt");

    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    const green = loadRepo(cwd).states.at(-1)!;

    Bun.write(filePath, "purple\n");
    await runCli(["save", "purple"], cwd);
    const purple = loadRepo(cwd).states.at(-1)!;

    await runCli(["undo"], cwd);

    const repo = loadRepo(cwd);
    expect(repo.states.find((state) => state.id === purple.id)).toBeDefined();
    const lane = repo.lanes[repo.branchLaneMap["jjk/green"] ?? ""];
    expect(lane?.currentStateId).toBe(green.id);
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe("jjk/green");
    expect(run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout).toBe(green.commit);
    expect((await captureCli(["current"], cwd))).toContain(green.label);
  });

  test("undo -rm erases a non-empty current state and -y skips confirmation", async () => {
    const filePath = join(cwd, "notes.txt");

    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    const green = loadRepo(cwd).states.at(-1)!;

    Bun.write(filePath, "purple\n");
    await runCli(["save", "purple"], cwd);
    const purple = loadRepo(cwd).states.at(-1)!;

    await expect(runCli(["undo", "-rm"], cwd)).rejects.toThrow("Confirmation required");

    await runCli(["undo", "-rm", "-y"], cwd);

    const repo = loadRepo(cwd);
    expect(repo.states.find((state) => state.id === purple.id)).toBeUndefined();
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe("jjk/green");
    expect(run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout).toBe(green.commit);
    expect((await captureCli(["current"], cwd))).toContain(green.label);
  });
});
