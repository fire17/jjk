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

describe("inspection and filtering commands", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-inspection-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("inspect prints detailed metadata for a chosen state", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);

    const repo = loadRepo(cwd);
    const green = repo.states.find((state) => state.description === "green");
    const purple = repo.states.find((state) => state.description === "purple");
    expect(green).toBeTruthy();
    expect(purple).toBeTruthy();

    const output = await captureCli(["inspect", "green"], cwd);
    expect(output).toContain("description: green");
    expect(output).toContain("branch: jjk/green");
    expect(output).toContain("lane: jjk/green");
    expect(output).toContain(`parent: ${repo.states.find((state) => state.description === "main")?.id.slice(0, 8)} main`);
    expect(output).toContain(`children: ${purple?.id.slice(0, 8)} purple`);
    expect(output).toContain("deleted: no");
  });

  test("search returns ranked matches as a table", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);
    Bun.write(filePath, "super purple\n");
    await runCli(["super_purple"], cwd);

    const output = await captureCli(["search", "pur"], cwd);
    const purpleIndex = output.indexOf("purple");
    const superPurpleIndex = output.indexOf("super_purple");

    expect(output).toContain("Search results for `pur`:");
    expect(output).toContain("jjk/purple");
    expect(output).toContain("jjk/super_purple");
    expect(purpleIndex).toBeGreaterThan(-1);
    expect(superPurpleIndex).toBeGreaterThan(-1);
    expect(purpleIndex).toBeLessThan(superPurpleIndex);
  });

  test("timeline shows states in chronological order", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);

    const output = await captureCli(["timeline"], cwd);
    const mainIndex = output.indexOf("main");
    const greenIndex = output.indexOf("green");
    const purpleIndex = output.indexOf("purple");

    expect(mainIndex).toBeGreaterThan(-1);
    expect(greenIndex).toBeGreaterThan(mainIndex);
    expect(purpleIndex).toBeGreaterThan(greenIndex);
  });

  test("graph can be filtered to a single branch", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);

    const output = await captureCli(["graph", "--branch", "jjk/purple"], cwd);
    expect(output).toContain("(jjk/purple)");
    expect(output).not.toContain("(jjk/green)");
  });

  test("see filters by kind, tag, and since", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    await runCli(["star"], cwd);

    const kindOutput = await captureCli(["see", "--kind", "new"], cwd);
    expect(kindOutput).toContain("[new]");
    expect(kindOutput).not.toContain("[save] main");

    const tagOutput = await captureCli(["see", "--tag", "star"], cwd);
    expect(tagOutput).toContain("★");
    expect(tagOutput).toContain("green");

    const sinceOutput = await captureCli(["see", "--since", "2999-01-01T00:00:00.000Z"], cwd);
    expect(sinceOutput).toContain("No states matched the selected filters.");
  });

  test("favorites lists starred states only", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    await runCli(["star"], cwd);

    const output = await captureCli(["favorites"], cwd);
    expect(output).toContain("★");
    expect(output).toContain("green");
    expect(output).not.toContain("main");
  });

  test("compare-branch compares the latest state on two branches", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);

    const output = await captureCli(["compare-branch", "jjk/green", "jjk/purple"], cwd);
    expect(output).toContain("branch a:");
    expect(output).toContain("branch b:");
    expect(output).toContain("green");
    expect(output).toContain("purple");
  });
});
