import { describe, expect, test } from "bun:test";
import { renderGraph } from "../src/render";
import type { RepoData } from "../src/types";

describe("renderGraph", () => {
  test("marks current state and lane leaves", () => {
    const repo: RepoData = {
      version: 1,
      safeSpaceId: "safe1234",
      createdAt: "2026-03-22T00:00:00.000Z",
      updatedAt: "2026-03-22T00:00:00.000Z",
      settings: {
        watchDebounceMs: 1200,
        autoStatePrefix: "auto",
      },
      states: [
        {
          id: "root1111",
          kind: "save",
          label: "baseline",
          description: "baseline",
          createdAt: "2026-03-22T00:00:00.000Z",
          branch: "main",
          lane: "main",
          commit: "aaaa",
          parentCommit: null,
          parentStateId: null,
          tags: [],
          stats: { changedFiles: 1 },
        },
        {
          id: "leaf2222",
          kind: "step",
          label: "feature step",
          description: "feature step",
          createdAt: "2026-03-22T00:01:00.000Z",
          branch: "jjk/lane/feature",
          lane: "feature",
          commit: "bbbb",
          parentCommit: "aaaa",
          parentStateId: "root1111",
          tags: [],
          stats: { changedFiles: 1 },
        },
      ],
      lanes: {
        main: {
          name: "main",
          branch: "main",
          baseRef: "main",
          createdAt: "2026-03-22T00:00:00.000Z",
          updatedAt: "2026-03-22T00:00:00.000Z",
          currentStateId: "root1111",
        },
        feature: {
          name: "feature",
          branch: "jjk/lane/feature",
          baseRef: "main",
          createdAt: "2026-03-22T00:01:00.000Z",
          updatedAt: "2026-03-22T00:01:00.000Z",
          currentStateId: "leaf2222",
        },
      },
      branchLaneMap: {
        main: "main",
        "jjk/lane/feature": "feature",
      },
      timeshifts: [],
      freezes: [],
    };

    const output = renderGraph(repo, { currentStateId: "leaf2222" });
    expect(output).toContain("* current state    ^ lane leaf");
    expect(output).toContain("└─  ^ root1111 [save] baseline (main)");
    expect(output).toContain("└─ *^ leaf2222 [step] feature step (feature)");
  });
});
