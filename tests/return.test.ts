import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { createLane, initSafeSpace, loadRepo, saveState } from "../src/store";
import { findStateMatches } from "../src/utils";
import { run } from "../src/shell";

describe("return flow", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-return-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("return to a tip state resumes its stable continuation branch", async () => {
    Bun.write(join(cwd, "notes.txt"), "alpha\n");
    const state = saveState(cwd, {
      kind: "star",
      description: "anchor before lane split",
    }).state;
    const stateCountBeforeReturn = loadRepo(cwd).states.length;

    await runCli(["return", state.id], cwd);

    const repo = loadRepo(cwd);
    const currentBranch = run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], {
      cwd,
    }).stdout;

    expect(currentBranch).toBe("jjk/anchor_before_lane_split");
    expect(repo.branchLaneMap[`jjk/return-${state.id}`]).toBeUndefined();
    expect(repo.returnContext?.stateId).toBe(state.id);
    expect(repo.states).toHaveLength(stateCountBeforeReturn);
  });

  test("return main arms the next save to land on main", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    saveState(cwd, {
      kind: "save",
      description: "green",
    });

    await runCli(["return", "main"], cwd);
    Bun.write(join(cwd, "notes.txt"), "main again\n");
    const saved = saveState(cwd, {
      kind: "save",
      description: "main refresh",
    }).state;

    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe("main");
    expect(saved.branch).toBe("main");
    expect(run(["git", "rev-parse", "--verify", "main"], { cwd }).stdout).toBe(saved.commit);
  });

  test("return to a continuation-branch tip switches to that branch instead of detaching", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    const green = saveState(cwd, {
      kind: "save",
      description: "green",
    }).state;

    await runCli(["return", "green"], cwd);

    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/green",
    );
    expect(loadRepo(cwd).returnContext?.stateId).toBe(green.id);
    expect(green.continuationBranch).toBe("jjk/green");
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
      description: "green",
    }).state;

    Bun.write(join(cwd, "notes.txt"), "purple\n");
    const purple = saveState(cwd, {
      kind: "step",
      description: "purple",
    }).state;

    await runCli(["return", baseline.id], cwd);

    const branchBeforeSave = run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], {
      cwd,
    }).stdout;
    expect(branchBeforeSave).toBe("jjk/green");
    expect(loadRepo(cwd).returnContext?.stateId).toBe(baseline.id);

    Bun.write(join(cwd, "notes.txt"), "alpha\norange=true\n");
    const orange = saveState(cwd, {
      kind: "save",
      description: "orange",
    }).state;

    const branch = run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], {
      cwd,
    }).stdout;

    expect(branch).toBe("jjk/orange");
    expect(orange.branch).toBe("jjk/orange");
    expect(orange.parentStateId).toBe(baseline.id);
    expect(purple.continuationBranch).toBe("jjk/purple");
    expect(loadRepo(cwd).returnContext).toBeNull();
  });

  test("return matching does not use lane names", () => {
    saveState(cwd, {
      kind: "save",
      description: "green",
    });
    saveState(cwd, {
      kind: "save",
      description: "purple",
    });

    const repo = loadRepo(cwd);
    const matches = findStateMatches(repo.states, "main");

    expect(matches.map((match) => match.state.description)).toEqual(["main"]);
  });
});
