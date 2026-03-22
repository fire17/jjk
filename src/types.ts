export type StateKind =
  | "new"
  | "save"
  | "stash"
  | "cherry"
  | "step"
  | "nice"
  | "star"
  | "auto";

export interface SnapshotStats {
  changedFiles: number;
  insertedLines?: number;
  deletedLines?: number;
}

export interface StateMetadata {
  gitCommit: string;
  message?: string;
  base?: string;
  cherry?: string;
  stashFromBranch?: string;
  stashFromStateId?: string;
  deletedAt?: string;
  deletedBranch?: string;
  deletedLocation?: {
    branch: string;
    lane: string;
    continuationBranch?: string | null;
    parentStateId: string | null;
    wasLaneCurrent: boolean;
  };
  priorContexts?: Array<{
    branch: string;
    lane: string;
    continuationBranch?: string | null;
    updatedAt: string;
  }>;
}

export interface StateRecord {
  id: string;
  kind: StateKind;
  label: string;
  description: string;
  createdAt: string;
  branch: string;
  lane: string;
  continuationBranch?: string | null;
  commit: string;
  parentCommit: string | null;
  parentStateId: string | null;
  tags: string[];
  stats: SnapshotStats;
  metadata?: StateMetadata;
}

export interface LaneRecord {
  name: string;
  branch: string;
  baseRef: string;
  createdAt: string;
  updatedAt: string;
  currentStateId: string | null;
}

export interface TimeshiftRecord {
  id: string;
  label: string;
  createdAt: string;
  branch: string;
  lane: string;
  stateId: string | null;
  relativeCwd: string;
  env: Record<string, string>;
}

export interface FreezeRecord {
  id: string;
  stateId: string;
  createdAt: string;
  bundlePath: string;
  manifestPath: string;
}

export interface RepoSettings {
  watchDebounceMs: number;
  autoStatePrefix: string;
  showWorkspaceSnapshotsInGit?: boolean;
}

export interface ReturnContext {
  stateId: string;
  sourceBranch: string;
  sourceLane: string;
}

export interface StateNavigationHistory {
  entries: string[];
  index: number;
}

export interface RepoData {
  version: 1;
  safeSpaceId: string;
  createdAt: string;
  updatedAt: string;
  settings: RepoSettings;
  states: StateRecord[];
  lanes: Record<string, LaneRecord>;
  branchLaneMap: Record<string, string>;
  allowMainBranchSave?: boolean;
  returnContext?: ReturnContext | null;
  currentStateHistory?: StateNavigationHistory;
  timeshifts: TimeshiftRecord[];
  freezes: FreezeRecord[];
}

export interface CommandContext {
  cwd: string;
}

export interface SaveStateRequest {
  kind: StateKind;
  description: string;
  label?: string;
  message?: string;
  metadata?: Omit<StateMetadata, "gitCommit">;
  tags?: string[];
}

export interface SaveStateResult {
  state: StateRecord;
  repo: RepoData;
}

export interface StateMatch {
  state: StateRecord;
  score: number;
}

export interface MapHit {
  path: string;
  markers: string[];
}
