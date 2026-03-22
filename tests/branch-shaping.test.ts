import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, loadRepo } from "../src/store";
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

describe("branch shaping commands", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-branch-shaping-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("move updates the selected state and branch target without creating a new state", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    await runCli(["green"], cwd);

    const before = loadRepo(cwd);
    const green = before.states.at(-1)!;

    await runCli(["move", green.id, "jjk/manual"], cwd);

    const repo = loadRepo(cwd);
    const moved = repo.states.find((state) => state.id === green.id)!;

    expect(repo.states).toHaveLength(before.states.length);
    expect(moved.branch).toBe("jjk/manual");
    expect(moved.lane).toBe("jjk/manual");
    expect(moved.continuationBranch).toBe("jjk/manual");
    expect(repo.branchLaneMap["jjk/manual"]).toBe("jjk/manual");
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/manual",
    );
  });

  test("note stores a human message on an existing state", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    await runCli(["green"], cwd);

    const repoBefore = loadRepo(cwd);
    const state = repoBefore.states.at(-1)!;
    await runCli(["note", `${state.id},`, "remember this"], cwd);

    const repo = loadRepo(cwd);
    const noted = repo.states.find((entry) => entry.id === state.id)!;
    const output = await captureCli(["see"], cwd);

    expect(noted.metadata?.message).toBe("remember this");
    expect(output).toContain("remember this");
  });

  test("pin and unpin toggle a marker on the current state", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    await runCli(["green"], cwd);

    await runCli(["pin"], cwd);
    let repo = loadRepo(cwd);
    let state = repo.states.at(-1)!;
    let output = await captureCli(["see"], cwd);

    expect(state.tags).toContain("pin");
    expect(output).toContain("📌");

    await runCli(["unpin"], cwd);
    repo = loadRepo(cwd);
    state = repo.states.at(-1)!;
    output = await captureCli(["see"], cwd);

    expect(state.tags).not.toContain("pin");
    expect(output).not.toContain("📌");
  });

  test("rename-state updates the state label and preserves previous label metadata", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    await runCli(["green"], cwd);

    const repoBefore = loadRepo(cwd);
    const state = repoBefore.states.at(-1)!;

    await runCli(["rename-state", state.id, "polished_green"], cwd);

    const repo = loadRepo(cwd);
    const renamed = repo.states.find((entry) => entry.id === state.id)!;

    expect(renamed.label).toBe("polished_green");
    expect(renamed.description).toBe("polished_green");
    expect(renamed.metadata?.priorLabels?.at(-1)).toEqual({
      label: "green",
      description: "green",
      updatedAt: renamed.metadata?.priorLabels?.at(-1)?.updatedAt,
    });
  });

  test("branch-from creates a new branch at a selected state without switching away", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    await runCli(["green"], cwd);

    const repoBefore = loadRepo(cwd);
    const green = repoBefore.states.at(-1)!;

    await runCli(["branch-from", green.id, "review_lane"], cwd);

    const repo = loadRepo(cwd);
    expect(repo.branchLaneMap["jjk/review_lane"]).toBe("jjk/review_lane");
    expect(repo.lanes["jjk/review_lane"]?.currentStateId).toBe(green.id);
    expect(run(["git", "rev-parse", "--verify", "refs/heads/jjk/review_lane"], { cwd }).stdout).toBe(
      green.commit,
    );
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/green",
    );
  });

  test("rename-branch moves branch metadata and the git ref together", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    await runCli(["green"], cwd);

    const repoBefore = loadRepo(cwd);
    const green = repoBefore.states.at(-1)!;

    await runCli(["rename-branch", "jjk/green", "jjk/green_experiment"], cwd);

    const repo = loadRepo(cwd);
    const renamed = repo.states.find((entry) => entry.id === green.id)!;

    expect(renamed.branch).toBe("jjk/green_experiment");
    expect(renamed.lane).toBe("jjk/green_experiment");
    expect(repo.branchLaneMap["jjk/green_experiment"]).toBe("jjk/green_experiment");
    expect(repo.branchLaneMap["jjk/green"]).toBeUndefined();
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/green_experiment",
    );
    expect(
      run(["git", "rev-parse", "--verify", "refs/heads/jjk/green_experiment"], { cwd }).stdout,
    ).toBe(green.commit);
  });
});
