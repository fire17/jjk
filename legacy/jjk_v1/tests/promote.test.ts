import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, loadRepo, saveState } from "../src/store";
import { run } from "../src/shell";

describe("promotion flow", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-promote-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("promote creates a new metadata state on the same commit", async () => {
    Bun.write(join(cwd, "notes.txt"), "alpha\n");
    const source = saveState(cwd, {
      kind: "step",
      description: "candidate ready for review",
    }).state;

    await runCli(["promote", source.id, "nice", "approved by human"], cwd);

    const repo = loadRepo(cwd);
    const promoted = repo.states[repo.states.length - 1];

    expect(promoted.kind).toBe("nice");
    expect(promoted.commit).toBe(source.commit);
    expect(promoted.parentStateId).toBe(source.id);
    expect(promoted.description).toBe("approved by human");
    expect(source.kind).toBe("step");
  });
});
