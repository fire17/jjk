import { beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createLane, initSafeSpace, listLanes, resolveLane } from "../src/store";
import { run } from "../src/shell";

describe("lanes", () => {
  let cwd = "";

  beforeEach(() => {
    cwd = mkdtempSync(join(tmpdir(), "jjk-lane-"));
    initSafeSpace(cwd);
    run(["git", "config", "user.name", "jjk test"], { cwd });
    run(["git", "config", "user.email", "jjk@example.com"], { cwd });
  });

  test("createLane records and resolves a named lane", () => {
    const lane = createLane(cwd, "feature harvest");
    const lanes = listLanes(cwd);

    expect(lane.branch).toBe("jjk/lane/feature-harvest");
    expect(lanes.length).toBe(2);
    expect(resolveLane(cwd, "harvest")?.name).toBe("feature harvest");
  });
});
