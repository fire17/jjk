import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, loadRepo } from "../src/store";
import { run } from "../src/shell";

describe("save modes", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-save-mode-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("bare jjk description creates a new branch state of kind new", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    await runCli(["green"], cwd);

    const repo = loadRepo(cwd);
    const green = repo.states.find((state) => state.description === "green");

    expect(green?.kind).toBe("new");
    expect(green?.branch).toBe("jjk/green");
    expect(green?.continuationBranch).toBe("jjk/green");
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/green",
    );
  });

  test("jjk save keeps saving on the current branch as kind save", async () => {
    Bun.write(join(cwd, "notes.txt"), "baseline\n");
    await runCli(["green"], cwd);

    Bun.write(join(cwd, "notes.txt"), "saved on same branch\n");
    await runCli(["save", "checkpoint"], cwd);

    const repo = loadRepo(cwd);
    const saved = repo.states.find((state) => state.description === "checkpoint");

    expect(saved?.kind).toBe("save");
    expect(saved?.branch).toBe("jjk/green");
    expect(saved?.continuationBranch).toBeNull();
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/green",
    );
  });

  test("jjk save on main stays on main instead of opening a jjk branch", async () => {
    Bun.write(join(cwd, "notes.txt"), "saved on main\n");
    await runCli(["save", "main_checkpoint"], cwd);

    const repo = loadRepo(cwd);
    const saved = repo.states.find((state) => state.description === "main_checkpoint");

    expect(saved?.kind).toBe("save");
    expect(saved?.branch).toBe("main");
    expect(saved?.continuationBranch).toBeNull();
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "main",
    );
  });

  test("jjk save from a detached non-leaf return continues the source branch", async () => {
    const filePath = join(cwd, "snake.py");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);

    Bun.write(filePath, "purple stable\n");
    await runCli(["save", "stable_purple"], cwd);

    const repoBeforeReturn = loadRepo(cwd);
    const purple = repoBeforeReturn.states.find((state) => state.description === "purple");
    const stablePurple = repoBeforeReturn.states.find((state) => state.description === "stable_purple");

    await runCli(["return", purple!.id], cwd);
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], {
      cwd,
      allowFailure: true,
    }).stdout).toBe("");

    Bun.write(filePath, "purple rescued\n");
    await runCli(["save", "rescued_purple"], cwd);

    const repo = loadRepo(cwd);
    const rescued = repo.states.find((state) => state.description === "rescued_purple");

    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/purple",
    );
    expect(rescued?.kind).toBe("save");
    expect(rescued?.branch).toBe("jjk/purple");
    expect(rescued?.parentStateId).toBe(purple?.id);
    expect(run(["git", "rev-parse", `${rescued?.commit}^`], { cwd }).stdout).toBe(
      stablePurple?.commit,
    );
    expect(
      run(["git", "show-ref", "--verify", "--quiet", "refs/heads/jjk/rescued_purple"], {
        cwd,
        allowFailure: true,
      }).exitCode,
    ).not.toBe(0);
  });

  test("jjk nice from a detached non-leaf return continues the source branch", async () => {
    const filePath = join(cwd, "snake.py");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);

    Bun.write(filePath, "purple stable\n");
    await runCli(["save", "stable_purple"], cwd);

    const repoBeforeReturn = loadRepo(cwd);
    const purple = repoBeforeReturn.states.find((state) => state.description === "purple");
    const stablePurple = repoBeforeReturn.states.find((state) => state.description === "stable_purple");

    await runCli(["return", purple!.id], cwd);
    Bun.write(filePath, "purple polished\n");
    await runCli(["nice", "polished_purple"], cwd);

    const repo = loadRepo(cwd);
    const polished = repo.states.find((state) => state.description === "polished_purple");

    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/purple",
    );
    expect(polished?.kind).toBe("nice");
    expect(polished?.branch).toBe("jjk/purple");
    expect(polished?.parentStateId).toBe(purple?.id);
    expect(run(["git", "rev-parse", `${polished?.commit}^`], { cwd }).stdout).toBe(
      stablePurple?.commit,
    );
    expect(
      run(["git", "show-ref", "--verify", "--quiet", "refs/heads/jjk/polished_purple"], {
        cwd,
        allowFailure: true,
      }).exitCode,
    ).not.toBe(0);
  });
});
