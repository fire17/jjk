import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, saveState } from "../src/store";
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

describe("show and diff commands", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-show-diff-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("show prints the atomic patch for the selected state", async () => {
    const filePath = join(cwd, "notes.txt");

    Bun.write(filePath, "alpha\n");
    saveState(cwd, {
      kind: "save",
      description: "alpha",
    });

    Bun.write(filePath, "alpha\nbeta\n");
    const beta = saveState(cwd, {
      kind: "save",
      description: "beta",
    }).state;

    const output = await captureCli(["show", beta.id], cwd);

    expect(output).toContain("--- a/notes.txt");
    expect(output).toContain("+++ b/notes.txt");
    expect(output).toContain("+beta");
  });

  test("diff compares full saved snapshots by default", async () => {
    const modePath = join(cwd, "mode.txt");
    const extraPath = join(cwd, "extra.txt");

    Bun.write(modePath, "mode=base\n");
    Bun.write(extraPath, "extra=plain\n");
    const baseline = saveState(cwd, {
      kind: "save",
      description: "baseline",
    }).state;

    Bun.write(modePath, "mode=fast\n");
    const fastPlain = saveState(cwd, {
      kind: "save",
      description: "fast_plain",
    }).state;

    await runCli(["return", baseline.id], cwd);
    Bun.write(extraPath, "extra=red\n");
    const fastRed = saveState(cwd, {
      kind: "save",
      description: "fast_red",
    }).state;

    const output = await captureCli(["diff", fastPlain.id, fastRed.id], cwd);

    expect(output).toContain("--- /dev/null");
    expect(output).toContain("+extra=red");
  });

  test("diff --atomic compares only the changes held by the selected states", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "alpha\n");
    saveState(cwd, {
      kind: "save",
      description: "alpha",
    });

    Bun.write(filePath, "alpha\nbeta\n");
    const beta = saveState(cwd, {
      kind: "save",
      description: "beta",
    }).state;

    const output = await captureCli(["diff", "--atomic", beta.id, beta.id], cwd);

    expect(output).toBe("No diff between selected atomic state changes.");
  });
});
