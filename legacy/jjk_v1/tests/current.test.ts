import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, saveState } from "../src/store";
import { run } from "../src/shell";
import { shortStateId } from "../src/utils";

function captureLogs(): {
  output: string[];
  restore: () => void;
} {
  const output: string[] = [];
  const originalLog = console.log;
  console.log = (...args: unknown[]) => {
    output.push(args.join(" "));
  };

  return {
    output,
    restore: () => {
      console.log = originalLog;
    },
  };
}

describe("current command", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-current-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("reports the current saved state on the active branch", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    const green = saveState(cwd, {
      kind: "save",
      description: "green",
    }).state;
    await runCli(["return", green.id], cwd);

    const { output, restore } = captureLogs();
    try {
      await runCli(["current"], cwd);
    } finally {
      restore();
    }

    const text = output.join("\n");
    expect(text).toContain(`current state: ${shortStateId(green.id)} [save] green`);
    expect(text).toContain("lane: main");
    expect(text).toContain("branch: jjk/green");
    expect(text).toContain("workspace: jjk/green");
  });

  test("reports detached workspace context when returning to a non-tip state", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    const green = saveState(cwd, {
      kind: "save",
      description: "green",
    }).state;
    await runCli(["return", green.id], cwd);

    Bun.write(join(cwd, "notes.txt"), "purple\n");
    saveState(cwd, {
      kind: "step",
      description: "purple",
    });

    await runCli(["return", green.id], cwd);

    const { output, restore } = captureLogs();
    try {
      await runCli(["current"], cwd);
    } finally {
      restore();
    }

    const text = output.join("\n");
    expect(text).toContain(`current state: ${shortStateId(green.id)} [save] green`);
    expect(text).toContain("workspace: detached");
    expect(text).toContain("parent: ");
    expect(text).toContain("history: ");
  });
});
