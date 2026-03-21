export type StateKind =
  | "save"
  | "step"
  | "nice"
  | "star"
  | "auto";

export interface SnapshotStats {
  changedFiles: number;
  insertedLines?: number;
  deletedLines?: number;
}

export interface StateRecord {
  id: string;
  kind: StateKind;
  label: string;
  description: string;
  createdAt: string;
  branch: string;
  lane: string;
  commit: string;
  parentCommit: string | null;
  parentStateId: string | null;
  tags: string[];
  stats: SnapshotStats;
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
