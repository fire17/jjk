import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { createLane, initSafeSpace, loadRepo, saveState } from "../src/store";
import { run } from "../src/shell";
import { shortStateId } from "../src/utils";

describe("pick flow", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-pick-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("pick harvests a saved state onto another branch context", async () => {
    Bun.write(join(cwd, "notes.txt"), "alpha\n");
    const baseline = saveState(cwd, {
      kind: "star",
      description: "baseline anchor",
    }).state;

    createLane(cwd, "feature harvest");
    Bun.write(join(cwd, "notes.txt"), "alpha\nbeta\n");
    const harvested = saveState(cwd, {
      kind: "step",
      description: "beta addition",
    }).state;

    await runCli(["return", baseline.id], cwd);
    await runCli(["pick", harvested.id], cwd);

    const repo = loadRepo(cwd);
    const picked = repo.states[repo.states.length - 1];

    expect(readFileSync(join(cwd, "notes.txt"), "utf8")).toBe("alpha\nbeta\n");
    expect(picked?.kind).toBe("cherry");
    expect(picked?.label).toBe("cherry_beta_addition");
    expect(picked?.metadata?.base).toBe(baseline.id);
    expect(picked?.metadata?.cherry).toBe(harvested.id);
    expect(repo.currentStateHistory?.entries.at(-1)).toBe(picked?.id);
    expect(repo.returnContext?.stateId).toBe(picked?.id);
  });

  test("pick applies only the delta held by the chosen state after multiple returns", async () => {
    const filePath = join(cwd, "game.txt");

    Bun.write(filePath, "color=green\nfast=false\n");
    const green = saveState(cwd, {
      kind: "star",
      description: "green baseline",
    }).state;

    Bun.write(filePath, "color=purple\nfast=false\n");
    const purple = saveState(cwd, {
      kind: "step",
      description: "purple snake",
    }).state;

    await runCli(["return", green.id], cwd);
    Bun.write(filePath, "color=orange\nfast=false\n");
    const orange = saveState(cwd, {
      kind: "step",
      description: "orange snake",
    }).state;

    await runCli(["return", purple.id], cwd);
    Bun.write(filePath, "color=purple\nfast=true\n");
    const fastPurple = saveState(cwd, {
      kind: "step",
      description: "fast purple",
    }).state;

    await runCli(["return", orange.id], cwd);
    await runCli(["pick", fastPurple.id], cwd);

    const repo = loadRepo(cwd);
    const picked = repo.states[repo.states.length - 1];

    expect(readFileSync(filePath, "utf8")).toBe("color=orange\nfast=true\n");
    expect(picked?.kind).toBe("cherry");
    expect(picked?.label).toBe("cherry_fast_purple");
    expect(picked?.metadata?.base).toBe(orange.id);
    expect(picked?.metadata?.cherry).toBe(fastPurple.id);
    expect(repo.currentStateHistory?.entries.at(-1)).toBe(picked?.id);
    expect(repo.returnContext?.stateId).toBe(picked?.id);
  });

  test("pick makes the new cherry state the visible current state", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    const green = saveState(cwd, {
      kind: "new",
      description: "green",
    }).state;

    Bun.write(join(cwd, "notes.txt"), "purple\n");
    const purple = saveState(cwd, {
      kind: "save",
      description: "purple",
    }).state;

    await runCli(["return", green.id], cwd);
    await runCli(["pick", purple.id], cwd);

    const repo = loadRepo(cwd);
    const picked = repo.states[repo.states.length - 1];
    const output: string[] = [];
    const originalLog = console.log;
    console.log = (...args: unknown[]) => {
      output.push(args.join(" "));
    };

    try {
      await runCli(["see"], cwd);
    } finally {
      console.log = originalLog;
    }

    const rendered = output.join("\n");
    expect(rendered).toMatch(new RegExp(`\\*\\^\\s+${shortStateId(picked!.id)} \\[cherry\\]`));
    expect(rendered).toContain("cherry_purple");
    expect(rendered).toContain(shortStateId(green.id));
    expect(rendered).toContain(shortStateId(purple.id));
    expect(picked?.label).toBe("cherry_purple");
    expect(picked?.metadata?.base).toBe(green.id);
    expect(picked?.metadata?.cherry).toBe(purple.id);
    expect(repo.returnContext?.stateId).toBe(picked?.id);
  });
});
