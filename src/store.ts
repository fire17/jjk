import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import {
  createOrSwitchBranch,
  createSnapshotCommit,
  ensureLocalExcludes,
  getCurrentBranch,
  getHeadCommit,
  importIntoJj,
  initGitRepo,
  initJjRepo,
  isGitRepo,
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
      },
      states: [],
      lanes: {},
      branchLaneMap: {},
      timeshifts: [],
      freezes: [],
    };

    ensureLane(repo, branch, branch, branch);
    saveRepo(root, repo);
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

export function saveState(root: string, request: SaveStateRequest): SaveStateResult {
  const repo = loadRepo(root);
  const branch = getCurrentBranch(root);
  const headCommit = getHeadCommit(root);
  const lane = ensureLane(repo, branch, branch, branch);
  const description = ensureDescription(request.kind, request.description);
  const label = request.label ?? defaultLabel(request.kind, description);
  const snapshot = createSnapshotCommit(
    root,
    `jjk ${request.kind}: ${description}`,
  );

  const checkedOutStateId =
    headCommit
      ? repo.states.find((state) => state.commit === headCommit)?.id ?? null
      : null;
  const parentStateId =
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
  updateRef(root, `refs/jjk/states/${state.id}`, state.commit);
  saveRepo(root, repo);
  importIntoJj(root);

  return { state, repo };
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
  const branchName = `jjk/lane/${slugify(name)}`;
  const headCommit = getHeadCommit(root);
  const baseRef = headCommit ?? getCurrentBranch(root);

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
    label: `${kind} ${source.label}`.slice(0, 96),
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
