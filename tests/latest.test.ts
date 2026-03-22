import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, loadRepo } from "../src/store";
import { run } from "../src/shell";
import { shortStateId } from "../src/utils";

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

describe("lastest command", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-lastest-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("returns the latest jjk state for a branch", async () => {
    const filePath = join(cwd, "notes.txt");

    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);
    const purple = loadRepo(cwd).states.at(-1)!;

    const output = await captureCli(["lastest", "jjk/purple"], cwd);

    expect(output).toContain(shortStateId(purple.id));
    expect(output).toContain("[new]");
    expect(output).toContain("purple");
    expect(output).toContain("branch=jjk/purple");
  });

  test("accepts lane-like branch lookup and follows updated branch metadata", async () => {
    const filePath = join(cwd, "notes.txt");

    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    Bun.write(filePath, "purple\n");
    await runCli(["purple"], cwd);
    const purple = loadRepo(cwd).states.at(-1)!;

    await runCli(["update", "jjk/purple", purple.id], cwd);

    const output = await captureCli(["latest", "purple"], cwd);

    expect(output).toContain(shortStateId(purple.id));
    expect(output).toContain("branch=jjk/purple");
  });
});
