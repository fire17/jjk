import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, loadRepo } from "../src/store";
import { renderGraph, renderStateTable } from "../src/render";
import { run } from "../src/shell";
import { shortStateId } from "../src/utils";

describe("update branch", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-update-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("updates a branch ref to the current git state when no state is provided", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    await runCli(["return", "green"], cwd);

    const repoBeforeUpdate = loadRepo(cwd);
    const green = repoBeforeUpdate.states.find((state) => state.description === "green");
    const greenCommit = run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout;
    await runCli(["update", "jjk/manual"], cwd);

    expect(run(["git", "rev-parse", "--verify", "refs/heads/jjk/manual"], { cwd }).stdout).toBe(
      greenCommit,
    );
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/manual",
    );

    const repo = loadRepo(cwd);
    expect(repo.branchLaneMap["jjk/manual"]).toBe("jjk/manual");
    expect(repo.states).toHaveLength(repoBeforeUpdate.states.length);
    const updatedGreen = repo.states.find((state) => state.id === green?.id);
    expect(updatedGreen?.commit).toBe(greenCommit);
    expect(updatedGreen?.branch).toBe("jjk/manual");
    expect(updatedGreen?.lane).toBe("jjk/manual");
    expect(updatedGreen?.continuationBranch).toBe("jjk/manual");
    expect(updatedGreen?.metadata?.priorContexts?.at(-1)).toEqual({
      branch: "jjk/green",
      lane: "jjk/green",
      continuationBranch: "jjk/green",
      updatedAt: updatedGreen?.metadata?.priorContexts?.at(-1)?.updatedAt,
    });
    expect(repo.lanes["jjk/manual"]?.currentStateId).toBe(updatedGreen?.id);
  });

  test("updates the checked out branch to a selected saved state and keeps the worktree clean", async () => {
    const filePath = join(cwd, "snake.py");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);

    await runCli(["return", "purple"], cwd);
    Bun.write(filePath, "purple fast\n");
    await runCli(["fast_purple"], cwd);

    const repoBeforeUpdate = loadRepo(cwd);
    const purple = repoBeforeUpdate.states.find((state) => state.description === "purple");
    expect(purple).not.toBeUndefined();

    await runCli(["update", "purple", purple!.id], cwd);

    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/purple",
    );
    expect(run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout).toBe(purple!.commit);
    expect(run(["git", "status", "--short"], { cwd }).stdout).toBe("");

    const repo = loadRepo(cwd);
    expect(repo.states).toHaveLength(repoBeforeUpdate.states.length);
    const laneName = repo.branchLaneMap["jjk/purple"];
    expect(repo.lanes[laneName ?? ""]?.currentStateId).toBe(purple!.id);

    const graph = renderGraph(repo, { currentStateId: purple!.id });
    const table = renderStateTable(repo.states, { currentStateId: purple!.id, repo });
    expect(graph).toContain(`└─ *^   ${shortStateId(purple!.id)} [new] purple (jjk/purple)`);
    expect(graph).toContain(
      `${shortStateId(repo.states.find((state) => state.description === "fast_purple")?.id ?? "")} [new] fast_purple (jjk/fast_purple)`,
    );
    expect(table).toContain(shortStateId(purple!.id));
  });
});
