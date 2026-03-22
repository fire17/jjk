import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace } from "../src/store";
import { run } from "../src/shell";

describe("help output", () => {
  const originalLog = console.log;
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-help-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  afterEach(() => {
    console.log = originalLog;
  });

  async function capture(argv: string[], runCwd = process.cwd()): Promise<string> {
    const logs: string[] = [];
    console.log = (...args: unknown[]) => {
      logs.push(args.join(" "));
    };
    await runCli(argv, runCwd);
    return logs.join("\n");
  }

  test("help aliases all print the same expanded help output", async () => {
    const base = await capture(["help"]);
    const slash = await capture(["/help"]);
    const long = await capture(["--help"]);
    const dash = await capture(["-help"]);

    expect(slash).toBe(base);
    expect(long).toBe(base);
    expect(dash).toBe(base);
    expect(base).toContain("jjk graph [--deleted]");
    expect(base).toContain("Examples:");
    expect(base).toContain("Basic:");
    expect(base).toContain("Advanced flow:");
  });

  test("graph command prints the log-style state graph", async () => {
    await runCli(["green"], cwd);
    await runCli(["purple"], cwd);
    await runCli(["return", "green"], cwd);
    await runCli(["orange"], cwd);

    const output = await capture(["graph"], cwd);
    expect(output).toContain("[current, leaf]");
    expect(output).toContain("[leaf]");
    expect(output).toContain("(jjk/orange)");
    expect(output).toContain("| *");
  });
});
