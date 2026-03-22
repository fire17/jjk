import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initGitRepo, createSnapshotCommit, updateRef } from "../src/git";
import { runCli } from "../src/commands";
import { initSafeSpace } from "../src/store";
import { run } from "../src/shell";

describe("git snapshotting", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-test-"));
    initGitRepo(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("creates a real git commit and stages untracked files", () => {
    writeFileSync(join(cwd, "notes.txt"), "first version\n");
    const snapshot = createSnapshotCommit(cwd, "jjk save: first version");
    updateRef(cwd, "refs/jjk/states/test1234", snapshot.commit);

    expect(snapshot.commit.length).toBeGreaterThan(10);
    const ref = run(["git", "rev-parse", "refs/jjk/states/test1234"], { cwd });
    expect(ref.stdout).toBe(snapshot.commit);

    const head = run(["git", "rev-parse", "--verify", "HEAD"], { cwd });
    expect(head.stdout).toBe(snapshot.commit);
    expect(
      run(["git", "status", "--short", "--untracked-files=all"], {
        cwd,
        allowFailure: true,
      }).stdout,
    ).toBe("");
  });

  test("visible git-log commits have non-empty subjects and bodies in the snake branch flow", async () => {
    initSafeSpace(cwd);
    await runCli(["snapshots", "on"], cwd);

    writeFileSync(join(cwd, "snake.py"), "green\n");
    await runCli(["green"], cwd);

    writeFileSync(join(cwd, "snake.py"), "purple\n");
    await runCli(["purple"], cwd);

    await runCli(["return", "green"], cwd);
    writeFileSync(join(cwd, "snake.py"), "orange\n");
    await runCli(["orange"], cwd);

    await runCli(["return", "purple"], cwd);
    writeFileSync(join(cwd, "snake.py"), "fast purple\n");
    await runCli(["fast_purple"], cwd);

    const records = run(
      ["git", "log", "--all", "--format=%s%x1f%b%x1e", "-n", "40"],
      { cwd, allowFailure: true },
    ).stdout
      .split("\x1e")
      .map((record) => record.trim())
      .filter(Boolean)
      .map((record) => {
        const [subject = "", body = ""] = record.split("\x1f");
        return { subject: subject.trim(), body: body.trim() };
      });

    expect(records.length).toBeGreaterThan(0);
    expect(records.every((record) => record.subject.length > 0)).toBe(true);
    expect(records.every((record) => record.body.length > 0)).toBe(true);
    expect(records.some((record) => record.subject === "green [save] (jjk/green) - jjk")).toBe(
      true,
    );
    expect(
      records.some((record) => record.subject === "fast_purple [save] (jjk/purple) - jjk"),
    ).toBe(true);

    const snapshotRecords = records.filter((record) =>
      record.subject.startsWith("jjk workspace snapshot"),
    );
    expect(snapshotRecords.length).toBeGreaterThan(0);
    expect(snapshotRecords.every((record) => record.subject !== "jjk workspace snapshot")).toBe(
      true,
    );
    expect(
      snapshotRecords.every(
        (record) =>
          record.body.includes("Nearest-Ancestor-State:") &&
          record.body.includes("Nearest-Descendant-State:"),
      ),
    ).toBe(true);
  });

  test("workspace snapshot refs stay out of git by default and can be toggled back on", async () => {
    initSafeSpace(cwd);

    writeFileSync(join(cwd, "snake.py"), "green\n");
    await runCli(["green"], cwd);

    const keepRefsOff = run(
      ["git", "for-each-ref", "--format=%(refname)", "refs/jj/keep"],
      { cwd, allowFailure: true },
    ).stdout.trim();
    expect(keepRefsOff).toBe("");

    await runCli(["snapshots", "on"], cwd);
    writeFileSync(join(cwd, "snake.py"), "purple\n");
    await runCli(["purple"], cwd);

    const keepRefsOn = run(
      ["git", "for-each-ref", "--format=%(refname)", "refs/jj/keep"],
      { cwd, allowFailure: true },
    ).stdout.trim();
    expect(keepRefsOn.length).toBeGreaterThan(0);

    await runCli(["snapshots", "off"], cwd);
    const keepRefsPruned = run(
      ["git", "for-each-ref", "--format=%(refname)", "refs/jj/keep"],
      { cwd, allowFailure: true },
    ).stdout.trim();
    expect(keepRefsPruned).toBe("");
  });
});
