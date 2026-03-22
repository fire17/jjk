import { existsSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline/promises";
import { stdin, stdout } from "node:process";
import {
  commandExists,
  createBundle,
  createOrSwitchBranch,
  fetchStateRefs,
  getAheadBehind,
  getCurrentBranch,
  getHeadCommit,
  getWorktreeStatus,
  hasDirtyWorktree,
  hasRemote,
  importIntoJj,
  isJjRepo,
  pickStateChanges,
  pullFastForward,
  pushCurrentBranchAndStateRefs,
  switchToDetachedCommit,
} from "./git";
import {
  renderDoctor,
  renderGraph,
  renderLanes,
  renderMap,
  renderStateSummary,
  renderStatus,
  renderStateTable,
  renderStory,
  renderTimeshifts,
} from "./render";
import {
  createLane,
  initSafeSpace,
  JJK_DIR,
  listLanes,
  listStates,
  listTimeshifts,
  loadRepo,
  promoteState,
  recordFreeze,
  rememberTimeshift,
  requireSafeSpace,
  resolveLane,
  resolveState,
  resolveTimeshift,
  saveState,
  saveRepo,
} from "./store";
import type { MapHit, SaveStateRequest, StateRecord } from "./types";
import { branchSegment, formatRelativePath, nowIso } from "./utils";
import { JJK_VERSION } from "./version";
import { runWatch } from "./watch";
import { runRepl } from "./repl";

function printHelp(): void {
  console.log(`jjk ${JJK_VERSION}

Safe spaces:
  jjk init
  jjk map
  jjk status
  jjk doctor

States:
  jjk <description>
  jjk step [description]
  jjk nice [description]
  jjk star [description]
  jjk see
  jjk story
  jjk diff [state] [state]
  jjk pick <state>
  jjk promote <state> <nice|star>
  jjk return <state>

Flow:
  jjk lane
  jjk lane <name>
  jjk watch
  jjk up
  jjk down
  jjk pull
  jjk freeze [state]
  jjk timeshift save [label]
  jjk timeshift restore <id>

Shell:
  jjk
`);
}

async function promptForState(states: StateRecord[]): Promise<StateRecord> {
  const rl = createInterface({ input: stdin, output: stdout });
  console.log("Multiple states matched:");
  states.forEach((state, index) => {
    console.log(`${index + 1}. ${renderStateSummary(state)}`);
  });
  const answer = await rl.question("Select a state number: ");
  rl.close();
  const index = Number.parseInt(answer, 10) - 1;
  if (!Number.isInteger(index) || index < 0 || index >= states.length) {
    throw new Error("Invalid selection.");
  }
  return states[index];
}

async function handleSave(root: string, request: SaveStateRequest): Promise<void> {
  const result = saveState(root, request);
  console.log(renderStateSummary(result.state));
}

function scanForMapHits(start: string, maxDepth = 4): MapHit[] {
  const hits: MapHit[] = [];

  function walk(current: string, depth: number): void {
    if (depth > maxDepth) {
      return;
    }

    let entries: string[] = [];
    try {
      entries = readdirSync(current);
    } catch {
      return;
    }

    const markers = [".git", ".jj", ".jjk", "package.json"].filter((entry) =>
      entries.includes(entry),
    );
    if (markers.length > 0) {
      hits.push({
        path: current,
        markers,
      });
    }

    for (const entry of entries) {
      if (entry.startsWith(".git") || entry === "node_modules" || entry === ".jj") {
        continue;
      }

      const next = join(current, entry);
      try {
        if (statSync(next).isDirectory()) {
          walk(next, depth + 1);
        }
      } catch {
        continue;
      }
    }
  }

  walk(start, 0);
  return hits;
}

async function handleReturn(root: string, query: string): Promise<void> {
  const repo = loadRepo(root);
  let state = resolveState(root, query);

  if (query.trim().length > 0) {
    const matches = repo.states.filter((candidate) => {
      const haystack = [
        candidate.id,
        candidate.label,
        candidate.description,
        candidate.branch,
        candidate.lane,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(query.trim().toLowerCase());
    });

    if (matches.length > 1 && process.stdin.isTTY) {
      state = await promptForState(matches.slice(0, 8));
    }
  }

  const worktree = getWorktreeStatus(root);
  const headCommit = getHeadCommit(root);
  let autoSaved = false;
  if (worktree.unstaged > 0 || worktree.untracked > 0 || (!headCommit && worktree.dirty)) {
    const backToLabel = `back to ${state.id} ${state.description}`.trim();
    saveState(root, {
      kind: "auto",
      description: backToLabel,
      label: backToLabel,
    });
    autoSaved = true;
  }

  switchToDetachedCommit(root, state.commit, {
    discardChanges: autoSaved,
  });
  const repoData = loadRepo(root);
  repoData.returnContext = {
    stateId: state.id,
    sourceBranch: state.branch,
    sourceLane: state.lane,
    branchPrefix: branchSegment(state.description),
  };
  saveRepo(root, repoData);
  importIntoJj(root);
  console.log(`returned to ${state.id}`);
}

export async function runCli(argv: string[], cwd: string): Promise<void> {
  const args = argv.slice();

  if (args.length === 0) {
    await runRepl(cwd);
    return;
  }

  const command = args[0];

  if (command === "version" || command === "--version" || command === "-v") {
    console.log(JJK_VERSION);
    return;
  }

  if (command === "help" || command === "--help" || command === "-h") {
    printHelp();
    return;
  }

  if (command === "init") {
    const { root, repo } = initSafeSpace(cwd);
    console.log(`safe space ready at ${root}`);
    console.log(`states: ${repo.states.length}`);
    return;
  }

  if (command === "map") {
    console.log(renderMap(scanForMapHits(cwd)));
    return;
  }

  const root = requireSafeSpace(cwd);

  switch (command) {
    case "step":
    case "nice":
    case "star":
      await handleSave(root, {
        kind: command,
        description: args.slice(1).join(" "),
      });
      return;
    case "see": {
      const repo = loadRepo(root);
      const headCommit = getHeadCommit(root);
      const currentState =
        repo.states.find((state) => state.commit === headCommit) ??
        (() => {
          const branch = getCurrentBranch(root);
          const laneName = repo.branchLaneMap[branch];
          return laneName ? repo.states.find((state) => state.id === repo.lanes[laneName]?.currentStateId) ?? null : null;
        })();
      console.log(renderGraph(repo, { currentStateId: currentState?.id ?? null }));
      console.log("");
      console.log(renderStateTable(listStates(root)));
      return;
    }
    case "story":
      console.log(renderStory(listStates(root)));
      return;
    case "status": {
      const repo = loadRepo(root);
      const branch = getCurrentBranch(root);
      const laneName = repo.branchLaneMap[branch];
      const latestState = repo.states.length > 0 ? repo.states[repo.states.length - 1] : null;
      console.log(
        renderStatus({
          root,
          branch,
          headCommit: getHeadCommit(root),
          lane: laneName ? repo.lanes[laneName] : null,
          worktree: getWorktreeStatus(root),
          latestState,
          stateCount: repo.states.length,
          jjAvailable: commandExists("jj", root) && isJjRepo(root),
          remoteConfigured: hasRemote(root),
          aheadBehind: getAheadBehind(root),
        }),
      );
      return;
    }
    case "diff": {
      const queryA = args[1] ?? "";
      const queryB = args[2] ?? "";

      if (queryA.length === 0) {
        const repo = loadRepo(root);
        const branch = getCurrentBranch(root);
        const laneName = repo.branchLaneMap[branch];
        const lane = laneName ? repo.lanes[laneName] : null;
        const currentState =
          lane?.currentStateId
            ? resolveState(root, lane.currentStateId)
            : repo.states[repo.states.length - 1];
        if (!currentState) {
          throw new Error("No state is available to diff against.");
        }
        const result = Bun.spawnSync(
          ["git", "diff", "--stat", currentState.commit],
          {
            cwd: root,
            stdout: "pipe",
            stderr: "pipe",
          },
        );
        const output = result.stdout.toString().trim();
        console.log(output.length > 0 ? output : "No diff against the latest saved state.");
        return;
      }

      const stateA = resolveState(root, queryA);
      if (queryB.length === 0) {
        const result = Bun.spawnSync(
          ["git", "diff", "--stat", stateA.commit],
          {
            cwd: root,
            stdout: "pipe",
            stderr: "pipe",
          },
        );
        const output = result.stdout.toString().trim();
        console.log(output.length > 0 ? output : "No diff against the selected state.");
        return;
      }

      const stateB = resolveState(root, queryB);
      const result = Bun.spawnSync(
        ["git", "diff", "--stat", stateA.commit, stateB.commit],
        {
          cwd: root,
          stdout: "pipe",
          stderr: "pipe",
        },
      );
      const output = result.stdout.toString().trim();
      console.log(output.length > 0 ? output : "No diff between the selected states.");
      return;
    }
    case "pick": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Provide a state to pick.");
      }

      const state = resolveState(root, query);
      if (hasDirtyWorktree(root)) {
        saveState(root, {
          kind: "auto",
          description: `auto pre-pick checkpoint before ${state.id}`,
        });
      }

      const logicalParentCommit = state.parentStateId
        ? resolveState(root, state.parentStateId).commit
        : state.parentCommit;
      const applied = pickStateChanges(root, logicalParentCommit, state.commit);
      if (!applied) {
        console.log(`pick produced no changes for ${state.id}`);
        return;
      }

      const picked = saveState(root, {
        kind: "step",
        description: `picked ${state.id} ${state.label}`,
      });
      console.log(`picked ${state.id} onto ${getCurrentBranch(root)}`);
      console.log(renderStateSummary(picked.state));
      return;
    }
    case "promote": {
      const query = args[1] ?? "";
      const targetKind = args[2] as "nice" | "star" | undefined;
      const description = args.slice(3).join(" ").trim();

      if (query.length === 0 || !targetKind) {
        throw new Error("Usage: jjk promote <state> <nice|star> [description]");
      }

      if (targetKind !== "nice" && targetKind !== "star") {
        throw new Error("Promotion kind must be `nice` or `star`.");
      }

      const state = resolveState(root, query);
      const promoted = promoteState(root, state.id, targetKind, description);
      console.log(`promoted ${state.id} to ${targetKind}`);
      console.log(renderStateSummary(promoted));
      return;
    }
    case "return":
      await handleReturn(root, args.slice(1).join(" "));
      return;
    case "watch": {
      const repo = loadRepo(root);
      await runWatch(root, repo.settings.watchDebounceMs);
      return;
    }
    case "up":
      pushCurrentBranchAndStateRefs(root);
      console.log("pushed current branch and jjk state refs");
      return;
    case "down":
    case "pull":
      fetchStateRefs(root);
      pullFastForward(root);
      console.log("fetched jjk state refs and pulled fast-forward where possible");
      return;
    case "lane": {
      const name = args.slice(1).join(" ").trim();
      if (name.length === 0) {
        console.log(renderLanes(listLanes(root), getCurrentBranch(root)));
        return;
      }

      const existing = resolveLane(root, name);
      if (existing) {
        createOrSwitchBranch(root, existing.branch);
        console.log(`lane switched: ${existing.name} (${existing.branch})`);
        return;
      }

      const lane = createLane(root, name);
      console.log(`lane ready: ${lane.name} (${lane.branch})`);
      return;
    }
    case "doctor": {
      const repo = loadRepo(root);
      const branch = getCurrentBranch(root);
      const laneName = repo.branchLaneMap[branch];
      console.log(
        renderDoctor({
          root,
          branch,
          jjAvailable: commandExists("jj", root) && isJjRepo(root),
          lane: laneName ? repo.lanes[laneName] : null,
          stateCount: repo.states.length,
          remoteConfigured: hasRemote(root),
        }),
      );
      return;
    }
    case "freeze": {
      const state = resolveState(root, args.slice(1).join(" "));
      const record = recordFreeze(root, state.id);
      const bundlePath = join(root, record.bundlePath);
      mkdirSync(dirname(bundlePath), { recursive: true });
      createBundle(root, bundlePath, `refs/jjk/states/${state.id}`);
      Bun.write(
        join(root, record.manifestPath),
        `${JSON.stringify(
          {
            id: record.id,
            state,
            createdAt: record.createdAt,
            generatedAt: nowIso(),
          },
          null,
          2,
        )}\n`,
      );
      console.log(`freeze created: ${formatRelativePath(root, bundlePath)}`);
      return;
    }
    case "timeshift": {
      const subcommand = args[1] ?? "list";
      if (subcommand === "save") {
        const label = args.slice(2).join(" ").trim() || "timeshift";
        const record = rememberTimeshift(root, label);
        console.log(`timeshift saved: ${record.id} ${record.label}`);
        return;
      }

      if (subcommand === "restore") {
        const record = resolveTimeshift(root, args.slice(2).join(" "));
        if (hasDirtyWorktree(root)) {
          saveState(root, {
            kind: "auto",
            description: `auto pre-timeshift checkpoint before ${record.id}`,
          });
        }
        if (record.stateId) {
          const state = resolveState(root, record.stateId);
          createOrSwitchBranch(root, record.branch, state.commit, {
            force: true,
            reset: true,
          });
          console.log(`timeshift restored to ${record.branch} at ${state.id}`);
        } else {
          createOrSwitchBranch(root, record.branch, undefined, {
            force: true,
            reset: true,
          });
          console.log(`timeshift restored to branch ${record.branch}`);
        }
        console.log(`saved cwd: ${record.relativeCwd}`);
        return;
      }

      console.log(renderTimeshifts(listTimeshifts(root)));
      return;
    }
    default:
      await handleSave(root, {
        kind: "save",
        description: args.join(" "),
      });
  }
}
