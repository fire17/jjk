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

  test("return keeps the checkout detached and does not create a return branch when clean", async () => {
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
    const stateCountBeforeReturn = loadRepo(cwd).states.length;

    await runCli(["return", state.id], cwd);

    const repo = loadRepo(cwd);
    const detached = run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], {
      cwd,
      allowFailure: true,
    });

    expect(detached.exitCode).not.toBe(0);
    expect(repo.branchLaneMap[`jjk/return-${state.id}`]).toBeUndefined();
    expect(repo.lanes["feature harvest"].branch).toBe(lane.branch);
    expect(repo.returnContext?.stateId).toBe(state.id);
    expect(repo.states).toHaveLength(stateCountBeforeReturn);
  });

  test("return auto-saves only when unstaged or untracked work would be lost", async () => {
    Bun.write(join(cwd, "notes.txt"), "alpha\n");
    const baseline = saveState(cwd, {
      kind: "star",
      description: "baseline anchor",
    }).state;

    Bun.write(join(cwd, "staged-only.txt"), "staged\n");
    run(["git", "add", "staged-only.txt"], { cwd });
    await runCli(["return", baseline.id], cwd);

    let repo = loadRepo(cwd);
    expect(repo.states[repo.states.length - 1]?.id).toBe(baseline.id);

    run(["git", "switch", "main"], { cwd });
    Bun.write(join(cwd, "notes.txt"), "alpha\nlocal edit\n");
    await runCli(["return", baseline.id], cwd);

    repo = loadRepo(cwd);
    expect(repo.states[repo.states.length - 1]?.label).toBe(
      `back to ${baseline.id} ${baseline.description}`,
    );
  });

  test("saving after return opens a jjk branch from the returned description", async () => {
    Bun.write(join(cwd, "notes.txt"), "alpha\n");
    const baseline = saveState(cwd, {
      kind: "save",
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
      kind: "save",
      description: "fast mode only",
    }).state;

    const branch = run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], {
      cwd,
    }).stdout;

    expect(branch).toBe("jjk/baseline_anchor/fast_mode_only");
    expect(fastMode.branch).toBe("jjk/baseline_anchor/fast_mode_only");
    expect(fastMode.parentStateId).toBe(baseline.id);
    expect(loadRepo(cwd).returnContext).toBeNull();
  });
});
