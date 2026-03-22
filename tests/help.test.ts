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

  async function capture(
    argv: string[],
    runCwd = process.cwd(),
    options?: {
      tty?: boolean;
    },
  ): Promise<string> {
    const logs: string[] = [];
    const hadOwnIsTTY = Object.prototype.hasOwnProperty.call(process.stdout, "isTTY");
    const originalIsTTY = process.stdout.isTTY;
    console.log = (...args: unknown[]) => {
      logs.push(args.join(" "));
    };
    if (options?.tty !== undefined) {
      Object.defineProperty(process.stdout, "isTTY", {
        configurable: true,
        value: options.tty,
      });
    }
    try {
      await runCli(argv, runCwd);
      return logs.join("\n");
    } finally {
      if (options?.tty !== undefined) {
        if (hadOwnIsTTY) {
          Object.defineProperty(process.stdout, "isTTY", {
            configurable: true,
            value: originalIsTTY,
          });
        } else {
          delete (process.stdout as { isTTY?: boolean }).isTTY;
        }
      }
    }
  }

  test("help aliases all print the same expanded help output", async () => {
    const base = await capture(["help"]);
    const slash = await capture(["/help"]);
    const long = await capture(["--help"]);
    const dash = await capture(["-help"]);

    expect(slash).toBe(base);
    expect(long).toBe(base);
    expect(dash).toBe(base);
    expect(base).toContain("jjk inspect <state>");
    expect(base).toContain("jjk search <query>");
    expect(base).toContain("jjk timeline");
    expect(base).toContain("jjk graph [--deleted] [--branch <branch>]");
    expect(base).toContain("jjk see [--deleted] [--kind <kind>] [--tag <tag>] [--since <time>]");
    expect(base).toContain("jjk favorites");
    expect(base).toContain("jjk compare-branch <a> <b>");
    expect(base).toContain("jjk shell-init [zsh|bash]");
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

  test("graph command colorizes interactive output", async () => {
    await runCli(["green"], cwd);
    await runCli(["purple"], cwd);

    const output = await capture(["graph"], cwd, { tty: true });
    expect(output).toContain("\u001b[38;5;");
    expect(output).toContain("\u001b[0m");
  });

  test("git log command forwards color in interactive output", async () => {
    await runCli(["green"], cwd);

    const output = await capture(["git", "log"], cwd, { tty: true });
    expect(output).toContain("\u001b[");
    expect(output).toContain("HEAD ->");
  });
});
