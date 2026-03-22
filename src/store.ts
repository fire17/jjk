import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import {
  createOrSwitchBranch,
  createSnapshotCommit,
  ensureLocalExcludes,
  getCurrentBranch,
  getCurrentBranchName,
  getHeadCommit,
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
  StateRecord,
  StateKind,
  TimeshiftRecord,
} from "./types";
import {
  continuationBranchName,
  defaultLabel,
  ensureDescription,
  findStateMatches,
  nowIso,
  shortId,
  slugify,
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

export function loadRepo(root: string): RepoData {
  return JSON.parse(readFileSync(repoFilePath(root), "utf8")) as RepoData;
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
    if (repo.states[index]?.branch === branch) {
      return repo.states[index] ?? null;
    }
  }
  return null;
}

function buildStateCommitMessage(input: {
  kind: string;
  label: string;
  description: string;
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
    `Branch: ${input.branch}`,
    `Lane: ${input.lane}`,
    `Continuation-Branch: ${input.continuationBranch ?? "none"}`,
  ].join("\n");
  return `${subject}\n\n${body}`;
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
  } = {},
): SaveStateResult {
  const repo = loadRepo(root);
  const description = ensureDescription(request.kind, request.description);
  const label = request.label ?? defaultLabel(request.kind, description);
  const returnedState = repo.returnContext?.stateId
    ? repo.states.find((state) => state.id === repo.returnContext?.stateId) ?? null
    : null;
  const returnedStateHasChildren = returnedState
    ? repo.states.some((state) => state.parentStateId === returnedState.id)
    : false;

  if (repo.returnContext && request.kind !== "auto") {
    const currentBranch = getCurrentBranchName(root);
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
    repo.returnContext = null;
  }

  const currentBranch = options.forceCurrentBranch ?? getCurrentBranchName(root);
  const activeBranch = currentBranch ?? repo.returnContext?.sourceBranch ?? getCurrentBranch(root);
  const saveOnMain =
    activeBranch === "main" &&
    (options.allowMainBranchSave ?? repo.allowMainBranchSave ?? false);
  const branch = activeBranch;
  const laneName =
    currentBranch === null && repo.returnContext
      ? repo.returnContext.sourceLane
      : branch;
  const baseRef =
    currentBranch === null && repo.returnContext
      ? repo.returnContext.sourceBranch
      : branch;
  const headCommit = getHeadCommit(root);
  const commitTargetBranch =
    branch === "main" && !saveOnMain
      ? continuationBranchName(description)
      : undefined;
  const lane = ensureLane(repo, branch, laneName, baseRef);
  const logicalParentState =
    branch === "main" && !saveOnMain && lane.currentStateId
      ? repo.states.find((state) => state.id === lane.currentStateId) ?? null
      : null;
  const continuationBranch =
    options.continuationBranch !== undefined
      ? options.continuationBranch
      : request.kind === "auto"
      ? null
      : branch.startsWith("jjk/")
        ? branch
        : branch === "main"
          ? continuationBranchName(description)
          : null;
  const commitMessage = buildStateCommitMessage({
    kind: request.kind,
    label,
    description,
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
      ? repo.states.find((state) => state.commit === headCommit)?.id ?? null
      : null;
  const parentStateId =
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
    tags: request.tags ?? [],
    stats: {
      changedFiles: snapshot.changedFiles,
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

export function resolveState(root: string, query: string): StateRecord {
  const repo = loadRepo(root);
  if (repo.states.length === 0) {
    throw new Error("No saved states exist yet.");
  }

  const trimmed = query.trim();
  if (trimmed.length === 0) {
    return repo.states[repo.states.length - 1];
  }

  const exact = repo.states.find(
    (state) =>
      state.id === trimmed ||
      state.label === trimmed ||
      state.description === trimmed,
  );
  if (exact) {
    return exact;
  }

  const matches = findStateMatches(repo.states, trimmed);
  if (matches.length === 0) {
    throw new Error(`No state matched \`${trimmed}\`.`);
  }

  return matches[0].state;
}

export function listStates(root: string): StateRecord[] {
  return loadRepo(root).states.slice().sort((left, right) =>
    left.createdAt.localeCompare(right.createdAt),
  );
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

export function promoteState(
  root: string,
  sourceStateId: string,
  kind: Extract<StateKind, "nice" | "star">,
  description?: string,
): StateRecord {
  const repo = loadRepo(root);
  const source = repo.states.find((state) => state.id === sourceStateId);
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
    tags: [...source.tags],
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
