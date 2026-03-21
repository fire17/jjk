import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { createLane, initSafeSpace, loadRepo, saveState } from "../src/store";
import { run } from "../src/shell";

describe("return flow", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-return-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("return maps the new branch to the lane without rewriting the lane owner branch", async () => {
    Bun.write(join(cwd, "notes.txt"), "alpha\n");
    const state = saveState(cwd, {
      kind: "star",
      description: "anchor before lane split",
    }).state;

    const lane = createLane(cwd, "feature harvest");
    Bun.write(join(cwd, "notes.txt"), "alpha\nbeta\n");
    saveState(cwd, {
      kind: "step",
      description: "lane progress",
    });

    await runCli(["return", state.id], cwd);

    const repo = loadRepo(cwd);
    expect(repo.branchLaneMap[`jjk/return-${state.id}`]).toBe("main");
    expect(repo.lanes["feature harvest"].branch).toBe(lane.branch);
    expect(repo.states[repo.states.length - 1]?.label).toBe(
      `back to ${state.id} ${state.description}`,
    );
  });

  test("saving after return uses the returned state as the logical parent", async () => {
    Bun.write(join(cwd, "notes.txt"), "alpha\n");
    const baseline = saveState(cwd, {
      kind: "star",
      description: "baseline anchor",
    }).state;

    Bun.write(join(cwd, "notes.txt"), "purple\n");
    saveState(cwd, {
      kind: "step",
      description: "purple theme",
    });

    await runCli(["return", baseline.id], cwd);

    Bun.write(join(cwd, "notes.txt"), "alpha\nfast_mode=true\n");
    const fastMode = saveState(cwd, {
      kind: "step",
      description: "fast mode only",
    }).state;

    expect(fastMode.parentStateId).toBe(baseline.id);
  });
});
