import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, loadRepo, resolveLatestStateForBranch } from "../src/store";
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

describe("navigation commands", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-navigation-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("where, root, trail, children, and parents report the expected state relationships", async () => {
    const filePath = join(cwd, "notes.txt");
    const initialMain = loadRepo(cwd).states.find((state) => state.description === "main")!;

    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    Bun.write(filePath, "purple\n");
    await runCli(["return", "green"], cwd);
    await runCli(["purple"], cwd);

    const where = await captureCli(["where"], cwd);
    const root = await captureCli(["root", "purple"], cwd);
    const trail = await captureCli(["trail", "purple"], cwd);
    const children = await captureCli(["children", "green"], cwd);
    const parents = await captureCli(["parents", "purple"], cwd);

    expect(where).toContain("purple");
    expect(where).toContain("jjk/purple");
    expect(root).toContain("main");
    expect(trail).toContain(initialMain.id.slice(0, 8));
    expect(trail).toContain("green");
    expect(trail).toContain("purple");
    expect(children).toContain("purple");
    expect(parents).toContain("green");
  });

  test("heads and branch log focus on one branch at a time", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    Bun.write(filePath, "purple\n");
    await runCli(["return", "green"], cwd);
    await runCli(["purple"], cwd);

    const heads = await captureCli(["heads"], cwd);
    const log = await captureCli(["log", "jjk/green"], cwd);

    expect(heads).toContain("jjk/green");
    expect(heads).toContain("jjk/purple");
    expect(heads).toContain("* jjk/purple");
    expect(log).toContain("green");
    expect(log).not.toContain("purple");
  });

  test("next and prev move through parent and child states", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    Bun.write(filePath, "purple\n");
    await runCli(["return", "green"], cwd);
    await runCli(["purple"], cwd);

    await runCli(["prev"], cwd);
    const afterPrev = await captureCli(["current"], cwd);
    expect(afterPrev).toContain("green");

    await runCli(["next"], cwd);
    const afterNext = await captureCli(["current"], cwd);
    expect(afterNext).toContain("purple");
  });

  test("continue resumes the current branch tip", async () => {
    const filePath = join(cwd, "notes.txt");
    const initialMain = loadRepo(cwd).states.find((state) => state.description === "main")!;

    Bun.write(filePath, "main refresh\n");
    await runCli(["return", "main"], cwd);
    await runCli(["save", "main refresh"], cwd);
    await runCli(["return", initialMain.id], cwd);

    const expected = resolveLatestStateForBranch(cwd, "main");
    const output = await captureCli(["continue"], cwd);

    expect(output).toContain("continued to");
    expect(output).toContain(expected.id.slice(0, 8));
    expect(output).toContain(expected.label);
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe("main");
  });
});
