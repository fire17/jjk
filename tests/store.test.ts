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
    });

    expect(result.state.description).toBe("baseline before parser rewrite");
    expect(run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout).toBe(
      result.state.commit,
    );
    expect(listStates(cwd)).toHaveLength(1);
  });
});
