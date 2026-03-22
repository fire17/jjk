import type { LaneRecord, MapHit, RepoData, StateRecord, TimeshiftRecord } from "./types";
import type { AheadBehindStatus, WorktreeStatus } from "./git";
import {
  formatDate,
  isDeletedState,
  pad,
  shortCommit,
  shortStateId,
  stateHasTag,
  stateHasStar,
  stateDisplayBranch,
  stateGitCommit,
  stateMessage,
} from "./utils";

const ANSI_RESET = "\u001b[0m";
const STATE_GRAPH_LABEL_MESSAGE_MAX_LENGTH = 120;
const STATE_TABLE_LABEL_MESSAGE_MAX_LENGTH = 72;
const BRANCH_COLOR_PALETTE = [
  31, 32, 33, 37, 38, 39, 43, 44, 45, 68,
  69, 74, 75, 80, 81, 104, 105, 110, 111, 136,
  142, 143, 149, 150, 172, 173, 174, 179, 180, 181,
  203, 204, 205, 206, 207, 208, 209, 214, 215, 221,
];

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
    shortStateId(state.id),
    `git=${shortCommit(stateGitCommit(state))}`,
    `[${state.kind}]`,
    state.label,
    `branch=${stateDisplayBranch(state)}`,
    formatDate(state.createdAt),
  ];

  if (options?.includeLane !== false) {
    parts.splice(3, 0, `lane=${state.lane}`);
  }

  return appendStateMetadata(parts.join(" "), state);
}

export function renderStateInspection(repo: RepoData, state: StateRecord): string {
  const parent = state.parentStateId
    ? repo.states.find((candidate) => candidate.id === state.parentStateId) ?? null
    : null;
  const children = repo.states
    .filter((candidate) => candidate.parentStateId === state.id && !isDeletedState(candidate))
    .sort((left, right) => left.createdAt.localeCompare(right.createdAt));
  const metadata = state.metadata ?? {};
  const lines = [
    renderStateSummary(state),
    `description: ${state.description}`,
    `message: ${stateMessage(state) ?? "-"}`,
    `branch: ${state.branch}`,
    `display branch: ${stateDisplayBranch(state)}`,
    `lane: ${state.lane}`,
    `parent: ${parent ? `${shortStateId(parent.id)} ${parent.label}` : "-"}`,
    `children: ${children.length > 0 ? children.map((child) => `${shortStateId(child.id)} ${child.label}`).join(", ") : "-"}`,
    `tags: ${state.tags.length > 0 ? state.tags.join(", ") : "-"}`,
    `git: ${shortCommit(stateGitCommit(state))}`,
    `base: ${shortLinkedStateId(metadata.base)}`,
    `cherry: ${shortLinkedStateId(metadata.cherry)}`,
    `created: ${formatDate(state.createdAt)}`,
  ];

  if (metadata.deletedAt || metadata.deletedBranch || metadata.deletedLocation) {
    lines.push(`deleted: yes`);
    if (metadata.deletedBranch) {
      lines.push(`deleted branch: ${metadata.deletedBranch}`);
    }
    if (metadata.deletedAt) {
      lines.push(`deleted at: ${formatDate(metadata.deletedAt)}`);
    }
  } else {
    lines.push("deleted: no");
  }

  if (metadata.stashFromBranch) {
    lines.push(`stash from branch: ${metadata.stashFromBranch}`);
  }
  if (metadata.stashFromStateId) {
    lines.push(`stash from state: ${shortStateId(metadata.stashFromStateId)}`);
  }
  if (metadata.priorContexts && metadata.priorContexts.length > 0) {
    lines.push(`prior contexts: ${metadata.priorContexts.length}`);
  }

  lines.push(
    `stats: changed=${state.stats.changedFiles}` +
      `${state.stats.insertedLines !== undefined ? ` inserted=${state.stats.insertedLines}` : ""}` +
      `${state.stats.deletedLines !== undefined ? ` deleted=${state.stats.deletedLines}` : ""}`,
  );

  return lines.join("\n");
}

function appendStateMessage(text: string, state: StateRecord): string {
  const message = stateMessage(state);
  return message ? `${text} | ${message}` : text;
}

function appendStateMetadata(text: string, state: StateRecord): string {
  const metadata = state.metadata;
  if (!metadata) {
    return text;
  }

  const parts = [
    metadata.status ? `status=${metadata.status}` : "",
    metadata.assignee ? `assignee=${metadata.assignee}` : "",
    metadata.note ? `note=${truncate(singleLine(metadata.note), 60)}` : "",
    metadata.handoff ? `handoff=${truncate(singleLine(metadata.handoff), 60)}` : "",
    metadata.publishedAt ? `published=${formatDate(metadata.publishedAt)}` : "",
    metadata.quarantinedAt ? `quarantine=${formatDate(metadata.quarantinedAt)}` : "",
  ].filter(Boolean);

  return parts.length > 0 ? `${text} | ${parts.join(" | ")}` : text;
}

function singleLine(value: string): string {
  return value.replace(/\s+/g, " ").trim();
}

function formatStateTableLabelMessage(state: StateRecord): string {
  return truncate(
    singleLine(appendStateMessage(stateLabelWithMarkers(state), state)),
    STATE_TABLE_LABEL_MESSAGE_MAX_LENGTH,
  );
}

function formatStateGraphLine(text: string, state: StateRecord): string {
  const message = stateMessage(state);
  if (!message) {
    return text;
  }

  const separator = " | ";
  const maxMessageLength = Math.max(
    3,
    STATE_GRAPH_LABEL_MESSAGE_MAX_LENGTH - text.length - separator.length,
  );
  return `${text}${separator}${truncate(singleLine(message), maxMessageLength)}`;
}

function stateLabelWithMarkers(
  state: StateRecord,
  options?: {
    includeStar?: boolean;
  },
): string {
  const markers = [
    options?.includeStar === false ? "" : stateHasStar(state) ? "★" : "",
    stateHasTag(state, "thumbsup") ? "👍" : "",
    stateHasTag(state, "thumbsdown") ? "👎" : "",
    stateHasTag(state, "pin") ? "📌" : "",
  ].filter(Boolean);
  const prefix = markers.length > 0 ? `${markers.join("")} ` : "";
  return `${prefix}${state.label}`;
}

function shortLinkedStateId(stateId: string | undefined): string {
  return stateId ? shortStateId(stateId) : "-";
}

export function renderStateChoiceTable(
  states: StateRecord[],
  options?: {
    colorize?: boolean;
  },
): string {
  if (states.length === 0) {
    return "";
  }

  const separator = "  ";
  const indexWidth = Math.max(2, String(states.length).length);
  const idWidth = Math.max(8, ...states.map((state) => shortStateId(state.id).length));
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
    const line = `${pad(String(index + 1), indexWidth)}${separator}${pad(shortStateId(state.id), idWidth)}${separator}${pad(state.kind, kindWidth)}${separator}${pad(truncate(state.label, 40), labelWidth)}${separator}${pad(truncate(stateDisplayBranch(state), 24), branchWidth)}${separator}${formatDate(state.createdAt)}`;
    lines.push(
      colorizeBranchLine(
        line,
        stateDisplayBranch(state),
        options?.colorize === true,
        true,
        false,
      ),
    );
  });

  return lines.join("\n");
}

export function renderGraph(
  repo: RepoData,
  options?: {
    currentStateId?: string | null;
    colorize?: boolean;
    includeDeleted?: boolean;
  },
): string {
  const allStates = repo.states
    .slice()
    .sort((left, right) => left.createdAt.localeCompare(right.createdAt));
  const visibleStateIds = new Set(
    allStates
      .filter((state) => options?.includeDeleted === true || !isDeletedState(state))
      .map((state) => state.id),
  );
  const children = new Map<string | null, StateRecord[]>();
  const leafStateIds = resolveBranchLeafStateIds(
    allStates.filter((state) => visibleStateIds.has(state.id)),
    repo,
  );
  const stateById = new Map(allStates.map((state) => [state.id, state] as const));

  function visibleParentId(state: StateRecord): string | null {
    let parentId = state.parentStateId;
    while (parentId) {
      if (visibleStateIds.has(parentId)) {
        return parentId;
      }
      parentId = stateById.get(parentId)?.parentStateId ?? null;
    }
    return null;
  }

  for (const state of allStates) {
    if (!visibleStateIds.has(state.id)) {
      continue;
    }
    const parent = visibleParentId(state);
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
      const starMarker = stateHasStar(state) ? "★ " : "  ";
      const currentMarker = state.id === options?.currentStateId ? "*" : " ";
      const isCurrent = state.id === options?.currentStateId;
      const isLeaf = leafStateIds.has(state.id);
      const leafMarker = isLeaf ? "^" : " ";
      const line = formatStateGraphLine(
        `${prefix}${connector} ${currentMarker}${leafMarker} ${starMarker}${shortStateId(state.id)} [${state.kind}] ${stateLabelWithMarkers(state, { includeStar: false })} (${stateDisplayBranch(state)})`,
        state,
      );
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

  return ["★ starred    * current state    ^ branch leaf", "", ...lines].join("\n");
}

export function renderLogGraph(
  repo: RepoData,
  options?: {
    currentStateId?: string | null;
    colorize?: boolean;
    includeDeleted?: boolean;
  },
): string {
  const visibleStates = repo.states
    .filter((state) => options?.includeDeleted === true || !isDeletedState(state))
    .slice()
    .sort((left, right) => right.createdAt.localeCompare(left.createdAt));

  if (visibleStates.length === 0) {
    return "No states saved yet.";
  }

  const visibleStateIds = new Set(visibleStates.map((state) => state.id));
  const stateById = new Map(visibleStates.map((state) => [state.id, state] as const));
  const leafStateIds = resolveBranchLeafStateIds(visibleStates.slice().reverse(), repo);

  function visibleParentId(state: StateRecord): string | null {
    let parentId = state.parentStateId;
    while (parentId) {
      if (visibleStateIds.has(parentId)) {
        return parentId;
      }
      parentId = stateById.get(parentId)?.parentStateId ?? null;
    }
    return null;
  }

  const active: string[] = [];
  const lines: string[] = [];

  for (const state of visibleStates) {
    if (!active.includes(state.id)) {
      active.push(state.id);
    }

    const column = active.indexOf(state.id);
    const graphPrefix = active
      .map((_, index) => (index === column ? "*" : "|"))
      .join(" ");
    const decorations: string[] = [];
    if (state.id === options?.currentStateId) {
      decorations.push("current");
    }
    if (leafStateIds.has(state.id)) {
      decorations.push("leaf");
    }
    const decorationText = decorations.length > 0 ? ` [${decorations.join(", ")}]` : "";
    const line = formatStateGraphLine(
      `${graphPrefix} ${shortStateId(state.id)} [${state.kind}] ${stateLabelWithMarkers(state)} (${stateDisplayBranch(state)})${decorationText}`,
      state,
    );
    lines.push(
      colorizeBranchLine(
        line,
        stateDisplayBranch(state),
        options?.colorize === true,
        leafStateIds.has(state.id),
        state.id === options?.currentStateId,
      ),
    );

    const parentId = visibleParentId(state);
    if (!parentId) {
      active.splice(column, 1);
      continue;
    }

    const parentColumn = active.indexOf(parentId);
    if (parentColumn === -1) {
      active[column] = parentId;
      continue;
    }

    active.splice(column, 1);
  }

  return lines.join("\n");
}

export function renderStateTable(
  states: StateRecord[],
  options?: {
    colorize?: boolean;
    currentStateId?: string | null;
    repo?: RepoData;
    includeDeleted?: boolean;
  },
): string {
  const visibleStates = states.filter((state) => options?.includeDeleted === true || !isDeletedState(state));
  if (visibleStates.length === 0) {
    return "No states saved yet.";
  }

  const separator = "  ";
  const idWidth = Math.max(8, "id".length, ...visibleStates.map((state) => shortStateId(state.id).length));
  const gitWidth = Math.max(
    8,
    "git".length,
    ...visibleStates.map((state) => shortCommit(stateGitCommit(state), 8).length),
  );
  const kindWidth = Math.max(6, "kind".length, ...visibleStates.map((state) => state.kind.length));
  const laneWidth = Math.max(4, "lane".length, ...visibleStates.map((state) => state.lane.length));
  const branchWidth = Math.max(
    6,
    "branch".length,
    ...visibleStates.map((state) => stateDisplayBranch(state).length),
  );
  const labelWidth = Math.max(
    "label | message".length,
    ...visibleStates.map((state) => formatStateTableLabelMessage(state).length),
  );
  const dateWidth = Math.max(
    8,
    "datetime".length,
    ...visibleStates.map((state) => formatDate(state.createdAt).length),
  );
  const baseWidth = Math.max(4, "base".length, ...visibleStates.map((state) => shortLinkedStateId(state.metadata?.base).length));
  const cherryWidth = Math.max(
    6,
    "cherry".length,
    ...visibleStates.map((state) => shortLinkedStateId(state.metadata?.cherry).length),
  );
  const lines = [
    `${pad("id", idWidth)}${separator}${pad("git", gitWidth)}${separator}${pad("kind", kindWidth)}${separator}${pad("lane", laneWidth)}${separator}${pad("branch", branchWidth)}${separator}${pad("label | message", labelWidth)}${separator}${pad("base", baseWidth)}${separator}${pad("cherry", cherryWidth)}${separator}${pad("datetime", dateWidth)}`,
  ];
  const leafStateIds = resolveBranchLeafStateIds(visibleStates, options?.repo);

  for (const state of visibleStates) {
    const labelText = formatStateTableLabelMessage(state);
    const lineWithLinks = `${pad(shortStateId(state.id), idWidth)}${separator}${pad(shortCommit(stateGitCommit(state), 8), gitWidth)}${separator}${pad(state.kind, kindWidth)}${separator}${pad(state.lane, laneWidth)}${separator}${pad(stateDisplayBranch(state), branchWidth)}${separator}${pad(labelText, labelWidth)}${separator}${pad(shortLinkedStateId(state.metadata?.base), baseWidth)}${separator}${pad(shortLinkedStateId(state.metadata?.cherry), cherryWidth)}${separator}${pad(formatDate(state.createdAt), dateWidth)}`;
    lines.push(
      colorizeBranchLine(
        lineWithLinks,
        stateDisplayBranch(state),
        options?.colorize === true,
        leafStateIds.has(state.id),
        state.id === options?.currentStateId,
      ),
    );
  }

  return lines.join("\n");
}

function resolveBranchLeafStateIds(
  states: StateRecord[],
  repo?: RepoData,
): Set<string> {
  const latestByDisplayBranch = new Map<string, StateRecord>();

  for (const state of states) {
    latestByDisplayBranch.set(stateDisplayBranch(state), state);
  }

  if (repo) {
    for (const [branch, laneName] of Object.entries(repo.branchLaneMap)) {
      const lane = repo.lanes[laneName];
      const stateId = lane?.currentStateId;
      if (!stateId) {
        continue;
      }
      const state = states.find((candidate) => candidate.id === stateId);
      if (!state) {
        continue;
      }
      if (stateDisplayBranch(state) !== branch) {
        continue;
      }
      latestByDisplayBranch.set(branch, state);
    }
  }

  return new Set(Array.from(latestByDisplayBranch.values()).map((state) => state.id));
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

  let hash = 2166136261;
  for (let index = 0; index < branch.length; index += 1) {
    hash ^= branch.charCodeAt(index);
    hash = Math.imul(hash, 16777619) >>> 0;
  }

  const mixed = scrambleHash(hash ^ 0x9e3779b9);
  return BRANCH_COLOR_PALETTE[mixed % BRANCH_COLOR_PALETTE.length] ?? 111;
}

function truncate(value: string, length: number): string {
  return value.length > length ? `${value.slice(0, length - 3)}...` : value;
}

function scrambleHash(value: number): number {
  let hash = value >>> 0;
  hash ^= hash >>> 16;
  hash = Math.imul(hash, 0x7feb352d) >>> 0;
  hash ^= hash >>> 15;
  hash = Math.imul(hash, 0x846ca68b) >>> 0;
  hash ^= hash >>> 16;
  return hash >>> 0;
}

export function renderStory(states: StateRecord[]): string {
  const memorable = states.filter((state) =>
    state.kind === "nice" || stateHasStar(state)
  );

  if (memorable.length === 0) {
    return "No `nice` or `star` states yet.";
  }

  return memorable
    .map((state) =>
      `${shortStateId(state.id)} [${state.kind}] ${state.label}\n  ${state.description}\n  ${formatDate(state.createdAt)} on ${stateDisplayBranch(state)}`
    )
    .join("\n\n");
}

export function renderCurrentState(input: {
  state: StateRecord;
  parentState: StateRecord | null;
  workspaceBranch: string | null;
  historyIndex: number;
  historyLength: number;
}): string {
  const parent = input.parentState
    ? `${shortStateId(input.parentState.id)} [${input.parentState.kind}] ${input.parentState.label}`
    : "none";
  const workspace = input.workspaceBranch ?? "detached";

  return [
    `current state: ${shortStateId(input.state.id)} [${input.state.kind}] ${input.state.label}`,
    `description: ${input.state.description}`,
    `lane: ${input.state.lane}`,
    `branch: ${stateDisplayBranch(input.state)}`,
    `workspace: ${workspace}`,
    `git: ${shortCommit(stateGitCommit(input.state))}`,
    `parent: ${parent}`,
    `saved at: ${formatDate(input.state.createdAt)}`,
    `history: ${input.historyIndex + 1}/${input.historyLength}`,
  ].join("\n");
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
    ? `${shortStateId(input.latestState.id)} [${input.latestState.kind}] ${input.latestState.label}`
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
