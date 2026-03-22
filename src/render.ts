import type { LaneRecord, MapHit, RepoData, StateRecord, TimeshiftRecord } from "./types";
import type { AheadBehindStatus, WorktreeStatus } from "./git";
import { formatDate, pad, stateDisplayBranch } from "./utils";

const ANSI_RESET = "\u001b[0m";
const BRANCH_COLOR_PALETTE = [39, 42, 45, 69, 111, 150, 178, 208, 214];

export function renderStateSummary(state: StateRecord): string {
  return renderStateSummaryWithOptions(state);
}

export function renderStateSummaryWithOptions(
  state: StateRecord,
  options?: {
    includeLane?: boolean;
  },
): string {
  const parts = [
    `${state.id}`,
    `[${state.kind}]`,
    state.label,
    `branch=${stateDisplayBranch(state)}`,
    formatDate(state.createdAt),
  ];

  if (options?.includeLane !== false) {
    parts.splice(3, 0, `lane=${state.lane}`);
  }

  return parts.join(" ");
}

export function renderStateChoiceTable(states: StateRecord[]): string {
  if (states.length === 0) {
    return "";
  }

  const separator = "  ";
  const indexWidth = Math.max(2, String(states.length).length);
  const idWidth = Math.max(10, ...states.map((state) => state.id.length));
  const kindWidth = Math.max(6, ...states.map((state) => state.kind.length));
  const labelWidth = Math.max(18, ...states.map((state) => Math.min(state.label.length, 40)));
  const branchWidth = Math.max(
    20,
    ...states.map((state) => Math.min(stateDisplayBranch(state).length, 24)),
  );

  const lines = [
    `${pad("#", indexWidth)}${separator}${pad("id", idWidth)}${separator}${pad("kind", kindWidth)}${separator}${pad("label", labelWidth)}${separator}${pad("branch", branchWidth)}${separator}date`,
  ];

  states.forEach((state, index) => {
    lines.push(
      `${pad(String(index + 1), indexWidth)}${separator}${pad(state.id, idWidth)}${separator}${pad(state.kind, kindWidth)}${separator}${pad(truncate(state.label, 40), labelWidth)}${separator}${pad(truncate(stateDisplayBranch(state), 24), branchWidth)}${separator}${formatDate(state.createdAt)}`,
    );
  });

  return lines.join("\n");
}

export function renderGraph(
  repo: RepoData,
  options?: {
    currentStateId?: string | null;
    colorize?: boolean;
  },
): string {
  const sorted = repo.states
    .slice()
    .sort((left, right) => left.createdAt.localeCompare(right.createdAt));
  const children = new Map<string | null, StateRecord[]>();
  const latestByDisplayBranch = new Map<string, StateRecord>();

  for (const state of sorted) {
    latestByDisplayBranch.set(stateDisplayBranch(state), state);
  }

  const leafStateIds = new Set(
    Array.from(latestByDisplayBranch.values()).map((state) => state.id),
  );

  for (const state of sorted) {
    const parent = state.parentStateId;
    if (!children.has(parent)) {
      children.set(parent, []);
    }
    children.get(parent)!.push(state);
  }

  const lines: string[] = [];

  function walk(parentId: string | null, prefix: string): void {
    const nodes = children.get(parentId) ?? [];
    nodes.forEach((state, index) => {
      const isLast = index === nodes.length - 1;
      const connector = isLast ? "└─" : "├─";
      const currentMarker = state.id === options?.currentStateId ? "*" : " ";
      const isCurrent = state.id === options?.currentStateId;
      const isLeaf = leafStateIds.has(state.id);
      const leafMarker = isLeaf ? "^" : " ";
      const line = `${prefix}${connector} ${currentMarker}${leafMarker} ${state.id} [${state.kind}] ${state.label} (${stateDisplayBranch(state)})`;
      lines.push(
        colorizeBranchLine(
          line,
          stateDisplayBranch(state),
          options?.colorize === true,
          isLeaf,
          isCurrent,
        ),
      );
      walk(state.id, `${prefix}${isLast ? "   " : "│  "}`);
    });
  }

  walk(null, "");
  if (lines.length === 0) {
    return "No states saved yet.";
  }

  return ["* current state    ^ branch leaf", "", ...lines].join("\n");
}

export function renderStateTable(
  states: StateRecord[],
  options?: {
    colorize?: boolean;
    currentStateId?: string | null;
  },
): string {
  if (states.length === 0) {
    return "No states saved yet.";
  }

  const lines = [
    `${pad("id", 10)} ${pad("kind", 6)} ${pad("lane", 16)} ${pad("branch", 18)} label`,
  ];
  const latestByDisplayBranch = new Map<string, StateRecord>();

  for (const state of states) {
    latestByDisplayBranch.set(stateDisplayBranch(state), state);
  }
  const leafStateIds = new Set(
    Array.from(latestByDisplayBranch.values()).map((state) => state.id),
  );

  for (const state of states) {
    const line = `${pad(state.id, 10)} ${pad(state.kind, 6)} ${pad(stateDisplayBranch(state), 16)} ${pad(stateDisplayBranch(state), 18)} ${state.label}`;
    lines.push(
      colorizeBranchLine(
        line,
        stateDisplayBranch(state),
        options?.colorize === true,
        leafStateIds.has(state.id),
        state.id === options?.currentStateId,
      ),
    );
  }

  return lines.join("\n");
}

function colorizeBranchLine(
  line: string,
  branch: string,
  enabled: boolean,
  isLeaf: boolean,
  isCurrent: boolean,
): string {
  if (!enabled) {
    return line;
  }

  const color = branchAnsiColor(branch);
  const dim = isLeaf || isCurrent ? "" : "\u001b[2m";
  const bold = isCurrent ? "\u001b[1m" : "";
  return `${bold}${dim}\u001b[38;5;${color}m${line}${ANSI_RESET}`;
}

function branchAnsiColor(branch: string): number {
  if (branch === "main") {
    return 111;
  }

  let hash = 0;
  for (let index = 0; index < branch.length; index += 1) {
    hash = (hash * 31 + branch.charCodeAt(index)) >>> 0;
  }

  return BRANCH_COLOR_PALETTE[hash % BRANCH_COLOR_PALETTE.length] ?? 111;
}

function truncate(value: string, length: number): string {
  return value.length > length ? `${value.slice(0, length - 3)}...` : value;
}

export function renderStory(states: StateRecord[]): string {
  const memorable = states.filter((state) =>
    state.kind === "star" || state.kind === "nice"
  );

  if (memorable.length === 0) {
    return "No `nice` or `star` states yet.";
  }

  return memorable
    .map((state) =>
      `${state.id} [${state.kind}] ${state.label}\n  ${state.description}\n  ${formatDate(state.createdAt)} on ${stateDisplayBranch(state)}`
    )
    .join("\n\n");
}

export function renderDoctor(input: {
  root: string;
  branch: string;
  jjAvailable: boolean;
  lane: LaneRecord | null;
  stateCount: number;
  remoteConfigured: boolean;
}): string {
  const lines = [
    `safe space: ${input.root}`,
    `branch: ${input.branch}`,
    `jj available: ${input.jjAvailable ? "yes" : "no"}`,
    `current lane: ${input.lane ? input.lane.name : "none"}`,
    `saved states: ${input.stateCount}`,
    `origin remote: ${input.remoteConfigured ? "configured" : "missing"}`,
  ];

  return lines.join("\n");
}

export function renderMap(hits: MapHit[]): string {
  if (hits.length === 0) {
    return "No project markers found.";
  }

  return hits
    .map((hit) => `${hit.path}\n  ${hit.markers.join(", ")}`)
    .join("\n\n");
}

export function renderTimeshifts(timeshifts: TimeshiftRecord[]): string {
  if (timeshifts.length === 0) {
    return "No timeshifts saved yet.";
  }

  return timeshifts
    .map((entry) =>
      `${entry.id} ${entry.label} (${entry.branch}, ${entry.lane}) ${formatDate(entry.createdAt)}`
    )
    .join("\n");
}

export function renderLanes(lanes: LaneRecord[], currentBranch: string): string {
  if (lanes.length === 0) {
    return "No lanes recorded yet.";
  }

  return lanes
    .map((lane) => {
      const marker = lane.branch === currentBranch ? "*" : " ";
      return `${marker} ${lane.name} -> ${lane.branch} (base: ${lane.baseRef})`;
    })
    .join("\n");
}

export function renderStatus(input: {
  root: string;
  branch: string;
  headCommit: string | null;
  lane: LaneRecord | null;
  worktree: WorktreeStatus;
  latestState: StateRecord | null;
  stateCount: number;
  jjAvailable: boolean;
  remoteConfigured: boolean;
  aheadBehind: AheadBehindStatus | null;
}): string {
  const latest = input.latestState
    ? `${input.latestState.id} [${input.latestState.kind}] ${input.latestState.label}`
    : "none";
  const head = input.headCommit ? input.headCommit.slice(0, 12) : "unborn";
  const worktree = input.worktree.dirty
    ? `dirty (${input.worktree.changedFiles} files, staged=${input.worktree.staged}, unstaged=${input.worktree.unstaged}, untracked=${input.worktree.untracked})`
    : "clean";
  const upstream = input.aheadBehind
    ? `ahead=${input.aheadBehind.ahead} behind=${input.aheadBehind.behind}`
    : "no upstream";

  return [
    `safe space: ${input.root}`,
    `branch: ${input.branch}`,
    `head: ${head}`,
    `current lane: ${input.lane ? input.lane.name : "none"}`,
    `worktree: ${worktree}`,
    `latest state: ${latest}`,
    `saved states: ${input.stateCount}`,
    `jj available: ${input.jjAvailable ? "yes" : "no"}`,
    `origin remote: ${input.remoteConfigured ? "configured" : "missing"}`,
    `upstream: ${upstream}`,
  ].join("\n");
}
