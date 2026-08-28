import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, loadRepo } from "../src/store";
import { renderGraph, renderStateTable } from "../src/render";
import { run } from "../src/shell";

describe("branch organization", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-branches-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("keeps green purple and orange as stable branches while later states stay on purple and orange", { timeout: 10000 }, async () => {
    const filePath = join(cwd, "snake.py");

    Bun.write(filePath, "color=green\nfast=false\n");
    await runCli(["green"], cwd);

    Bun.write(filePath, "color=purple\nfast=false\n");
    await runCli(["purple"], cwd);

    await runCli(["return", "green"], cwd);
    Bun.write(filePath, "color=orange\nfast=false\n");
    await runCli(["orange"], cwd);

    await runCli(["return", "purple"], cwd);
    Bun.write(filePath, "color=purple\nfast=true\n");
    await runCli(["fast_purple"], cwd);

    await runCli(["return", "orange"], cwd);
    await runCli(["pick", "fast_purple"], cwd);
    await runCli(["nice", "fast_orange"], cwd);

    const repo = loadRepo(cwd);
    const green = repo.states.find((state) => state.description === "green");
    const purple = repo.states.find((state) => state.description === "purple");
    const orange = repo.states.find((state) => state.description === "orange");
    const fastPurple = repo.states.find((state) => state.description === "fast_purple");
    const fastOrange = repo.states.find((state) => state.description === "fast_orange");

    expect(green?.continuationBranch).toBe("jjk/green");
    expect(purple?.continuationBranch).toBe("jjk/purple");
    expect(orange?.branch).toBe("jjk/orange");
    expect(orange?.continuationBranch).toBe("jjk/orange");
    expect(fastPurple?.branch).toBe("jjk/fast_purple");
    expect(fastPurple?.continuationBranch).toBe("jjk/fast_purple");
    expect(fastOrange?.branch).toBe("jjk/orange");
    expect(fastOrange?.continuationBranch).toBe("jjk/orange");
    expect(purple?.parentStateId).toBe(green?.id);
    expect(orange?.parentStateId).toBe(green?.id);
    expect(fastPurple?.parentStateId).toBe(purple?.id);

    expect(run(["git", "rev-parse", "--verify", "refs/heads/jjk/green"], { cwd }).stdout.length).toBeGreaterThan(10);
    expect(run(["git", "rev-parse", "--verify", "refs/heads/jjk/purple"], { cwd }).stdout.length).toBeGreaterThan(10);
    expect(run(["git", "rev-parse", "--verify", "refs/heads/jjk/orange"], { cwd }).stdout.length).toBeGreaterThan(10);
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe("jjk/orange");
    expect(run(["git", "rev-parse", `${purple?.commit}^`], { cwd }).stdout).toBe(green?.commit);
    expect(run(["git", "rev-parse", `${orange?.commit}^`], { cwd }).stdout).toBe(green?.commit);
    expect(run(["git", "rev-parse", `${fastPurple?.commit}^`], { cwd }).stdout).toBe(purple?.commit);

    const graph = renderGraph(repo, { currentStateId: fastOrange?.id ?? null });
    const table = renderStateTable(repo.states);
    const main = repo.states.find((state) => state.description === "main");
    expect(graph).toContain(`└─  ^   ${main?.id} [save] main (main)`);
    expect(graph).toContain(`└─  ^   ${green?.id} [new] green (jjk/green)`);
    expect(graph).toContain(`├─  ^   ${purple?.id} [new] purple (jjk/purple)`);
    expect(graph).toContain(`└─  ^   ${fastPurple?.id} [new] fast_purple (jjk/fast_purple)`);
    expect(graph).toContain(`└─ *^   ${fastOrange?.id} [nice] fast_orange (jjk/orange)`);
    expect(table).toContain("jjk/purple");
    expect(table).toContain("jjk/orange");
  });
});
