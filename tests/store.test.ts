import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initSafeSpace, listStates, loadRepo, saveState } from "../src/store";
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

  test("init imports an existing git repo as chronological jjk states with branch tips and current HEAD", () => {
    const repoCwd = mkdtempSync(join(tmpdir(), "jjk-store-import-"));
    run(["git", "init", "-b", "main"], { cwd: repoCwd });
    run(["git", "config", "user.name", "jjk test"], { cwd: repoCwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd: repoCwd });

    const commit = (message: string, content: string, timestamp: string) => {
      Bun.write(join(repoCwd, "app.txt"), content);
      run(["git", "add", "app.txt"], { cwd: repoCwd });
      run(["git", "commit", "-m", message], {
        cwd: repoCwd,
        env: {
          GIT_AUTHOR_DATE: timestamp,
          GIT_COMMITTER_DATE: timestamp,
        },
      });
    };

    commit("root", "root\n", "2026-01-01T00:00:00Z");
    commit("green", "green\n", "2026-01-01T01:00:00Z");
    run(["git", "switch", "-c", "purple"], { cwd: repoCwd });
    commit("purple", "purple\n", "2026-01-01T02:00:00Z");
    const purpleHead = run(["git", "rev-parse", "--verify", "HEAD"], { cwd: repoCwd }).stdout;
    run(["git", "switch", "main"], { cwd: repoCwd });
    commit("orange", "orange\n", "2026-01-01T03:00:00Z");
    const orangeHead = run(["git", "rev-parse", "--verify", "HEAD"], { cwd: repoCwd }).stdout;
    run(["git", "switch", "purple"], { cwd: repoCwd });

    initSafeSpace(repoCwd);
    const repo = loadRepo(repoCwd);

    expect(repo.states.map((state) => state.description)).toEqual(["root", "green", "purple", "orange"]);
    expect(repo.states.map((state) => state.branch)).toEqual(["main", "main", "purple", "main"]);
    expect(repo.currentStateHistory?.entries).toEqual([repo.states[2]?.id]);
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd: repoCwd }).stdout).toBe("purple");
    expect(run(["git", "rev-parse", "--verify", "HEAD"], { cwd: repoCwd }).stdout).toBe(purpleHead);
    expect(repo.lanes[repo.branchLaneMap["main"] ?? ""]?.currentStateId).toBe(
      repo.states.find((state) => state.commit === orangeHead)?.id,
    );
    expect(repo.lanes[repo.branchLaneMap["purple"] ?? ""]?.currentStateId).toBe(
      repo.states.find((state) => state.commit === purpleHead)?.id,
    );
  });

  test("loadRepo auto-imports raw git commits created outside jjk into the correct branch and current HEAD", () => {
    Bun.write(join(cwd, "app.txt"), "green\n");
    const green = saveState(cwd, {
      kind: "save",
      description: "green",
    }).state;

    run(["git", "switch", "jjk/green"], { cwd });
    run(["git", "switch", "-c", "purple"], { cwd });
    Bun.write(join(cwd, "app.txt"), "purple from git\n");
    run(["git", "add", "app.txt"], { cwd });
    run(["git", "commit", "-m", "purple raw git"], { cwd });
    const purpleHead = run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout;

    const repo = loadRepo(cwd);
    const imported = repo.states.find((state) => state.commit === purpleHead);

    expect(imported).toBeTruthy();
    expect(imported?.description).toBe("purple raw git");
    expect(imported?.branch).toBe("purple");
    expect(imported?.parentStateId).toBe(green.id);
    expect(repo.currentStateHistory?.entries.at(-1)).toBe(imported?.id);
    expect(repo.lanes[repo.branchLaneMap["purple"] ?? ""]?.currentStateId).toBe(imported?.id);
  });
});
