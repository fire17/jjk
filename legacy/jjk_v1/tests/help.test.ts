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
    expect(base).toContain("jjk graph [--deleted]");
    expect(base).toContain("jjk see [--deleted] [--table] [-v2] [-v3] [-v4]");
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

  test("see command shows only the tree by default", async () => {
    await runCli(["green"], cwd);
    await runCli(["purple"], cwd);
    await runCli(["return", "green"], cwd);
    await runCli(["orange"], cwd);

    const output = await capture(["see"], cwd);
    const firstNonEmptyLine = output.split("\n").find((line) => line.trim().length > 0);

    expect(firstNonEmptyLine).toBe("★ starred    * current state    ^ branch leaf");
    expect(output).not.toContain("label | message");
    expect(output).toContain("(jjk/orange)");
  });

  test("see --table shows the table first and ends with the tree", async () => {
    await runCli(["green"], cwd);
    await runCli(["purple"], cwd);
    await runCli(["return", "green"], cwd);
    await runCli(["orange"], cwd);

    const output = await capture(["see", "--table"], cwd);
    const lines = output.split("\n");
    const firstNonEmptyLine = lines.find((line) => line.trim().length > 0);
    const lastNonEmptyLine = [...lines].reverse().find((line) => line.trim().length > 0);

    expect(firstNonEmptyLine).toContain("id");
    expect(firstNonEmptyLine).toContain("label | message");
    expect(firstNonEmptyLine).toContain("datetime");
    expect(output.indexOf("id        git       kind")).toBeLessThan(output.indexOf("★ starred    * current state    ^ branch leaf"));
    expect(lastNonEmptyLine).toContain("(jjk/orange)");
  });

  test("see -v2 keeps same-branch states aligned without changing default see", async () => {
    await runCli(["green"], cwd);
    await runCli(["save", "green_checkpoint"], cwd);
    await runCli(["purple"], cwd);

    const defaultOutput = await capture(["see"], cwd);
    const v2Output = await capture(["see", "-v2"], cwd);

    expect(defaultOutput).toContain("      └─  ^");
    expect(v2Output).toContain("   └─  ^");
    expect(v2Output).toContain("      └─ *^");
  });

  test("see -v3 draws aligned continuation with fork lines and keeps default see unchanged", async () => {
    await runCli(["green"], cwd);
    await runCli(["save", "green_checkpoint"], cwd);
    await runCli(["purple"], cwd);
    await runCli(["return", "green_checkpoint"], cwd);
    await runCli(["orange"], cwd);
    await runCli(["return", "purple"], cwd);
    await runCli(["save", "purple_polish"], cwd);

    const defaultOutput = await capture(["see"], cwd);
    const v3Output = await capture(["see", "-v3"], cwd);

    expect(defaultOutput).toContain("         └─");
    expect(v3Output).toContain("      ├─");
    expect(v3Output).toContain("   │  └─");
    expect(v3Output).toContain("      └─");
  });

  test("see -v4 trims one indent level for same-branch states from the original graph and keeps default see unchanged", async () => {
    await runCli(["green"], cwd);
    await runCli(["save", "green_checkpoint"], cwd);
    await runCli(["purple"], cwd);
    await runCli(["return", "main"], cwd);
    await runCli(["blue"], cwd);

    const defaultOutput = await capture(["see"], cwd);
    const v4Output = await capture(["see", "-v4"], cwd);

    expect(defaultOutput).toContain("   │  └─  ^");
    expect(v4Output).toContain("│  └─  ^");
    expect(v4Output).toContain("│     └─");
    expect(v4Output).toContain("   └─  ^");
  });

  test("git log command forwards color in interactive output", async () => {
    await runCli(["green"], cwd);

    const output = await capture(["git", "log"], cwd, { tty: true });
    expect(output).toContain("\u001b[");
    expect(output).toContain("HEAD ->");
  });
});
