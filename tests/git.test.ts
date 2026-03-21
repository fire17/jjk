import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initGitRepo, createSnapshotCommit, updateRef } from "../src/git";
import { run } from "../src/shell";

describe("git snapshotting", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-test-"));
    initGitRepo(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("creates a hidden snapshot commit without moving HEAD", () => {
    writeFileSync(join(cwd, "notes.txt"), "first version\n");
    const snapshot = createSnapshotCommit(cwd, "jjk save: first version");
    updateRef(cwd, "refs/jjk/states/test1234", snapshot.commit);

    expect(snapshot.commit.length).toBeGreaterThan(10);
    const ref = run(["git", "rev-parse", "refs/jjk/states/test1234"], { cwd });
    expect(ref.stdout).toBe(snapshot.commit);

    const head = run(["git", "rev-parse", "--verify", "HEAD"], {
      cwd,
      allowFailure: true,
    });
    expect(head.exitCode).not.toBe(0);
  });
});
