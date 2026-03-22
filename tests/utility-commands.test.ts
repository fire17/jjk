import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/commands";
import { initSafeSpace, loadRepo } from "../src/store";
import { run } from "../src/shell";
import { renderStateSummary } from "../src/render";

describe("utility commands", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-utils-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("default-branch, aliases, and config persist repo settings", async () => {
    await runCli(["green"], cwd);
    await runCli(["default-branch", "jjk/green"], cwd);
    await runCli(["aliases", "add", "focus", "green"], cwd);

    const output: string[] = [];
    const originalLog = console.log;
    console.log = (...args: unknown[]) => {
      output.push(args.join(" "));
    };
    try {
      await runCli(["config"], cwd);
      await runCli(["copy-id", "focus"], cwd);
    } finally {
      console.log = originalLog;
    }

    const repo = loadRepo(cwd);
    expect(repo.settings.defaultBranch).toBe("jjk/green");
    expect(repo.settings.aliases?.focus).toBe("green");
    expect(output.join("\n")).toContain("default branch: jjk/green");
    expect(output.join("\n")).toContain(repo.states.find((state) => state.description === "green")?.id ?? "");
  });

  test("mark-style commands update metadata and render in summaries", async () => {
    await runCli(["green"], cwd);
    const repoBefore = loadRepo(cwd);
    const green = repoBefore.states.find((state) => state.description === "green")!;

    await runCli(["mark", green.id, "blocked"], cwd);
    await runCli(["assign-note", `${green.id}, @alice/review parser output`], cwd);
    await runCli(["ready", green.id], cwd);
    await runCli(["publish", green.id], cwd);
    await runCli(["handoff", `${green.id}, finalize parser cleanup`], cwd);
    await runCli(["quarantine", green.id], cwd);

    const repo = loadRepo(cwd);
    const updated = repo.states.find((state) => state.id === green.id)!;
    const summary = renderStateSummary(updated);

    expect(updated.metadata?.status).toBe("quarantined");
    expect(updated.metadata?.assignee).toBe("@alice");
    expect(updated.metadata?.note).toContain("review parser output");
    expect(updated.metadata?.handoff).toContain("finalize parser cleanup");
    expect(updated.metadata?.publishedAt).toBeDefined();
    expect(updated.metadata?.quarantinedAt).toBeDefined();
    expect(summary).toContain("status=quarantined");
    expect(summary).toContain("assignee=@alice");
    expect(summary).toContain("note=review parser output");
  });

  test("open and recent surface useful state information", async () => {
    const filePath = join(cwd, "notes.txt");
    Bun.write(filePath, "alpha\n");
    await runCli(["save", "baseline"], cwd);
    Bun.write(filePath, "alpha\nbeta\n");
    await runCli(["save", "second"], cwd);

    const output: string[] = [];
    const originalLog = console.log;
    console.log = (...args: unknown[]) => {
      output.push(args.join(" "));
    };
    try {
      await runCli(["recent"], cwd);
      await runCli(["open", "second"], cwd);
    } finally {
      console.log = originalLog;
    }

    expect(output.join("\n")).toContain("baseline");
    expect(output.join("\n")).toContain("second");
    expect(output.join("\n")).toContain("notes.txt");
  });

  test("branch locks block state-changing saves until unlocked", async () => {
    await runCli(["green"], cwd);
    await runCli(["lock", "jjk/green"], cwd);

    await expect(runCli(["save", "locked attempt"], cwd)).rejects.toThrow(
      /Branch `jjk\/green` is locked/,
    );

    await runCli(["unlock", "jjk/green"], cwd);
    await runCli(["save", "unlocked attempt"], cwd);

    const repo = loadRepo(cwd);
    expect(repo.states.some((state) => state.description === "unlocked attempt")).toBe(true);
  });

  test("archive hides a state through the existing deleted-state path", async () => {
    await runCli(["green"], cwd);
    const repoBefore = loadRepo(cwd);
    const green = repoBefore.states.find((state) => state.description === "green")!;

    await runCli(["archive", green.id], cwd);

    const repo = loadRepo(cwd);
    const archived = repo.states.find((state) => state.id === green.id)!;
    expect(archived.metadata?.deletedAt).toBeDefined();
    expect(archived.branch).toBe("deleted/green");
    expect(archived.metadata?.deletedBranch).toBe("deleted/green");
  });
});
