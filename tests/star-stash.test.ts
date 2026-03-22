import { beforeEach, describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, loadRepo } from "../src/store";
import { run } from "../src/shell";
import { stateDisplayBranch } from "../src/utils";

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

describe("star and stash commands", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-star-stash-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("star saves a starred state and jjk see shows the star icon", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "milestone\n");

    await runCli(["star", "milestone"], cwd);

    const state = loadRepo(cwd).states.at(-1)!;
    const output = await captureCli(["see"], cwd);

    expect(state.kind).toBe("star");
    expect(state.tags).toContain("star");
    expect(output).toContain(`★ ${state.label}`);
    expect(output).toContain("[star]");
  });

  test("star with a state query marks the existing state instead of creating a new one", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");

    await runCli(["green"], cwd);
    const beforeRepo = loadRepo(cwd);
    const green = beforeRepo.states.at(-1)!;

    await runCli(["star", green.id], cwd);

    const repo = loadRepo(cwd);
    const starred = repo.states.find((state) => state.id === green.id)!;
    const output = await captureCli(["see"], cwd);

    expect(repo.states).toHaveLength(beforeRepo.states.length);
    expect(starred.kind).toBe("new");
    expect(starred.tags).toContain("star");
    expect(output).toContain(`★ ${starred.label}`);
  });

  test("stash captures the dirty workspace on a new branch without advancing the current branch", async () => {
    const trackedPath = join(cwd, "notes.txt");
    const untrackedPath = join(cwd, "scratch.txt");

    Bun.write(trackedPath, "green\n");
    await runCli(["green"], cwd);
    const green = loadRepo(cwd).states.at(-1)!;
    const beforeHead = run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout;
    const beforeBranch = run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout;

    Bun.write(trackedPath, "green\norange\n");
    Bun.write(untrackedPath, "temporary stash file\n");

    await runCli(["stash", "workspace backup"], cwd);

    const repo = loadRepo(cwd);
    const stashed = repo.states.at(-1)!;
    const stashBranch = stateDisplayBranch(stashed);

    expect(stashed.kind).toBe("stash");
    expect(stashed.tags).toContain("stash");
    expect(stashed.parentStateId).toBe(green.id);
    expect(stashed.metadata?.stashFromBranch).toBe("jjk/green");
    expect(stashed.metadata?.stashFromStateId).toBe(green.id);
    expect(stashBranch).toContain("jjk/stash_workspace_backup_");
    expect(run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout).toBe(beforeHead);
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(beforeBranch);
    expect(await Bun.file(trackedPath).text()).toBe("green\n");
    expect(existsSync(untrackedPath)).toBe(false);
    expect(run(["git", "show", `${stashed.commit}:notes.txt`], { cwd }).stdout.trim()).toBe("green\norange");
    expect(run(["git", "show", `${stashed.commit}:scratch.txt`], { cwd }).stdout.trim()).toBe("temporary stash file");
    expect(repo.currentStateHistory?.entries.at(-1)).toBe(green.id);
    expect(repo.lanes[repo.branchLaneMap[stashBranch] ?? ""]?.currentStateId).toBe(stashed.id);
    expect((await captureCli(["current"], cwd))).toContain(green.label);
  });
});
