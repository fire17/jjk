import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { createLane, initSafeSpace, saveState } from "../src/store";
import { run } from "../src/shell";

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

    expect(readFileSync(join(cwd, "notes.txt"), "utf8")).toBe("alpha\nbeta\n");
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

    expect(readFileSync(filePath, "utf8")).toBe("color=orange\nfast=true\n");
  });
});
