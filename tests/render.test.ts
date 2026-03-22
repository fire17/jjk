import { describe, expect, test } from "bun:test";
import { renderGraph, renderStateChoiceTable, renderStateSummary, renderStateTable } from "../src/render";
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
          commit: "bbbbbbbb1234",
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
          commit: "cccccccc5678",
          parentCommit: "bbbbbbbb1234",
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
    expect(table).toContain("git");
    expect(table).toContain("bbbbbbbb");
    expect(table).toContain("cccccccc");
    expect(table).not.toContain("bbbbbbbb1234");
    expect(table).not.toContain("cccccccc5678");
  });

  test("uses a diverse set of stable branch colors", () => {
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
          label: "main",
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
          id: "alpha111",
          kind: "save",
          label: "alpha",
          description: "alpha",
          createdAt: "2026-03-22T00:01:00.000Z",
          branch: "jjk/alpha-spectrum",
          continuationBranch: "jjk/alpha-spectrum",
          lane: "alpha",
          commit: "bbbb",
          parentCommit: "aaaa",
          parentStateId: "main1111",
          tags: [],
          stats: { changedFiles: 1 },
        },
        {
          id: "beta2222",
          kind: "save",
          label: "beta",
          description: "beta",
          createdAt: "2026-03-22T00:02:00.000Z",
          branch: "jjk/beta-spectrum",
          continuationBranch: "jjk/beta-spectrum",
          lane: "beta",
          commit: "cccc",
          parentCommit: "bbbb",
          parentStateId: "alpha111",
          tags: [],
          stats: { changedFiles: 1 },
        },
        {
          id: "gamma333",
          kind: "save",
          label: "gamma",
          description: "gamma",
          createdAt: "2026-03-22T00:03:00.000Z",
          branch: "jjk/gamma-spectrum",
          continuationBranch: "jjk/gamma-spectrum",
          lane: "gamma",
          commit: "dddd",
          parentCommit: "cccc",
          parentStateId: "beta2222",
          tags: [],
          stats: { changedFiles: 1 },
        },
        {
          id: "delta444",
          kind: "save",
          label: "delta",
          description: "delta",
          createdAt: "2026-03-22T00:04:00.000Z",
          branch: "jjk/delta-spectrum",
          continuationBranch: "jjk/delta-spectrum",
          lane: "delta",
          commit: "eeee",
          parentCommit: "dddd",
          parentStateId: "gamma333",
          tags: [],
          stats: { changedFiles: 1 },
        },
        {
          id: "omega555",
          kind: "save",
          label: "omega",
          description: "omega",
          createdAt: "2026-03-22T00:05:00.000Z",
          branch: "jjk/omega-spectrum",
          continuationBranch: "jjk/omega-spectrum",
          lane: "omega",
          commit: "ffff",
          parentCommit: "eeee",
          parentStateId: "delta444",
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
        alpha: {
          name: "alpha",
          branch: "jjk/alpha-spectrum",
          baseRef: "main",
          createdAt: "2026-03-22T00:01:00.000Z",
          updatedAt: "2026-03-22T00:01:00.000Z",
          currentStateId: "alpha111",
        },
        beta: {
          name: "beta",
          branch: "jjk/beta-spectrum",
          baseRef: "main",
          createdAt: "2026-03-22T00:02:00.000Z",
          updatedAt: "2026-03-22T00:02:00.000Z",
          currentStateId: "beta2222",
        },
        gamma: {
          name: "gamma",
          branch: "jjk/gamma-spectrum",
          baseRef: "main",
          createdAt: "2026-03-22T00:03:00.000Z",
          updatedAt: "2026-03-22T00:03:00.000Z",
          currentStateId: "gamma333",
        },
        delta: {
          name: "delta",
          branch: "jjk/delta-spectrum",
          baseRef: "main",
          createdAt: "2026-03-22T00:04:00.000Z",
          updatedAt: "2026-03-22T00:04:00.000Z",
          currentStateId: "delta444",
        },
        omega: {
          name: "omega",
          branch: "jjk/omega-spectrum",
          baseRef: "main",
          createdAt: "2026-03-22T00:05:00.000Z",
          updatedAt: "2026-03-22T00:05:00.000Z",
          currentStateId: "omega555",
        },
      },
      branchLaneMap: {
        main: "main",
        "jjk/alpha-spectrum": "alpha",
        "jjk/beta-spectrum": "beta",
        "jjk/gamma-spectrum": "gamma",
        "jjk/delta-spectrum": "delta",
        "jjk/omega-spectrum": "omega",
      },
      timeshifts: [],
      freezes: [],
    };

    const table = renderStateTable(repo.states, { colorize: true });
    const colors = Array.from(table.matchAll(/\u001b\[38;5;(\d+)m/g), (match) => match[1]);

    expect(new Set(colors).size).toBeGreaterThanOrEqual(5);
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

  test("can colorize state choices for fuzzy return selection", () => {
    const states: RepoData["states"] = [
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
        label: "orange",
        description: "orange",
        createdAt: "2026-03-22T05:17:00.000Z",
        branch: "jjk/orange",
        continuationBranch: "jjk/orange",
        lane: "main",
        commit: "bbbb",
        parentCommit: "aaaa",
        parentStateId: "ff698b81",
        tags: [],
        stats: { changedFiles: 1 },
      },
    ];

    const output = renderStateChoiceTable(states, { colorize: true });

    expect(output).toContain("\u001b[38;5;");
    expect(output).toContain("\u001b[0m");
    expect(output).not.toContain("\u001b[48;");
  });

  test("renders git ids in state summaries", () => {
    const text = renderStateSummary({
      id: "ff698b81120d",
      kind: "save",
      label: "purple",
      description: "purple",
      createdAt: "2026-03-22T05:16:00.000Z",
      branch: "jjk/purple",
      continuationBranch: "jjk/purple",
      lane: "main",
      commit: "1234567890abcdef1234567890abcdef12345678",
      parentCommit: null,
      parentStateId: null,
      tags: [],
      stats: { changedFiles: 1 },
      metadata: {
        gitCommit: "1234567890abcdef1234567890abcdef12345678",
      },
    });

    expect(text).toContain("ff698b81");
    expect(text).not.toContain("ff698b81120d");
    expect(text).toContain("git=1234567890ab");
  });
});
