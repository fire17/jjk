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
  getCurrentBranchName,
  getHeadCommit,
  getWorktreeStatus,
  hasDirtyWorktree,
  hasRemote,
  importIntoJj,
  isJjRepo,
  pickStateChanges,
  pullFastForward,
  pruneJjKeepRefs,
  pushCurrentBranchAndStateRefs,
  switchToDetachedCommit,
  worktreeMatchesCommit,
} from "./git";
import {
  renderDoctor,
  renderGraph,
  renderCurrentState,
  renderLanes,
  renderMap,
  renderStateChoiceTable,
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
  updateBranchTarget,
  saveState,
  saveRepo,
  isTipStateOnBranch,
} from "./store";
import type { MapHit, RepoData, SaveStateRequest, StateRecord } from "./types";
import {
  branchSegment,
  continuationBranchName,
  formatRelativePath,
  nowIso,
  parseStateLabelAndMessage,
  shortStateId,
  stateDisplayBranch,
} from "./utils";
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
  jjk current
  jjk <description>
  jjk save [description]
  jjk step [description]
  jjk nice [description]
  jjk star [description]
  jjk see
  jjk story
  jjk diff [state] [state]
  jjk pick <state>
  jjk promote <state> <nice|star>
  jjk return <state>
  jjk return -
  jjk back
  jjk forward
  jjk up
  jjk down
  jjk update <branch> [state]

Flow:
  jjk lane
  jjk lane <name>
  jjk watch
  jjk push
  jjk pull
  jjk freeze [state]
  jjk timeshift save [label]
  jjk timeshift restore <id>
  jjk snapshots <on|off>

Shell:
  jjk
`);
}

async function promptForState(states: StateRecord[]): Promise<StateRecord> {
  const rl = createInterface({ input: stdin, output: stdout });
  console.log("Multiple states matched:");
  console.log(renderStateChoiceTable(states, { colorize: shouldColorizeOutput() }));
  const answer = await rl.question("Select a state number: ");
  rl.close();
  const index = Number.parseInt(answer, 10) - 1;
  if (!Number.isInteger(index) || index < 0 || index >= states.length) {
    throw new Error("Invalid selection.");
  }
  return states[index];
}

async function handleSave(
  root: string,
  request: SaveStateRequest,
  options?: {
    allowMainBranchSave?: boolean;
    continuationBranch?: string | null;
    suppressReturnBranchFork?: boolean;
  },
): Promise<void> {
  if (request.kind === "new") {
    const description = request.description.trim();
    if (description.length === 0) {
      throw new Error("Provide a description for the new branch state.");
    }
    createOrSwitchBranch(root, continuationBranchName(description), getHeadCommit(root) ?? undefined);
  }

  const result = saveState(root, request, options);
  syncCurrentStateHistory(root, resolveCurrentState(root, loadRepo(root).states)?.id ?? null);
  console.log(renderStateSummary(result.state));
}

function buildSaveRequest(kind: SaveStateRequest["kind"], input: string): SaveStateRequest {
  return {
    kind,
    ...parseStateLabelAndMessage(input),
  };
}

function shouldColorizeOutput(): boolean {
  return Boolean(process.stdout.isTTY) && process.env.NO_COLOR === undefined;
}

function stateMatchesWorkspace(
  state: StateRecord,
  branchName: string | null,
  headCommit: string | null,
): boolean {
  if (!headCommit || state.commit !== headCommit) {
    return false;
  }

  if (branchName === null) {
    return true;
  }

  return stateDisplayBranch(state) === branchName;
}

function resolveCurrentState(root: string, states: StateRecord[]): StateRecord | null {
  const branchName = getCurrentBranchName(root);
  const headCommit = getHeadCommit(root);
  const repo = loadRepo(root);
  const historyStateId = getCurrentStateHistoryEntry(repo);

  if (historyStateId) {
    const historyState = states.find((state) => state.id === historyStateId) ?? null;
    if (historyState && stateMatchesWorkspace(historyState, branchName, headCommit)) {
      return historyState;
    }
  }

  if (headCommit) {
    for (let index = states.length - 1; index >= 0; index -= 1) {
      const state = states[index];
      if (state && stateMatchesWorkspace(state, branchName, headCommit)) {
        return state;
      }
    }

    for (let index = states.length - 1; index >= 0; index -= 1) {
      const state = states[index];
      if (state && state.commit === headCommit) {
        return state;
      }
    }
  }

  if (historyStateId) {
    return states.find((state) => state.id === historyStateId) ?? null;
  }

  const branch = branchName ?? getCurrentBranch(root);
  const laneName = repo.branchLaneMap[branch];
  return laneName ? states.find((state) => state.id === repo.lanes[laneName]?.currentStateId) ?? null : null;
}

function getCurrentStateHistoryEntry(repo: RepoData): string | null {
  const history = repo.currentStateHistory;
  if (!history || history.index < 0 || history.index >= history.entries.length) {
    return null;
  }
  return history.entries[history.index] ?? null;
}

function syncCurrentStateHistory(root: string, currentStateId: string | null): RepoData {
  const repo = loadRepo(root);
  const history = repo.currentStateHistory ?? { entries: [], index: -1 };
  const activeStateId = getCurrentStateHistoryEntry(repo);

  if (currentStateId && activeStateId !== currentStateId) {
    history.entries = history.entries.slice(0, history.index + 1);
    history.entries.push(currentStateId);
    history.index = history.entries.length - 1;
    repo.currentStateHistory = history;
    saveRepo(root, repo);
    return repo;
  }

  if (!repo.currentStateHistory) {
    repo.currentStateHistory = history;
    saveRepo(root, repo);
  }

  return repo;
}

function recordStateVisit(root: string, stateId: string): void {
  const repo = loadRepo(root);
  const history = repo.currentStateHistory ?? { entries: [], index: -1 };
  const activeStateId = getCurrentStateHistoryEntry(repo);
  if (activeStateId === stateId) {
    return;
  }

  history.entries = history.entries.slice(0, history.index + 1);
  history.entries.push(stateId);
  history.index = history.entries.length - 1;
  repo.currentStateHistory = history;
  saveRepo(root, repo);
}

function moveStateHistoryIndex(root: string, index: number): void {
  const repo = loadRepo(root);
  const history = repo.currentStateHistory ?? { entries: [], index: -1 };
  if (index < 0 || index >= history.entries.length) {
    throw new Error("State history is out of range.");
  }
  history.index = index;
  repo.currentStateHistory = history;
  saveRepo(root, repo);
}

function findStateById(repo: RepoData, stateId: string): StateRecord {
  const state = repo.states.find((candidate) => candidate.id === stateId);
  if (!state) {
    throw new Error(`No state matched \`${stateId}\`.`);
  }
  return state;
}

function resolveHistoryState(root: string, offset: -1 | 1): {
  state: StateRecord;
  index: number;
} {
  const currentState = resolveCurrentState(root, loadRepo(root).states);
  const repo = syncCurrentStateHistory(root, currentState?.id ?? null);
  const history = repo.currentStateHistory ?? { entries: [], index: -1 };
  const nextIndex = history.index + offset;

  if (nextIndex < 0 || nextIndex >= history.entries.length) {
    throw new Error(offset < 0 ? "No earlier state is available." : "No later state is available.");
  }

  return {
    state: findStateById(repo, history.entries[nextIndex] ?? ""),
    index: nextIndex,
  };
}

function resolvePreviousVisitedState(root: string): StateRecord {
  const currentState = resolveCurrentState(root, loadRepo(root).states);
  const repo = syncCurrentStateHistory(root, currentState?.id ?? null);
  const history = repo.currentStateHistory ?? { entries: [], index: -1 };
  const previousId = history.index > 0 ? history.entries[history.index - 1] : null;

  if (!previousId) {
    throw new Error("No previously visited state is available.");
  }

  return findStateById(repo, previousId);
}

async function resolveChildState(root: string): Promise<StateRecord> {
  const repo = loadRepo(root);
  const currentState = resolveCurrentState(root, repo.states);
  if (!currentState) {
    throw new Error("No current state is available.");
  }

  const children = repo.states.filter((state) => state.parentStateId === currentState.id);
  if (children.length === 0) {
    throw new Error(`No child states are available from ${shortStateId(currentState.id)}.`);
  }

  const historyRepo = syncCurrentStateHistory(root, currentState.id);
  const history = historyRepo.currentStateHistory ?? { entries: [], index: -1 };
  const forwardId = history.entries[history.index + 1] ?? null;
  const preferred = forwardId
    ? children.find((state) => state.id === forwardId) ?? null
    : null;
  if (preferred) {
    return preferred;
  }

  if (children.length === 1 || !process.stdin.isTTY) {
    return children[children.length - 1] ?? children[0]!;
  }

  return promptForState(children.slice().reverse());
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

function activateState(
  root: string,
  state: StateRecord,
  action = "returned",
  options?: {
    historyIndex?: number;
    syncHistoryBeforeNavigate?: boolean;
  },
): string {
  if (options?.syncHistoryBeforeNavigate !== false) {
    const currentState = resolveCurrentState(root, loadRepo(root).states);
    syncCurrentStateHistory(root, currentState?.id ?? null);
  }
  const worktree = getWorktreeStatus(root);
  const headCommit = getHeadCommit(root);
  const repo = loadRepo(root);
  const alreadySavedDirtyState = worktree.dirty
    ? repo.states
        .slice()
        .reverse()
        .find((candidate) => worktreeMatchesCommit(root, candidate.commit)) ?? null
    : null;
  let autoSaved = false;
  if (
    !alreadySavedDirtyState &&
    (worktree.unstaged > 0 || worktree.untracked > 0 || (!headCommit && worktree.dirty))
  ) {
    const backToLabel = `back to ${state.id} ${state.description}`.trim();
    saveState(root, {
      kind: "auto",
      description: backToLabel,
      label: backToLabel,
    });
    autoSaved = true;
  }

  if (state.branch === "main" && state.description === "main") {
    createOrSwitchBranch(root, "main", state.commit, {
      force: true,
      reset: true,
    });
    const repoData = loadRepo(root);
    repoData.allowMainBranchSave = true;
    repoData.returnContext = null;
    saveRepo(root, repoData);
    importIntoJj(root);
    if (options?.historyIndex !== undefined) {
      moveStateHistoryIndex(root, options.historyIndex);
    } else {
      recordStateVisit(root, state.id);
    }
    return `${action} to ${shortStateId(state.id)} on main`;
  }

  const returnBranch = state.continuationBranch ?? state.branch;

  if (state.continuationBranch && isTipStateOnBranch(root, state.id, returnBranch)) {
    createOrSwitchBranch(root, state.continuationBranch, state.commit, {
      force: true,
      reset: true,
    });
    const repoData = loadRepo(root);
    repoData.allowMainBranchSave = false;
    repoData.returnContext = {
      stateId: state.id,
      sourceBranch: returnBranch,
      sourceLane: returnBranch,
    };
    saveRepo(root, repoData);
    importIntoJj(root);
    if (options?.historyIndex !== undefined) {
      moveStateHistoryIndex(root, options.historyIndex);
    } else {
      recordStateVisit(root, state.id);
    }
    return `${action} to ${shortStateId(state.id)} on ${stateDisplayBranch(state)}`;
  }

  switchToDetachedCommit(root, state.commit, {
    discardChanges: autoSaved || Boolean(alreadySavedDirtyState),
  });
  const repoData = loadRepo(root);
  repoData.allowMainBranchSave = false;
  repoData.returnContext = {
    stateId: state.id,
    sourceBranch: returnBranch,
    sourceLane: state.lane,
  };
  saveRepo(root, repoData);
  importIntoJj(root);
  if (options?.historyIndex !== undefined) {
    moveStateHistoryIndex(root, options.historyIndex);
  } else {
    recordStateVisit(root, state.id);
  }
  return `${action} to ${shortStateId(state.id)}`;
}

async function handleReturn(root: string, query: string): Promise<void> {
  if (query.trim() === "-") {
    console.log(activateState(root, resolvePreviousVisitedState(root)));
    return;
  }

  const repo = loadRepo(root);
  let state = resolveState(root, query);

  if (query.trim().length > 0) {
    const matches = repo.states.filter((candidate) => {
      const haystack = [
        candidate.id,
        candidate.label,
        candidate.description,
        stateDisplayBranch(candidate),
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(query.trim().toLowerCase());
    });

    if (matches.length > 1 && process.stdin.isTTY) {
      state = await promptForState(matches.slice(0, 8));
    }
  }

  console.log(activateState(root, state));
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
    case "save":
      await handleSave(
        root,
        buildSaveRequest("save", args.slice(1).join(" ")),
        {
          allowMainBranchSave: true,
          continuationBranch: null,
          suppressReturnBranchFork: true,
        },
      );
      return;
    case "step":
    case "star":
      await handleSave(root, buildSaveRequest(command, args.slice(1).join(" ")));
      return;
    case "nice":
      await handleSave(
        root,
        buildSaveRequest("nice", args.slice(1).join(" ")),
        {
          suppressReturnBranchFork: true,
        },
      );
      return;
    case "see": {
      const repo = loadRepo(root);
      const colorize = shouldColorizeOutput();
      const currentState = resolveCurrentState(root, repo.states);
      console.log(renderGraph(repo, { currentStateId: currentState?.id ?? null, colorize }));
      console.log("");
      console.log(
        renderStateTable(listStates(root), {
          colorize,
          currentStateId: currentState?.id ?? null,
          repo,
        }),
      );
      return;
    }
    case "story":
      console.log(renderStory(listStates(root)));
      return;
    case "current": {
      const currentState = resolveCurrentState(root, loadRepo(root).states);
      if (!currentState) {
        throw new Error("No current state is available.");
      }

      const repo = syncCurrentStateHistory(root, currentState.id);
      const history = repo.currentStateHistory ?? { entries: [], index: -1 };
      const parentState = currentState.parentStateId
        ? repo.states.find((state) => state.id === currentState.parentStateId) ?? null
        : null;
      console.log(
        renderCurrentState({
          state: currentState,
          parentState,
          workspaceBranch: getCurrentBranchName(root),
          historyIndex: history.index,
          historyLength: history.entries.length,
        }),
      );
      return;
    }
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
        const currentState = resolveCurrentState(root, repo.states) ??
          repo.states[repo.states.length - 1];
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

      const currentState = resolveCurrentState(root, loadRepo(root).states);
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
        kind: "cherry",
        description: `picked ${state.id} ${state.label}`,
        label: `cherry_${branchSegment(state.label)}`,
        metadata: {
          ...(currentState?.id ? { base: currentState.id } : {}),
          cherry: state.id,
        },
      });
      const activation = activateState(root, picked.state, "returned", {
        syncHistoryBeforeNavigate: false,
      });
      console.log(`picked ${state.id} onto ${getCurrentBranch(root)}`);
      console.log(renderStateSummary(picked.state));
      console.log(activation);
      return;
    }
    case "promote": {
      const query = args[1] ?? "";
      const targetKind = args[2] as "nice" | "star" | undefined;
      const promotionInput = parseStateLabelAndMessage(args.slice(3).join(" "));

      if (query.length === 0 || !targetKind) {
        throw new Error("Usage: jjk promote <state> <nice|star> [description]");
      }

      if (targetKind !== "nice" && targetKind !== "star") {
        throw new Error("Promotion kind must be `nice` or `star`.");
      }

      const state = resolveState(root, query);
      const promoted = promoteState(
        root,
        state.id,
        targetKind,
        promotionInput.description,
        promotionInput.message,
      );
      console.log(`promoted ${shortStateId(state.id)} to ${targetKind}`);
      console.log(renderStateSummary(promoted));
      return;
    }
    case "return":
      await handleReturn(root, args.slice(1).join(" "));
      return;
    case "back":
      {
        const target = resolveHistoryState(root, -1);
        console.log(
          activateState(root, target.state, "back", {
            historyIndex: target.index,
            syncHistoryBeforeNavigate: false,
          }),
        );
      }
      return;
    case "forward":
      {
        const target = resolveHistoryState(root, 1);
        console.log(
          activateState(root, target.state, "forward", {
            historyIndex: target.index,
            syncHistoryBeforeNavigate: false,
          }),
        );
      }
      return;
    case "up": {
      const repo = loadRepo(root);
      const currentState = resolveCurrentState(root, repo.states);
      if (!currentState?.parentStateId) {
        throw new Error("No parent state is available.");
      }
      console.log(activateState(root, resolveState(root, currentState.parentStateId), "up"));
      return;
    }
    case "down":
      console.log(activateState(root, await resolveChildState(root), "down"));
      return;
    case "update": {
      const branchQuery = args[1] ?? "";
      const stateQuery = args.slice(2).join(" ").trim();
      if (branchQuery.trim().length === 0) {
        throw new Error("Usage: jjk update <branch> [state]");
      }

      const updated = updateBranchTarget(
        root,
        branchQuery,
        stateQuery.length > 0 ? stateQuery : undefined,
      );
      const targetLabel = updated.state
        ? `${shortStateId(updated.state.id)} ${updated.state.label}`
        : updated.commit.slice(0, 8);
      syncCurrentStateHistory(root, resolveCurrentState(root, loadRepo(root).states)?.id ?? null);
      console.log(`updated ${updated.branch} to ${targetLabel}`);
      return;
    }
    case "watch": {
      const repo = loadRepo(root);
      await runWatch(root, repo.settings.watchDebounceMs);
      return;
    }
    case "push":
      pushCurrentBranchAndStateRefs(root);
      console.log("pushed current branch and jjk state refs");
      return;
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
        syncCurrentStateHistory(root, resolveCurrentState(root, loadRepo(root).states)?.id ?? null);
        console.log(`lane switched: ${existing.name} (${existing.branch})`);
        return;
      }

      const lane = createLane(root, name);
      syncCurrentStateHistory(root, resolveCurrentState(root, loadRepo(root).states)?.id ?? null);
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
    case "snapshots": {
      const mode = (args[1] ?? "").trim().toLowerCase();
      if (mode !== "on" && mode !== "off") {
        throw new Error("Usage: jjk snapshots <on|off>");
      }

      const repo = loadRepo(root);
      repo.settings.showWorkspaceSnapshotsInGit = mode === "on";
      saveRepo(root, repo);

      if (mode === "off") {
        const removed = pruneJjKeepRefs(root);
        console.log(`git workspace snapshots hidden (${removed} refs removed)`);
        return;
      }

      importIntoJj(root);
      console.log("git workspace snapshots enabled");
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
          syncCurrentStateHistory(root, resolveCurrentState(root, loadRepo(root).states)?.id ?? null);
          console.log(`timeshift restored to ${record.branch} at ${shortStateId(state.id)}`);
        } else {
          createOrSwitchBranch(root, record.branch, undefined, {
            force: true,
            reset: true,
          });
          syncCurrentStateHistory(root, resolveCurrentState(root, loadRepo(root).states)?.id ?? null);
          console.log(`timeshift restored to branch ${record.branch}`);
        }
        console.log(`saved cwd: ${record.relativeCwd}`);
        return;
      }

      console.log(renderTimeshifts(listTimeshifts(root)));
      return;
    }
    default:
      await handleSave(root, buildSaveRequest("new", args.join(" ")));
  }
}
