import { describe, expect, test } from "bun:test";
import { renderGraph, renderStateChoiceTable, renderStateTable } from "../src/render";
import type { RepoData } from "../src/types";

describe("renderGraph", () => {
  test("marks current state and branch leaves", () => {
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
    expect(output).toContain("* current state    ^ branch leaf");
    expect(output).toContain("└─  ^ root1111 [save] baseline (main)");
    expect(output).toContain("└─ *^ leaf2222 [step] feature step (jjk/lane/feature)");
  });

  test("can colorize output by branch", () => {
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
          id: "main1111",
          kind: "save",
          label: "save main",
          description: "main",
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
          id: "green222",
          kind: "save",
          label: "save green",
          description: "green",
          createdAt: "2026-03-22T00:01:00.000Z",
          branch: "jjk/green",
          continuationBranch: "jjk/green",
          lane: "jjk/green",
          commit: "bbbb",
          parentCommit: "aaaa",
          parentStateId: "main1111",
          tags: [],
          stats: { changedFiles: 1 },
        },
        {
          id: "green333",
          kind: "save",
          label: "save brighter green",
          description: "brighter green",
          createdAt: "2026-03-22T00:02:00.000Z",
          branch: "jjk/green",
          continuationBranch: "jjk/green",
          lane: "jjk/green",
          commit: "cccc",
          parentCommit: "bbbb",
          parentStateId: "green222",
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
          currentStateId: "main1111",
        },
        green: {
          name: "green",
          branch: "jjk/green",
          baseRef: "main",
          createdAt: "2026-03-22T00:01:00.000Z",
          updatedAt: "2026-03-22T00:02:00.000Z",
          currentStateId: "green333",
        },
      },
      branchLaneMap: {
        main: "main",
        "jjk/green": "green",
      },
      timeshifts: [],
      freezes: [],
    };

    const graph = renderGraph(repo, { currentStateId: "green333", colorize: true });
    const table = renderStateTable(repo.states, { colorize: true, currentStateId: "green333" });

    expect(graph).toContain("\u001b[38;5;");
    expect(table).toContain("\u001b[38;5;");
    expect(graph).toContain("\u001b[0m");
    expect(table).toContain("\u001b[0m");
    expect(graph).toContain("\u001b[2m");
    expect(table).toContain("\u001b[2m");
    expect(graph).toContain("\u001b[1m");
    expect(table).toContain("\u001b[1m");
    expect(graph).toContain("green222");
    expect(graph).toContain("green333");
  });

  test("renders state choices as an aligned table", () => {
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
          id: "ff698b81",
          kind: "save",
          label: "purple",
          description: "purple",
          createdAt: "2026-03-22T05:16:00.000Z",
          branch: "jjk/purple",
          continuationBranch: "jjk/purple",
          lane: "main",
          commit: "aaaa",
          parentCommit: null,
          parentStateId: null,
          tags: [],
          stats: { changedFiles: 1 },
        },
        {
          id: "6ef57e58",
          kind: "save",
          label: "fast_purple",
          description: "fast_purple",
          createdAt: "2026-03-22T05:16:00.000Z",
          branch: "jjk/purple",
          continuationBranch: "jjk/purple",
          lane: "jjk/purple",
          commit: "bbbb",
          parentCommit: "aaaa",
          parentStateId: "ff698b81",
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
          currentStateId: null,
        },
        purple: {
          name: "jjk/purple",
          branch: "jjk/purple",
          baseRef: "main",
          createdAt: "2026-03-22T05:16:00.000Z",
          updatedAt: "2026-03-22T05:16:00.000Z",
          currentStateId: "6ef57e58",
        },
      },
      branchLaneMap: {
        main: "main",
        "jjk/purple": "purple",
      },
      timeshifts: [],
      freezes: [],
    };

    const output = renderStateChoiceTable(repo.states);
    expect(output).toContain("#   id");
    expect(output).toContain("1   ff698b81");
    expect(output).toContain("2   6ef57e58");
    expect(output).toContain("jjk/purple");
  });
});
