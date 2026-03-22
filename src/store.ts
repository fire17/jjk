import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import {
  createOrSwitchBranch,
  createSnapshotCommit,
  ensureLocalExcludes,
  getCurrentBranch,
  getCurrentBranchName,
  getHeadCommit,
  hasDirtyWorktree,
  importIntoJj,
  initGitRepo,
  initJjRepo,
  isGitRepo,
  restoreHeadWorktree,
  updateRef,
} from "./git";
import type {
  FreezeRecord,
  LaneRecord,
  RepoData,
  SaveStateRequest,
  SaveStateResult,
  StateNavigationHistory,
  StateRecord,
  StateKind,
  TimeshiftRecord,
} from "./types";
import {
  continuationBranchName,
  defaultLabel,
  ensureDescription,
  findStateMatches,
  branchSegment,
  isDeletedState,
  nowIso,
  shortId,
  slugify,
  stateDisplayBranch,
  stateGitCommit,
} from "./utils";

export const JJK_DIR = ".jjk";
const REPO_FILE = "repo.json";
const FREEZE_DIR = "freezes";

export function findSafeSpaceRoot(startCwd: string): string | null {
  let current = resolve(startCwd);

  while (true) {
    if (existsSync(join(current, JJK_DIR, REPO_FILE))) {
      return current;
    }

    const parent = dirname(current);
    if (parent === current) {
      return null;
    }
    current = parent;
  }
}

export function requireSafeSpace(startCwd: string): string {
  const root = findSafeSpaceRoot(startCwd);
  if (!root) {
    throw new Error("This directory is not a jjk safe space yet. Run `jjk init` first.");
  }
  return root;
}

export function repoFilePath(root: string): string {
  return join(root, JJK_DIR, REPO_FILE);
}

function normalizeStateRecord(state: StateRecord): StateRecord {
  const gitCommit = state.metadata?.gitCommit ?? state.commit;
  if (state.metadata?.gitCommit === gitCommit) {
    return state;
  }

  return {
    ...state,
    metadata: {
      ...(state.metadata ?? {}),
      gitCommit,
    },
  };
}

function normalizeCurrentStateHistory(
  history: StateNavigationHistory | null | undefined,
  states: StateRecord[],
): StateNavigationHistory {
  const knownStateIds = new Set(states.map((state) => state.id));
  const entries = (history?.entries ?? []).filter((stateId) => knownStateIds.has(stateId));

  if (entries.length === 0) {
    return {
      entries: [],
      index: -1,
    };
  }

  const requestedIndex = history?.index ?? entries.length - 1;
  return {
    entries,
    index: Math.max(0, Math.min(requestedIndex, entries.length - 1)),
  };
}

export function loadRepo(root: string): RepoData {
  const repo = JSON.parse(readFileSync(repoFilePath(root), "utf8")) as RepoData;
  const states = repo.states.map((state) => normalizeStateRecord(state));
  return {
    ...repo,
    states,
    currentStateHistory: normalizeCurrentStateHistory(repo.currentStateHistory, states),
  };
}

export function saveRepo(root: string, repo: RepoData): void {
  repo.updatedAt = nowIso();
  const path = repoFilePath(root);
  Bun.write(path, `${JSON.stringify(repo, null, 2)}\n`);
}

export function initSafeSpace(startCwd: string): { root: string; repo: RepoData } {
  const root = resolve(startCwd);
  initGitRepo(root);
  ensureLocalExcludes(root);
  initJjRepo(root);
  importIntoJj(root);

  const jjkRoot = join(root, JJK_DIR);
  mkdirSync(jjkRoot, { recursive: true });
  mkdirSync(join(jjkRoot, FREEZE_DIR), { recursive: true });

  const filePath = repoFilePath(root);
  if (!existsSync(filePath)) {
    const branch = isGitRepo(root) ? getCurrentBranch(root) : "main";
    const createdAt = nowIso();
    const repo: RepoData = {
      version: 1,
      safeSpaceId: shortId(),
      createdAt,
      updatedAt: createdAt,
      settings: {
        watchDebounceMs: 1200,
        autoStatePrefix: "auto",
        showWorkspaceSnapshotsInGit: false,
      },
      states: [],
      lanes: {},
      branchLaneMap: {},
      allowMainBranchSave: false,
      returnContext: null,
      currentStateHistory: {
        entries: [],
        index: -1,
      },
      timeshifts: [],
      freezes: [],
    };

    ensureLane(repo, branch, branch, branch);
    saveRepo(root, repo);

    const initial = saveState(root, {
      kind: "save",
      description: branch,
    }, {
      forceCurrentBranch: branch,
      allowMainBranchSave: true,
      continuationBranch: null,
    });
    const seeded = initial.repo;
    seeded.allowMainBranchSave = false;
    seeded.currentStateHistory = {
      entries: [initial.state.id],
      index: 0,
    };
    saveRepo(root, seeded);
  }

  return { root, repo: loadRepo(root) };
}

export function ensureLane(
  repo: RepoData,
  branch: string,
  laneName: string,
  baseRef: string,
): LaneRecord {
  const existingLaneName = repo.branchLaneMap[branch];
  if (existingLaneName && repo.lanes[existingLaneName]) {
    return repo.lanes[existingLaneName];
  }

  const name = laneName.trim() || branch;
  if (!repo.lanes[name]) {
    const createdAt = nowIso();
    repo.lanes[name] = {
      name,
      branch,
      baseRef,
      createdAt,
      updatedAt: createdAt,
      currentStateId: null,
    };
  }

  repo.branchLaneMap[branch] = name;
  return repo.lanes[name];
}

function getLatestStateOnBranch(repo: RepoData, branch: string): StateRecord | null {
  for (let index = repo.states.length - 1; index >= 0; index -= 1) {
    if (repo.states[index]?.branch === branch && !isDeletedState(repo.states[index]!)) {
      return repo.states[index] ?? null;
    }
  }
  return null;
}

function findLatestVisibleStateForLane(
  repo: RepoData,
  laneName: string,
  branch: string,
  excludeStateId?: string,
): StateRecord | null {
  for (let index = repo.states.length - 1; index >= 0; index -= 1) {
    const state = repo.states[index];
    if (!state || state.id === excludeStateId || isDeletedState(state)) {
      continue;
    }
    if (state.lane === laneName || stateDisplayBranch(state) === branch) {
      return state;
    }
  }
  return null;
}

function buildStateCommitMessage(input: {
  kind: string;
  label: string;
  description: string;
  message?: string;
  branch: string;
  lane: string;
  continuationBranch?: string | null;
}): string {
  const displayBranch = input.continuationBranch ?? input.branch;
  const subject = `${input.label} [${input.kind}] (${displayBranch}) - jjk`;
  const body = [
    `Kind: ${input.kind}`,
    `Label: ${input.label}`,
    `Description: ${input.description}`,
    `Message: ${input.message ?? "none"}`,
    `Branch: ${input.branch}`,
    `Lane: ${input.lane}`,
    `Continuation-Branch: ${input.continuationBranch ?? "none"}`,
  ].join("\n");
  return `${subject}\n\n${body}`;
}

function withKindTags(kind: StateKind, tags: string[] | undefined): string[] {
  const merged = new Set(tags ?? []);
  if (kind === "star") {
    merged.add("star");
  }
  if (kind === "stash") {
    merged.add("stash");
  }
  return Array.from(merged);
}

function findMostRecentStateForCommit(
  repo: RepoData,
  commit: string,
  branch?: string | null,
): StateRecord | null {
  for (let index = repo.states.length - 1; index >= 0; index -= 1) {
    const state = repo.states[index];
    if (!state || stateGitCommit(state) !== commit) {
      continue;
    }
    if (!branch || stateDisplayBranch(state) === branch) {
      return state;
    }
  }

  for (let index = repo.states.length - 1; index >= 0; index -= 1) {
    const state = repo.states[index];
    if (state && stateGitCommit(state) === commit) {
      return state;
    }
  }

  return null;
}

function findLatestStateForLane(
  repo: RepoData,
  laneName: string,
  branch: string,
  excludeStateId?: string,
): StateRecord | null {
  for (let index = repo.states.length - 1; index >= 0; index -= 1) {
    const state = repo.states[index];
    if (!state || state.id === excludeStateId || isDeletedState(state)) {
      continue;
    }
    if (state.lane === laneName || stateDisplayBranch(state) === branch) {
      return state;
    }
  }
  return null;
}

export function saveState(
  root: string,
  request: SaveStateRequest,
): SaveStateResult;

export function saveState(
  root: string,
  request: SaveStateRequest,
  options: {
    forceCurrentBranch?: string;
    allowMainBranchSave?: boolean;
    continuationBranch?: string | null;
    suppressReturnBranchFork?: boolean;
  } = {},
): SaveStateResult {
  const repo = loadRepo(root);
  const returnContext = repo.returnContext ?? null;
  const description = ensureDescription(request.kind, request.description);
  const label = request.label ?? defaultLabel(request.kind, description);
  const message = request.message?.trim() || undefined;
  const requestMetadata = request.metadata ?? {};
  const returnedState = returnContext?.stateId
    ? repo.states.find((state) => state.id === returnContext.stateId) ?? null
    : null;
  const returnedStateHasChildren = returnedState
    ? repo.states.some((state) => state.parentStateId === returnedState.id)
    : false;

  if (returnContext && request.kind !== "auto") {
    const currentBranch = getCurrentBranchName(root);
    if (!options.suppressReturnBranchFork) {
      if (
        currentBranch &&
        returnedState?.continuationBranch === currentBranch &&
        returnedStateHasChildren
      ) {
        const branchName = continuationBranchName(description);
        createOrSwitchBranch(root, branchName, getHeadCommit(root) ?? undefined);
      } else if (currentBranch === null) {
        const branchName = continuationBranchName(description);
        createOrSwitchBranch(root, branchName, getHeadCommit(root) ?? undefined);
      }
    }
    repo.returnContext = null;
  }

  const currentBranch = options.forceCurrentBranch ?? getCurrentBranchName(root);
  const activeBranch = currentBranch ?? returnContext?.sourceBranch ?? getCurrentBranch(root);
  const saveOnMain =
    activeBranch === "main" &&
    (options.allowMainBranchSave ?? repo.allowMainBranchSave ?? false);
  const branch = activeBranch;
  const continueDetachedSourceBranch = Boolean(
    currentBranch === null &&
    returnContext &&
    options.suppressReturnBranchFork,
  );
  const laneName =
    currentBranch === null && returnContext
      ? returnContext.sourceLane
      : branch;
  const baseRef =
    currentBranch === null && returnContext
      ? returnContext.sourceBranch
      : branch;
  const headCommit = getHeadCommit(root);
  const commitTargetBranch =
    continueDetachedSourceBranch
      ? branch
      : branch === "main" && !saveOnMain
      ? continuationBranchName(description)
      : undefined;
  const lane = ensureLane(repo, branch, laneName, baseRef);
  const logicalParentState =
    continueDetachedSourceBranch && lane.currentStateId
      ? repo.states.find((state) => state.id === lane.currentStateId) ?? null
      : branch === "main" && !saveOnMain && lane.currentStateId
      ? repo.states.find((state) => state.id === lane.currentStateId) ?? null
      : null;
  const continuationBranch =
    options.continuationBranch !== undefined
      ? options.continuationBranch
      : request.kind === "auto"
      ? null
      : branch.startsWith("jjk/")
        ? branch
        : request.kind === "new" || branch === "main"
          ? continuationBranchName(description)
          : null;
  const commitMessage = buildStateCommitMessage({
    kind: request.kind,
    label,
    description,
    message,
    branch,
    lane: lane.name,
    continuationBranch,
  });
  const snapshot = createSnapshotCommit(
    root,
    commitMessage,
    {
      parentCommit: logicalParentState?.commit ?? undefined,
      targetBranch: commitTargetBranch,
    },
  );

  const checkedOutStateId =
    headCommit
      ? findMostRecentStateForCommit(
          repo,
          headCommit,
          getCurrentBranchName(root) ?? undefined,
        )?.id ?? null
      : null;
  const hierarchyParentStateId =
    continueDetachedSourceBranch && returnedState
      ? returnedState.id
      : null;
  const parentStateId =
    hierarchyParentStateId ??
    logicalParentState?.id ??
    checkedOutStateId ??
    lane.currentStateId ??
    repo.states.find((state) => state.commit === snapshot.parentCommit)?.id ??
    null;

  const state: StateRecord = {
    id: shortId(),
    kind: request.kind,
    label,
    description,
    createdAt: nowIso(),
    branch,
    lane: lane.name,
    continuationBranch,
    commit: snapshot.commit,
    parentCommit: snapshot.parentCommit,
    parentStateId,
    tags: withKindTags(request.kind, request.tags),
    stats: {
      changedFiles: snapshot.changedFiles,
    },
    metadata: {
      ...requestMetadata,
      gitCommit: snapshot.commit,
      ...(message ? { message } : {}),
    },
  };

  repo.states.push(state);
  lane.currentStateId = state.id;
  lane.updatedAt = state.createdAt;
  repo.allowMainBranchSave = false;
  if (continuationBranch && continuationBranch !== branch) {
    const continuationLane = ensureLane(repo, continuationBranch, continuationBranch, branch);
    continuationLane.branch = continuationBranch;
    continuationLane.baseRef = branch;
    continuationLane.currentStateId = state.id;
    continuationLane.updatedAt = state.createdAt;
    repo.branchLaneMap[continuationBranch] = continuationLane.name;
    updateRef(root, `refs/heads/${continuationBranch}`, state.commit);
  }
  if (continueDetachedSourceBranch) {
    createOrSwitchBranch(root, branch, snapshot.commit, {
      force: true,
      reset: true,
    });
  }
  if (commitTargetBranch && branch === "main") {
    restoreHeadWorktree(root);
  }
  updateRef(root, `refs/jjk/states/${state.id}`, state.commit);
  saveRepo(root, repo);
  importIntoJj(root);

  return { state, repo };
}

export function isTipStateOnBranch(root: string, stateId: string, branch: string): boolean {
  const repo = loadRepo(root);
  const laneName = repo.branchLaneMap[branch];
  if (laneName && repo.lanes[laneName]?.currentStateId === stateId) {
    return true;
  }
  return getLatestStateOnBranch(repo, branch)?.id === stateId;
}

function resolveWorkspaceStateForBranch(
  repo: RepoData,
  root: string,
  branchName: string | null,
  headCommit: string | null,
): StateRecord | null {
  const returnedStateId = repo.returnContext?.stateId ?? null;
  if (returnedStateId) {
    const returned = repo.states.find((state) => state.id === returnedStateId) ?? null;
    if (returned && (!headCommit || stateGitCommit(returned) === headCommit)) {
      return returned;
    }
  }

  if (headCommit) {
    for (let index = repo.states.length - 1; index >= 0; index -= 1) {
      const state = repo.states[index];
      if (!state || isDeletedState(state)) {
        continue;
      }
      if (stateGitCommit(state) !== headCommit) {
        continue;
      }
      if (!branchName || stateDisplayBranch(state) === branchName) {
        return state;
      }
    }
  }

  if (branchName) {
    const laneName = repo.branchLaneMap[branchName];
    const stateId = laneName ? repo.lanes[laneName]?.currentStateId ?? null : null;
    if (stateId) {
      return repo.states.find((state) => state.id === stateId) ?? null;
    }
  }

  return null;
}

export function stashWorkspace(
  root: string,
  request: {
    description: string;
    label?: string;
    message?: string;
  },
): SaveStateResult {
  if (!hasDirtyWorktree(root)) {
    throw new Error("No working changes are available to stash.");
  }

  const repo = loadRepo(root);
  const currentBranchName = getCurrentBranchName(root);
  const sourceBranch = currentBranchName ?? repo.returnContext?.sourceBranch ?? getCurrentBranch(root);
  const headCommit = getHeadCommit(root);
  const sourceState = resolveWorkspaceStateForBranch(repo, root, currentBranchName, headCommit);
  const description = ensureDescription("stash", request.description);
  const label = request.label ?? defaultLabel("stash", description);
  const message = request.message?.trim() || undefined;
  const stateId = shortId();
  const stashBranch = `jjk/stash_${branchSegment(label)}_${stateId}`;
  const lane = ensureLane(repo, stashBranch, stashBranch, sourceBranch);
  const continuationBranch = stashBranch;
  const commitMessage = buildStateCommitMessage({
    kind: "stash",
    label,
    description,
    message,
    branch: stashBranch,
    lane: lane.name,
    continuationBranch,
  });
  const snapshot = createSnapshotCommit(root, commitMessage, {
    targetBranch: stashBranch,
    parentCommit: headCommit,
  });

  const state: StateRecord = {
    id: stateId,
    kind: "stash",
    label,
    description,
    createdAt: nowIso(),
    branch: stashBranch,
    lane: lane.name,
    continuationBranch,
    commit: snapshot.commit,
    parentCommit: snapshot.parentCommit,
    parentStateId: sourceState?.id ?? null,
    tags: withKindTags("stash", []),
    stats: {
      changedFiles: snapshot.changedFiles,
    },
    metadata: {
      gitCommit: snapshot.commit,
      ...(message ? { message } : {}),
      stashFromBranch: sourceBranch,
      ...(sourceState?.id ? { stashFromStateId: sourceState.id } : {}),
    },
  };

  repo.states.push(state);
  lane.currentStateId = state.id;
  lane.updatedAt = state.createdAt;
  repo.branchLaneMap[stashBranch] = lane.name;
  updateRef(root, `refs/jjk/states/${state.id}`, state.commit);
  saveRepo(root, repo);
  restoreHeadWorktree(root);
  importIntoJj(root);

  return { state, repo };
}

export function resolveState(
  root: string,
  query: string,
  options?: {
    includeDeleted?: boolean;
  },
): StateRecord {
  const repo = loadRepo(root);
  const states = (options?.includeDeleted ? repo.states : repo.states.filter((state) => !isDeletedState(state)));
  if (states.length === 0) {
    throw new Error("No saved states exist yet.");
  }

  const trimmed = query.trim();
  if (trimmed.length === 0) {
    return states[states.length - 1];
  }

  const exact = states.find(
    (state) =>
      state.id === trimmed ||
      state.label === trimmed ||
      state.description === trimmed ||
      state.metadata?.message === trimmed,
  );
  if (exact) {
    return exact;
  }

  const matches = findStateMatches(states, trimmed);
  if (matches.length === 0) {
    throw new Error(`No state matched \`${trimmed}\`.`);
  }

  return matches[0].state;
}

export function listStates(
  root: string,
  options?: {
    includeDeleted?: boolean;
  },
): StateRecord[] {
  const states = loadRepo(root).states.filter((state) => options?.includeDeleted || !isDeletedState(state));
  return states.slice().sort((left, right) =>
    left.createdAt.localeCompare(right.createdAt),
  );
}

export function deleteState(root: string, stateId: string): StateRecord {
  const repo = loadRepo(root);
  const state = repo.states.find((entry) => entry.id === stateId);
  if (!state) {
    throw new Error(`No state matched \`${stateId}\`.`);
  }
  if (isDeletedState(state)) {
    throw new Error(`State \`${stateId}\` is already deleted.`);
  }

  const deletedBranch = `deleted/${branchSegment(state.label)}`;
  const previousBranch = state.branch;
  const previousLane = state.lane;
  const previousContinuationBranch = state.continuationBranch ?? null;
  const lane = repo.lanes[previousLane] ?? null;
  const wasLaneCurrent = lane?.currentStateId === state.id;

  state.metadata = {
    ...(state.metadata ?? {}),
    gitCommit: stateGitCommit(state),
    deletedAt: nowIso(),
    deletedBranch,
    deletedLocation: {
      branch: previousBranch,
      lane: previousLane,
      continuationBranch: previousContinuationBranch,
      parentStateId: state.parentStateId,
      wasLaneCurrent,
    },
  };
  state.branch = deletedBranch;
  state.lane = deletedBranch;
  state.continuationBranch = null;

  if (wasLaneCurrent && lane) {
    lane.currentStateId = findLatestVisibleStateForLane(
      repo,
      previousLane,
      previousBranch,
      state.id,
    )?.id ?? null;
    lane.updatedAt = nowIso();
  }

  if (repo.returnContext?.stateId === state.id) {
    repo.returnContext = null;
  }

  repo.currentStateHistory = normalizeCurrentStateHistory(repo.currentStateHistory, repo.states);
  saveRepo(root, repo);
  importIntoJj(root);
  return state;
}

export function recoverState(root: string, stateId: string): StateRecord {
  const repo = loadRepo(root);
  const state = repo.states.find((entry) => entry.id === stateId);
  if (!state) {
    throw new Error(`No state matched \`${stateId}\`.`);
  }
  const deletedLocation = state.metadata?.deletedLocation;
  if (!isDeletedState(state) || !deletedLocation) {
    throw new Error(`State \`${stateId}\` is not deleted.`);
  }

  state.branch = deletedLocation.branch;
  state.lane = deletedLocation.lane;
  state.continuationBranch = deletedLocation.continuationBranch ?? null;
  state.parentStateId = deletedLocation.parentStateId;
  state.metadata = {
    ...(state.metadata ?? {}),
    gitCommit: stateGitCommit(state),
  };
  delete state.metadata.deletedAt;
  delete state.metadata.deletedBranch;
  delete state.metadata.deletedLocation;

  if (deletedLocation.wasLaneCurrent) {
    const lane = ensureLane(repo, state.branch, deletedLocation.lane, state.branch);
    lane.currentStateId = state.id;
    lane.updatedAt = nowIso();
    repo.branchLaneMap[state.branch] = lane.name;
  }

  saveRepo(root, repo);
  importIntoJj(root);
  return state;
}

export function eraseState(root: string, stateId: string): StateRecord {
  const repo = loadRepo(root);
  const index = repo.states.findIndex((entry) => entry.id === stateId);
  if (index < 0) {
    throw new Error(`No state matched \`${stateId}\`.`);
  }
  if (repo.states.length <= 1) {
    throw new Error("Cannot erase the last remaining state.");
  }

  const state = repo.states[index]!;
  const hasChildren = repo.states.some((entry) => entry.parentStateId === state.id);
  if (hasChildren) {
    throw new Error(`Cannot erase state \`${stateId}\` because other states depend on it.`);
  }

  repo.states.splice(index, 1);

  for (const lane of Object.values(repo.lanes)) {
    if (lane.currentStateId !== state.id) {
      continue;
    }
    lane.currentStateId = findLatestVisibleStateForLane(
      repo,
      lane.name,
      lane.branch,
      state.id,
    )?.id ?? null;
    lane.updatedAt = nowIso();
  }

  if (repo.returnContext?.stateId === state.id) {
    repo.returnContext = null;
  }

  repo.currentStateHistory = normalizeCurrentStateHistory(repo.currentStateHistory, repo.states);
  saveRepo(root, repo);
  importIntoJj(root);
  return state;
}

export function updateBranchTarget(
  root: string,
  branchQuery: string,
  stateQuery?: string,
): {
  branch: string;
  commit: string;
  state: StateRecord | null;
} {
  const trimmedBranch = branchQuery.trim();
  if (trimmedBranch.length === 0) {
    throw new Error("Provide a branch to update.");
  }

  const repo = loadRepo(root);
  const resolvedLane = resolveLane(root, trimmedBranch);
  const branch = resolvedLane?.branch ?? trimmedBranch;
  const state = stateQuery?.trim() ? resolveState(root, stateQuery) : null;
  const commit = state ? stateGitCommit(state) : getHeadCommit(root);

  if (!commit) {
    throw new Error("No current Git commit is available to update the branch to.");
  }

  const currentBranch = getCurrentBranchName(root);
  const currentHead = getHeadCommit(root);
  const shouldCheckoutOrMove = currentBranch !== branch || currentHead !== commit;
  if (shouldCheckoutOrMove) {
    if (hasDirtyWorktree(root)) {
      throw new Error(
        `Cannot update \`${branch}\` while the worktree has uncommitted changes.`,
      );
    }
    createOrSwitchBranch(root, branch, commit, {
      force: true,
      reset: true,
    });
  }

  const matchedState =
    state ??
    findMostRecentStateForCommit(repo, commit, branch) ??
    null;
  const laneName = repo.branchLaneMap[branch];
  let lane =
    laneName && repo.lanes[laneName]
      ? repo.lanes[laneName]
      : branch.startsWith("jjk/")
        ? ensureLane(
            repo,
            branch,
            branch,
            state ? stateDisplayBranch(state) : currentBranch ?? branch,
          )
        : null;
  if (lane) {
    lane.branch = branch;
    lane.updatedAt = nowIso();
    repo.branchLaneMap[branch] = lane.name;
  }

  let currentState = matchedState;
  if (matchedState) {
    if (!lane) {
      lane = ensureLane(repo, branch, branch, currentBranch ?? branch);
      lane.branch = branch;
      lane.updatedAt = nowIso();
      repo.branchLaneMap[branch] = lane.name;
    }

    const targetContinuationBranch = branch === "main" ? null : branch;
    const needsContextRewrite =
      matchedState.branch !== branch ||
      matchedState.lane !== lane.name ||
      (matchedState.continuationBranch ?? null) !== targetContinuationBranch;

    if (needsContextRewrite) {
      const previousContexts = [
        ...((matchedState.metadata?.priorContexts ?? []).map((context) => ({ ...context }))),
        {
          branch: matchedState.branch,
          lane: matchedState.lane,
          continuationBranch: matchedState.continuationBranch ?? null,
          updatedAt: nowIso(),
        },
      ];
      matchedState.branch = branch;
      matchedState.lane = lane.name;
      matchedState.continuationBranch = targetContinuationBranch;
      matchedState.metadata = {
        ...(matchedState.metadata ?? {}),
        gitCommit: stateGitCommit(matchedState),
        priorContexts: previousContexts,
      };

      for (const candidateLane of Object.values(repo.lanes)) {
        if (candidateLane.currentStateId !== matchedState.id || candidateLane.name === lane.name) {
          continue;
        }
        const replacement = findLatestStateForLane(
          repo,
          candidateLane.name,
          candidateLane.branch,
          matchedState.id,
        );
        candidateLane.currentStateId = replacement?.id ?? null;
        candidateLane.updatedAt = nowIso();
      }
    }
  }

  if (lane) {
    lane.currentStateId = currentState?.id ?? null;
    lane.updatedAt = nowIso();
  }

  saveRepo(root, repo);
  importIntoJj(root);

  return {
    branch,
    commit,
    state: currentState,
  };
}

export function createLane(root: string, name: string): LaneRecord {
  const repo = loadRepo(root);
  const sourceBranch = getCurrentBranch(root);
  const sourceLaneName = repo.branchLaneMap[sourceBranch];
  const sourceStateId = sourceLaneName
    ? repo.lanes[sourceLaneName]?.currentStateId ?? null
    : null;
  const sourceState = sourceStateId
    ? repo.states.find((state) => state.id === sourceStateId) ?? null
    : null;
  const branchName = `jjk/lane/${slugify(name)}`;
  const headCommit = sourceState?.commit ?? getHeadCommit(root);
  const baseRef = sourceState?.commit ?? headCommit ?? getCurrentBranch(root);

  createOrSwitchBranch(root, branchName, headCommit ?? undefined);
  const lane = ensureLane(repo, branchName, name, baseRef);
  lane.branch = branchName;
  lane.updatedAt = nowIso();
  lane.currentStateId = lane.currentStateId ?? sourceStateId;
  repo.branchLaneMap[branchName] = lane.name;
  saveRepo(root, repo);
  return lane;
}

export function listLanes(root: string): LaneRecord[] {
  const repo = loadRepo(root);
  return Object.values(repo.lanes).sort((left, right) =>
    left.createdAt.localeCompare(right.createdAt),
  );
}

export function resolveLane(root: string, query: string): LaneRecord | null {
  const lanes = listLanes(root);
  const trimmed = query.trim().toLowerCase();
  if (trimmed.length === 0) {
    return null;
  }

  const exact = lanes.find(
    (lane) =>
      lane.name.toLowerCase() === trimmed || lane.branch.toLowerCase() === trimmed,
  );
  if (exact) {
    return exact;
  }

  return (
    lanes.find(
      (lane) =>
        lane.name.toLowerCase().includes(trimmed) ||
        lane.branch.toLowerCase().includes(trimmed),
    ) ?? null
  );
}

export function resolveLatestStateForBranch(root: string, query: string): StateRecord {
  const repo = loadRepo(root);
  const trimmed = query.trim();
  if (trimmed.length === 0) {
    throw new Error("Provide a branch to resolve.");
  }

  const resolvedLane = resolveLane(root, trimmed);
  const branch = resolvedLane?.branch ?? trimmed;
  const laneName = repo.branchLaneMap[branch] ?? resolvedLane?.name ?? null;
  const lane = laneName ? repo.lanes[laneName] ?? null : null;
  const laneStateId = lane?.currentStateId ?? null;

  if (laneStateId) {
    const laneState = repo.states.find((state) => state.id === laneStateId && !isDeletedState(state));
    if (laneState) {
      return laneState;
    }
  }

  for (let index = repo.states.length - 1; index >= 0; index -= 1) {
    const state = repo.states[index];
    if (!state || isDeletedState(state)) {
      continue;
    }
    if (stateDisplayBranch(state) === branch || state.branch === branch || state.lane === branch) {
      return state;
    }
  }

  throw new Error(`No saved state is available for branch \`${trimmed}\`.`);
}

export function rememberTimeshift(root: string, label: string): TimeshiftRecord {
  const repo = loadRepo(root);
  const branch = getCurrentBranch(root);
  const lane = ensureLane(repo, branch, branch, branch);
  const record: TimeshiftRecord = {
    id: shortId(),
    label,
    createdAt: nowIso(),
    branch,
    lane: lane.name,
    stateId: lane.currentStateId,
    relativeCwd: relative(root, process.cwd()) || ".",
    env: {
      SHELL: process.env.SHELL ?? "",
      TERM: process.env.TERM ?? "",
      COLORTERM: process.env.COLORTERM ?? "",
    },
  };
  repo.timeshifts.push(record);
  saveRepo(root, repo);
  return record;
}

export function listTimeshifts(root: string): TimeshiftRecord[] {
  return loadRepo(root).timeshifts.slice().sort((left, right) =>
    left.createdAt.localeCompare(right.createdAt),
  );
}

export function resolveTimeshift(root: string, query: string): TimeshiftRecord {
  const timeshifts = listTimeshifts(root);
  const trimmed = query.trim();
  const exact = timeshifts.find((entry) => entry.id === trimmed || entry.label === trimmed);
  if (exact) {
    return exact;
  }

  const lower = trimmed.toLowerCase();
  const partial = timeshifts.find(
    (entry) =>
      entry.id.includes(lower) || entry.label.toLowerCase().includes(lower),
  );
  if (!partial) {
    throw new Error(`No timeshift matched \`${query}\`.`);
  }
  return partial;
}

export function recordFreeze(root: string, stateId: string): FreezeRecord {
  const repo = loadRepo(root);
  const id = shortId();
  const record: FreezeRecord = {
    id,
    stateId,
    createdAt: nowIso(),
    bundlePath: join(JJK_DIR, FREEZE_DIR, `${id}.bundle`),
    manifestPath: join(JJK_DIR, FREEZE_DIR, `${id}.json`),
  };
  repo.freezes.push(record);
  saveRepo(root, repo);
  return record;
}

export function starState(root: string, stateId: string): StateRecord {
  const repo = loadRepo(root);
  const state = repo.states.find((entry) => entry.id === stateId);
  if (!state) {
    throw new Error(`No state matched \`${stateId}\`.`);
  }
  if (isDeletedState(state)) {
    throw new Error(`Cannot star deleted state \`${stateId}\`.`);
  }

  if (!state.tags.includes("star")) {
    state.tags = [...state.tags, "star"];
    saveRepo(root, repo);
  }

  return state;
}

export function promoteState(
  root: string,
  sourceStateId: string,
  kind: Extract<StateKind, "nice" | "star">,
  description?: string,
  message?: string,
): StateRecord {
  const repo = loadRepo(root);
  const source = repo.states.find((state) => state.id === sourceStateId);
  const nextMessage = message?.trim() || undefined;
  if (!source) {
    throw new Error(`No state matched \`${sourceStateId}\`.`);
  }

  const promoted: StateRecord = {
    ...source,
    id: shortId(),
    kind,
    label: (description?.trim() || source.label).slice(0, 96),
    description: description?.trim() || source.description,
    createdAt: nowIso(),
    parentStateId: source.id,
    tags: withKindTags(kind, source.tags),
    metadata: {
      ...(source.metadata ?? {}),
      gitCommit: source.metadata?.gitCommit ?? source.commit,
      ...(nextMessage ? { message: nextMessage } : {}),
    },
  };

  repo.states.push(promoted);

  const lane = repo.lanes[source.lane];
  if (lane && lane.currentStateId === source.id) {
    lane.currentStateId = promoted.id;
    lane.updatedAt = promoted.createdAt;
  }

  saveRepo(root, repo);
  return promoted;
}
