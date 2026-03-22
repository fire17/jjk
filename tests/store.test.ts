import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initSafeSpace, listStates, saveState } from "../src/store";
import { run } from "../src/shell";

describe("store", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-store-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
    Bun.write(join(cwd, "app.txt"), "hello\n");
  });

  test("saveState records state metadata", () => {
    const result = saveState(cwd, {
      kind: "save",
      description: "baseline before parser rewrite",
      message: "parser rewrite in progress",
    });

    expect(result.state.description).toBe("baseline before parser rewrite");
    expect(result.state.metadata?.gitCommit).toBe(result.state.commit);
    expect(result.state.metadata?.message).toBe("parser rewrite in progress");
    const states = listStates(cwd);
    expect(states[0]?.description).toBe("main");
    expect(states[1]?.metadata?.gitCommit).toBe(states[1]?.commit);
    expect(states[1]?.metadata?.message).toBe("parser rewrite in progress");
    expect(states).toHaveLength(2);
  });

  test("init creates the anchor state on main and later saves do not advance main by default", () => {
    const initialHead = run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout;

    Bun.write(join(cwd, "app.txt"), "purple\n");
    const result = saveState(cwd, {
      kind: "save",
      description: "purple",
    });

    expect(run(["git", "rev-parse", "--verify", "main"], { cwd }).stdout).toBe(initialHead);
    expect(run(["git", "rev-parse", "--verify", "refs/heads/jjk/purple"], { cwd }).stdout).toBe(
      result.state.commit,
    );
    expect(result.state.branch).toBe("main");
    expect(result.state.continuationBranch).toBe("jjk/purple");
  });
});
