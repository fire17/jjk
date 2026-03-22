import { beforeEach, describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync } from "node:fs";
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

describe("patch, backup, replay, and restore commands", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-patch-backup-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("amend updates the current state in place and keeps branch/tooling aligned", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);

    const before = loadRepo(cwd).states.at(-1)!;

    Bun.write(filePath, "green\namended\n");
    const output = await captureCli(["amend", "refined"], cwd);

    const repo = loadRepo(cwd);
    const amended = repo.states.find((state) => state.id === before.id);

    expect(output).toContain("amended");
    expect(amended?.id).toBe(before.id);
    expect(amended?.description).toBe("refined");
    expect(amended?.commit).not.toBe(before.commit);
    expect(repo.states.filter((state) => state.id === before.id)).toHaveLength(1);
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "jjk/green",
    );
    expect(run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout).toBe(amended?.commit);
  });

  test("show --atomic-chain, files, and touched expose the selected branch lineage", async () => {
    const filePath = join(cwd, "story.txt");
    Bun.write(filePath, "base\n");
    await runCli(["green"], cwd);

    Bun.write(filePath, "base\nchapter two\n");
    await runCli(["save", "chapter_two"], cwd);

    const repo = loadRepo(cwd);
    const chapterTwo = repo.states.find((state) => state.description === "chapter_two")!;

    const chain = await captureCli(["show", "--atomic-chain", chapterTwo.id], cwd);
    expect(chain).toContain("1/3");
    expect(chain).toContain("2/3");
    expect(chain).toContain("3/3");
    expect(chain).toContain("chapter_two");

    const files = await captureCli(["files", chapterTwo.id], cwd);
    expect(files).toContain("story.txt");

    const touched = await captureCli(["touched", "jjk/green"], cwd);
    expect(touched).toContain("story.txt");
  });

  test("backups lists saved files and snapshot-log shows the current snapshot history", async () => {
    Bun.write(join(cwd, "notes.txt"), "green\n");
    await runCli(["green"], cwd);
    await runCli(["amend", "green-refined"], cwd);

    await runCli(["backup", "before-cycle"], cwd);
    const backups = await captureCli(["backups"], cwd);
    const snapshots = await captureCli(["snapshot-log"], cwd);

    expect(backups).toContain("before-cycle.json");
    expect(backups).toContain("modified");
    expect(snapshots).toContain("amend:");
    expect(snapshots).toContain("*");
  });

  test("restore --preview does not mutate state and import/export round-trip a snapshot", async () => {
    const filePath = join(cwd, "notes.txt");
    const exportPath = join(cwd, "exports", "purple-snapshot.json");

    Bun.write(filePath, "green\n");
    await runCli(["green"], cwd);
    const green = loadRepo(cwd).states.at(-1)!;

    Bun.write(filePath, "purple\n");
    await runCli(["save", "purple"], cwd);
    const purple = loadRepo(cwd).states.at(-1)!;

    await runCli(["backup", "purple"], cwd);
    const preview = await captureCli(["restore", "--preview", "purple"], cwd);
    expect(preview).toContain("backup preview:");
    expect(preview).toContain("current branch:");
    expect(loadRepo(cwd).states.at(-1)?.id).toBe(purple.id);

    const exported = await captureCli(["export", purple.id, "exports/purple-snapshot"], cwd);
    expect(exported).toContain("exported");
    expect(existsSync(exportPath)).toBe(true);

    Bun.write(filePath, "orange\n");
    await runCli(["save", "orange"], cwd);
    expect(loadRepo(cwd).states.at(-1)?.description).toBe("orange");

    const imported = await captureCli(["import", "exports/purple-snapshot.json"], cwd);
    expect(imported).toContain("imported backup:");

    const repoAfterImport = loadRepo(cwd);
    expect(repoAfterImport.states.find((state) => state.description === "orange")).toBeUndefined();
    expect(run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout).toBe(purple.commit);
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      purple.branch,
    );
    expect(repoAfterImport.states.find((state) => state.id === green.id)).toBeDefined();
  });

  test("replay and merge-state create the expected branch results", { timeout: 10000 }, async () => {
    const filePath = join(cwd, "story.txt");
    Bun.write(filePath, "base\n");
    await runCli(["save", "main_base"], cwd);
    const mainBase = loadRepo(cwd).states.at(-1)!;

    await runCli(["green"], cwd);
    const green = loadRepo(cwd).states.at(-1)!;

    Bun.write(filePath, "base\nsource replay\n");
    await runCli(["save", "source_replay"], cwd);
    const sourceReplay = loadRepo(cwd).states.at(-1)!;

    const replayOutput = await captureCli(["replay", sourceReplay.id, "onto", "main"], cwd);
    expect(replayOutput).toContain("replay");
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "main",
    );

    const replayState = loadRepo(cwd).states.at(-1)!;
    expect(replayState.kind).toBe("cherry");
    expect(replayState.metadata?.base).toBe(mainBase.id);
    expect(replayState.metadata?.cherry).toBe(sourceReplay.id);

    await runCli(["return", green.id], cwd);
    const mergePath = join(cwd, "merge.txt");
    Bun.write(mergePath, "merge payload\n");
    await runCli(["save", "source_merge"], cwd);
    const sourceMerge = loadRepo(cwd).states.at(-1)!;

    const mergeOutput = await captureCli(["merge-state", sourceMerge.id, "into", "main"], cwd);
    expect(mergeOutput).toContain("merge-state");
    expect(run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], { cwd }).stdout).toBe(
      "main",
    );
    const merged = loadRepo(cwd).states.at(-1)!;
    expect(merged.kind).toBe("cherry");
    expect(merged.metadata?.base).toBe(replayState.id);
    expect(merged.metadata?.cherry).toBe(sourceMerge.id);
  });

  test("revert-state records the reverted source as metadata", async () => {
    const filePath = join(cwd, "story.txt");
    Bun.write(filePath, "base\n");
    await runCli(["green"], cwd);

    Bun.write(filePath, "base\nrevert payload\n");
    await runCli(["save", "source_revert"], cwd);
    const sourceRevert = loadRepo(cwd).states.at(-1)!;

    const revertOutput = await captureCli(["revert-state", sourceRevert.id], cwd);
    expect(revertOutput).toContain("reverted");
    const reverted = loadRepo(cwd).states.at(-1)!;
    expect(reverted.kind).toBe("save");
    expect(reverted.metadata?.base).toBe(sourceRevert.id);
    expect(reverted.metadata?.cherry).toBe(sourceRevert.id);
  });
});
