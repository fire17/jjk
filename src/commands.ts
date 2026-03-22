import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline/promises";
import { stdin, stdout } from "node:process";
import { fileURLToPath } from "node:url";
import {
  addWorktree,
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
  isGitIgnored,
  isJjRepo,
  localBranchExists,
  pickStateChanges,
  getStateChangedFiles,
  getStatePatch,
  pullFastForward,
  pruneJjKeepRefs,
  pushCurrentBranchAndStateRefs,
  revertStateChanges,
  switchBranch,
  switchToDetachedCommit,
  worktreeMatchesCommit,
} from "./git";
import {
  renderDoctor,
  renderGraph,
  renderLogGraph,
  renderCurrentState,
  renderLanes,
  renderMap,
  renderStateChoiceTable,
  renderStateInspection,
  renderStateSummary,
  renderStatus,
  renderStateTable,
  renderStory,
  renderTimeshifts,
} from "./render";
import {
  amendState,
  createBackup,
  attachBranchToState,
  branchFromState,
  createLane,
  createBranchAtState,
  deleteState,
  eraseState,
  ensureWorkspaceSnapshot,
  initSafeSpace,
  JJK_DIR,
  listLanes,
  listStates,
  listTimeshifts,
  loadBackup,
  loadRepo,
  moveState,
  noteState,
  pinState,
  getWorkspaceSnapshotHistory,
  listWorkspaceSnapshots,
  promoteState,
  recordWorkspaceSnapshot,
  redoWorkspaceSnapshot,
  recoverState,
  recordFreeze,
  renameBranch,
  renameState,
  rememberTimeshift,
  requireSafeSpace,
  resolveLane,
  resolveLatestStateForBranch,
  resolveState,
  resolveTimeshift,
  resolveBackupPath,
  saveState,
  saveRepo,
  isTipStateOnBranch,
  annotateState,
  listAliases,
  removeAlias,
  setAlias,
  setBranchLock,
  setDefaultBranch,
  isBranchLocked,
  starState,
  toggleStateTag,
  splitState,
  unstarState,
  unpinState,
  stashWorkspace,
  undoWorkspaceSnapshot,
  updateBranchTarget,
} from "./store";
import type { MapHit, RepoData, SaveStateRequest, StateRecord } from "./types";
import {
  branchSegment,
  continuationBranchName,
  formatDate,
  formatRelativePath,
  isDeletedState,
  findStateMatches,
  nowIso,
  parseStateLabelAndMessage,
  shortStateId,
  shortCommit,
  stateDisplayBranch,
  stateHasStar,
  stateHasTag,
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
  jjk where
  jjk <description>
  jjk save [description]
  jjk step [description]
  jjk nice [description]
  jjk star [state]
  jjk unstar [state]
  jjk pin <state>
  jjk unpin <state>
  jjk thumbsup [state]
  jjk thumbsdown [state]
  jjk note <state>, <message>
  jjk stash [description]
  jjk inspect <state>
  jjk search <query>
  jjk timeline
  jjk see [--deleted] [--kind <kind>] [--tag <tag>] [--since <time>]
  jjk graph [--deleted] [--branch <branch>]
  jjk favorites
  jjk show [state]
  jjk patch [state]
  jjk files [state]
  jjk touched <branch>
  jjk story
  jjk diff [--atomic] [state] [state]
  jjk log <branch>
  jjk git log
  jjk delete <state>
  jjk recover <deleted-state>
  jjk undo [-rm] [-y]
  jjk redo
  jjk archive <state>
  jjk quarantine <state>
  jjk open <state>
  jjk copy-id <query>
  jjk recent [limit]
  jjk aliases [add <name> <query>]
  jjk default-branch <branch>
  jjk config
  jjk mark <state> <status>
  jjk assign-note <state>, <person/note>
  jjk ready <state>
  jjk publish <state>
  jjk handoff <state>
  jjk checkpoint [description]
  jjk autosave now
  jjk lock <branch>
  jjk unlock <branch>
  jjk clean
  jjk gc
  jjk pick <state>
  jjk replay <state> onto <branch>
  jjk merge-state <state> into <branch>
  jjk revert-state <state>
  jjk promote <state> <nice|star>
  jjk compare-branch <a> <b>
  jjk move <state> <branch>
  jjk split <state> <new-branch>
  jjk branch-from <state> <label>
  jjk rename-state <state> <new-label>
  jjk rename-branch <old> <new>
  jjk backup [label]
  jjk backups
  jjk snapshot-log
  jjk load <backupfile>
  jjk restore <backupfile> [--preview]
  jjk export <state> <file>
  jjk import <file>
  jjk return <state>
  jjk lastest <branch>
  jjk continue
  jjk return -
  jjk back
  jjk forward
  jjk up
  jjk down
  jjk prev
  jjk next
  jjk root <state>
  jjk trail <state>
  jjk children <state>
  jjk parents <state>
  jjk heads
  jjk update <branch> [state]
  jjk branch [name]
  jjk checkout <branch>
  jjk fork <name> [--worktree]
  jjk worktree [state]

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
  jjk shell-init [zsh|bash]

Examples:
  Basic:
    jjk init
    jjk save "main checkpoint"
    jjk see
    jjk graph
    jjk timeline
    jjk where

  Branching:
    jjk green
    jjk purple
    jjk return green
    jjk orange
    jjk continue
    jjk branch-from purple review_lane
    jjk move purple jjk/review_lane
    jjk rename-state purple polished_purple
    jjk rename-branch jjk/green jjk/green_experiment

  Review markers:
    jjk star
    jjk thumbsup purple
    jjk thumbsdown fast_orange
    jjk favorites

  Compare and inspect:
    jjk inspect purple
    jjk search purple
    jjk show purple
    jjk patch purple
    jjk files purple
    jjk touched jjk/purple
    jjk diff purple orange
    jjk diff --atomic purple fast_purple
    jjk compare-branch jjk/green jjk/orange
    jjk log jjk/purple
    jjk git log

  Recovery:
    jjk backup before_refactor
    jjk backups
    jjk snapshot-log
    jjk restore before_refactor --preview
    jjk undo
    jjk redo
    jjk load before_refactor

  Advanced flow:
    jjk return orange
    jjk pick fast_purple
    jjk nice fast_orange
    jjk replay fast_purple onto jjk/orange
    jjk merge-state fast_purple into jjk/orange
    jjk revert-state orange
    jjk update jjk/purple purple
    jjk branch review_lane
    jjk fork review_slice
    jjk fork --worktree
    jjk worktree purple
    jjk checkout jjk/purple
    jjk graph --branch jjk/purple
    jjk see --kind new
    jjk see --tag star
    jjk see --since 2026-03-22T00:00:00Z
    jjk root purple
    jjk trail purple
    jjk children purple
    jjk parents purple
    jjk heads

  Shell integration:
    eval "$(jjk shell-init zsh)"
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

async function confirmAction(prompt: string): Promise<void> {
  if (!process.stdin.isTTY) {
    throw new Error("Confirmation required. Re-run with `-y` or `-rm` to skip the prompt.");
  }

  const rl = createInterface({ input: stdin, output: stdout });
  const answer = await rl.question(`${prompt} [y/N]: `);
  rl.close();
  if (!/^y(es)?$/i.test(answer.trim())) {
    throw new Error("Cancelled.");
  }
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
  recordWorkspaceSnapshot(root, `${request.kind}:${request.label ?? request.description}`);
  console.log(renderStateSummary(result.state));
}

function buildSaveRequest(kind: SaveStateRequest["kind"], input: string): SaveStateRequest {
  return {
    kind,
    ...parseStateLabelAndMessage(input),
  };
}

interface StateViewFilters {
  includeDeleted: boolean;
  kind?: string;
  tag?: string;
  since?: number;
  branch?: string;
}

function parseStateViewFilters(
  args: string[],
  options?: {
    allowBranch?: boolean;
  },
): StateViewFilters {
  const filters: StateViewFilters = {
    includeDeleted: false,
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--deleted") {
      filters.includeDeleted = true;
      continue;
    }
    if (arg === "--kind") {
      const value = args[++index];
      if (!value) {
        throw new Error("Usage: flag `--kind` requires a value.");
      }
      filters.kind = value.trim();
      continue;
    }
    if (arg === "--tag") {
      const value = args[++index];
      if (!value) {
        throw new Error("Usage: flag `--tag` requires a value.");
      }
      filters.tag = value.trim();
      continue;
    }
    if (arg === "--since") {
      const value = args[++index];
      if (!value) {
        throw new Error("Usage: flag `--since` requires a value.");
      }
      const timestamp = Date.parse(value);
      if (Number.isNaN(timestamp)) {
        throw new Error(`Invalid date value for --since: ${value}`);
      }
      filters.since = timestamp;
      continue;
    }
    if (options?.allowBranch && arg === "--branch") {
      const value = args[++index];
      if (!value) {
        throw new Error("Usage: flag `--branch` requires a value.");
      }
      filters.branch = value.trim();
      continue;
    }

    throw new Error(`Unknown flag: ${arg}`);
  }

  return filters;
}

function filterStatesForView(states: StateRecord[], filters: StateViewFilters): StateRecord[] {
  return states.filter((state) => {
    if (!filters.includeDeleted && isDeletedState(state)) {
      return false;
    }
    if (filters.kind && state.kind !== filters.kind) {
      return false;
    }
    if (filters.tag) {
      if (filters.tag === "star") {
        if (!stateHasStar(state)) {
          return false;
        }
      } else if (!stateHasTag(state, filters.tag)) {
        return false;
      }
    }
    if (filters.since !== undefined && new Date(state.createdAt).getTime() < filters.since) {
      return false;
    }
    if (filters.branch) {
      const branch = stateDisplayBranch(state);
      if (branch !== filters.branch && state.branch !== filters.branch && state.lane !== filters.branch) {
        return false;
      }
    }
    return true;
  });
}

function resolveBranchQuery(root: string, query: string): string {
  const resolvedLane = resolveLane(root, query);
  return resolvedLane?.branch ?? normalizeBranchName(query);
}

function splitNoteArgs(input: string): { state: string; message: string } {
  const separator = input.indexOf(",");
  if (separator <= 0) {
    throw new Error("Usage: jjk note <state>, <message>");
  }

  const state = input.slice(0, separator).trim();
  const message = input.slice(separator + 1).trim();
  if (!state || !message) {
    throw new Error("Usage: jjk note <state>, <message>");
  }
  return { state, message };
}

function shouldColorizeOutput(): boolean {
  return Boolean(process.stdout.isTTY);
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function shellSingleQuote(value: string): string {
  return `'${value.replace(/'/g, `'\"'\"'`)}'`;
}

function shellWrapperPath(): string {
  return resolve(dirname(fileURLToPath(import.meta.url)), "..", "bin", "jjk");
}

function renderShellInit(shell: string): string {
  const normalizedShell = shell.trim().toLowerCase();
  if (normalizedShell !== "zsh" && normalizedShell !== "bash") {
    throw new Error("Usage: jjk shell-init [zsh|bash]");
  }
  const binaryPath = shellSingleQuote(shellWrapperPath());
  return `jjk() {
  local __jjk_output __jjk_status __jjk_cd
  __jjk_output="$(JJK_CD_SENTINEL=1 ${binaryPath} "$@" 2>&1)"
  __jjk_status=$?
  __jjk_cd="$(printf '%s\\n' "$__jjk_output" | sed -n 's/^${CD_MARKER}//p' | tail -n 1)"
  printf '%s\\n' "$__jjk_output" | sed '/^${CD_MARKER}/d'
  if [ $__jjk_status -eq 0 ] && [ -n "$__jjk_cd" ]; then
    builtin cd "$__jjk_cd"
  fi
  return $__jjk_status
}`;
}

function emitDirectoryChange(path: string): void {
  if (process.env.JJK_CD_SENTINEL === "1") {
    console.log(`${CD_MARKER}${path}`);
  }
}

function tryResolveState(root: string, query: string): StateRecord | null {
  const trimmed = query.trim();
  if (trimmed.length === 0) {
    return null;
  }
  try {
    return resolveState(root, trimmed);
  } catch {
    return null;
  }
}

function uniqueBranchName(root: string, preferred: string): string {
  let candidate = preferred;
  let suffix = 2;
  while (localBranchExists(root, candidate)) {
    candidate = `${preferred}_${suffix}`;
    suffix += 1;
  }
  return candidate;
}

function uniqueWorktreePath(root: string, branch: string): string {
  const worktreesRoot = join(root, ".worktrees");
  mkdirSync(worktreesRoot, { recursive: true });
  const base = branchSegment(branch);
  let candidate = join(worktreesRoot, base);
  let suffix = 2;
  while (existsSync(candidate)) {
    candidate = join(worktreesRoot, `${base}-${suffix}`);
    suffix += 1;
  }
  return candidate;
}

function ensureWorktreeSharesJjkStore(root: string, worktreePath: string): void {
  const worktreeStorePath = join(worktreePath, JJK_DIR);
  if (existsSync(worktreeStorePath)) {
    return;
  }
  symlinkSync(resolve(root, JJK_DIR), worktreeStorePath, "dir");
}

function createBranchWorktree(
  root: string,
  state: StateRecord,
  preferredBranch: string,
): {
  branch: string;
  path: string;
  state: StateRecord;
} {
  const branch = uniqueBranchName(root, preferredBranch);
  if (isBranchLocked(root, branch)) {
    throw new Error(`Branch \`${branch}\` is locked.`);
  }
  const path = uniqueWorktreePath(root, branch);
  addWorktree(root, path, branch, {
    createBranch: true,
    startPoint: state.commit,
  });
  ensureWorktreeSharesJjkStore(root, path);
  attachBranchToState(root, branch, state.id);
  return { branch, path, state };
}

function createStateWorktree(
  root: string,
  state: StateRecord,
  kind: "fork" | "worktree",
): {
  branch: string;
  path: string;
  state: StateRecord;
} {
  const suffix = kind === "fork" ? "fork" : "worktree";
  return createBranchWorktree(root, state, continuationBranchName(`${state.label}_${suffix}`));
}

function printWorktreeReady(root: string, input: {
  branch: string;
  path: string;
  state: StateRecord;
}): void {
  console.log(`worktree ready: ${formatRelativePath(root, input.path)}`);
  console.log(`branch: ${input.branch}`);
  console.log(`state: ${shortStateId(input.state.id)} ${input.state.label}`);
  emitDirectoryChange(input.path);
}

function normalizeBranchName(input: string): string {
  const trimmed = input.trim();
  if (trimmed.length === 0) {
    throw new Error("Provide a branch name.");
  }
  if (trimmed === "main" || trimmed.startsWith("jjk/")) {
    return trimmed;
  }
  return continuationBranchName(trimmed);
}

function renderBranchList(root: string): string {
  const repo = loadRepo(root);
  const currentBranch = getCurrentBranchName(root);
  const detachedHead = currentBranch === null ? getHeadCommit(root)?.slice(0, 8) ?? "unknown" : null;
  const localBranches = Object.keys(fetchBranchRefsForDisplay(root, repo));
  const visibleBranches = localBranches
    .filter((branch) =>
      branch === "main" ||
      branch.startsWith("jjk/") ||
      Boolean(repo.branchLaneMap[branch]),
    )
    .sort((left, right) => left.localeCompare(right));

  const lines = visibleBranches.map((branch) => `${branch === currentBranch ? "*" : " "} ${branch}`);
  if (detachedHead) {
    lines.unshift(`* (detached at ${detachedHead})`);
  }
  return lines.join("\n");
}

function fetchBranchRefsForDisplay(root: string, repo: RepoData): Record<string, string> {
  const refs = new Map<string, string>(Object.entries(repo.branchLaneMap).map(([branch]) => [branch, ""]));
  for (const state of repo.states) {
    refs.set(state.branch, "");
    if (state.continuationBranch) {
      refs.set(state.continuationBranch, "");
    }
  }
  return Object.fromEntries(
    Object.keys({ ...Object.fromEntries(refs) }).map((branch) => [
      branch,
      localBranchExists(root, branch) ? branch : "",
    ]),
  );
}

const EMPTY_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const CD_MARKER = "__JJK_CD__=";

function runGitTextCommand(
  root: string,
  args: string[],
  emptyMessage: string,
  options?: {
    colorize?: boolean;
  },
): string {
  const gitArgs = options?.colorize === undefined
    ? args
    : [
      args[0] ?? "",
      `--color=${options.colorize ? "always" : "never"}`,
      ...args.slice(1),
    ];
  const proc = Bun.spawnSync(["git", ...gitArgs], {
    cwd: root,
    stdout: "pipe",
    stderr: "pipe",
  });
  const output = proc.stdout.toString().trim();
  if (proc.exitCode !== 0 && proc.exitCode !== 1) {
    const details = [proc.stderr.toString().trim(), output].filter(Boolean).join("\n");
    throw new Error(details.length > 0 ? details : `git ${gitArgs.join(" ")} failed`);
  }
  return output.length > 0 ? output : emptyMessage;
}

function resolveDefaultState(root: string): StateRecord {
  const repo = loadRepo(root);
  const defaultBranch = repo.settings.defaultBranch?.trim() ?? "";
  if (defaultBranch.length > 0) {
    const defaultState = resolveLatestStateForBranch(root, defaultBranch);
    if (defaultState) {
      return defaultState;
    }
  }

  const currentState = resolveCurrentState(root, repo.states) ?? repo.states[repo.states.length - 1] ?? null;
  if (!currentState) {
    throw new Error("No state is available.");
  }
  return currentState;
}

function resolveBranchName(root: string, query: string): string {
  const lane = resolveLane(root, query);
  return lane?.branch ?? query.trim();
}

function collectStateTrail(root: string, state: StateRecord): StateRecord[] {
  const repo = loadRepo(root);
  const trail = [state];
  let cursor = state;

  while (cursor.parentStateId) {
    const parent = repo.states.find(
      (candidate) => candidate.id === cursor.parentStateId && !isDeletedState(candidate),
    );
    if (!parent) {
      break;
    }
    trail.push(parent);
    cursor = parent;
  }

  return trail.reverse();
}

function renderStateTrail(trail: StateRecord[]): string {
  if (trail.length === 0) {
    return "No states available.";
  }

  return trail
    .map((state, index) => {
      const indent = "  ".repeat(index);
      const prefix = index === 0 ? "" : "└─ ";
      return `${indent}${prefix}${renderStateSummary(state)}`;
    })
    .join("\n");
}

function renderStateList(states: StateRecord[]): string {
  if (states.length === 0) {
    return "No states available.";
  }

  return states.map((state) => renderStateSummary(state)).join("\n");
}

function renderBranchHeads(root: string): string {
  const currentBranch = getCurrentBranchName(root);
  const lines = listLanes(root).map((lane) => {
    let head: StateRecord | null = null;
    try {
      head = resolveLatestStateForBranch(root, lane.branch);
    } catch {
      head = null;
    }
    const marker = lane.branch === currentBranch ? "*" : " ";
    return `${marker} ${lane.branch}: ${head ? renderStateSummary(head) : "none"}`;
  });

  if (lines.length === 0) {
    return "No heads available.";
  }

  return lines.join("\n");
}

function getAtomicBaseCommit(state: StateRecord): string {
  return state.parentCommit ?? EMPTY_TREE;
}

function renderAtomicStateDiff(root: string, state: StateRecord): string {
  return runGitTextCommand(
    root,
    ["diff", getAtomicBaseCommit(state), state.commit],
    "No changes captured in the selected state.",
  );
}

function compareAtomicStates(root: string, stateA: StateRecord, stateB: StateRecord): string {
  const patchA = runGitTextCommand(root, ["diff", getAtomicBaseCommit(stateA), stateA.commit], "");
  const patchB = runGitTextCommand(root, ["diff", getAtomicBaseCommit(stateB), stateB.commit], "");

  if (patchA.trim() === patchB.trim()) {
    return "No diff between selected atomic state changes.";
  }

  const tempRoot = mkdtempSync(join(tmpdir(), "jjk-atomic-diff-"));
  const fileA = join(tempRoot, `${branchSegment(stateA.label || stateA.id)}.patch`);
  const fileB = join(tempRoot, `${branchSegment(stateB.label || stateB.id)}.patch`);
  writeFileSync(fileA, `${patchA}\n`);
  writeFileSync(fileB, `${patchB}\n`);

  try {
    return runGitTextCommand(
      root,
      ["diff", "--no-index", "--", fileA, fileB],
      "No diff between selected atomic state changes.",
    );
  } finally {
    rmSync(tempRoot, { recursive: true, force: true });
  }
}

function renderAtomicChain(root: string, state: StateRecord): string {
  const repo = loadRepo(root);
  const chain: StateRecord[] = [];
  let current: StateRecord | null = state;

  while (current) {
    chain.push(current);
    current = current.parentStateId
      ? repo.states.find((candidate) => candidate.id === current?.parentStateId) ?? null
      : null;
  }

  chain.reverse();
  return chain
    .map((entry, index) =>
      [
        `${index + 1}/${chain.length} ${shortStateId(entry.id)} [${entry.kind}] ${entry.label} (${stateDisplayBranch(entry)})`,
        renderAtomicStateDiff(root, entry),
      ].join("\n"),
    )
    .join("\n\n");
}

function renderStateFiles(root: string, state: StateRecord): string {
  const files = getStateChangedFiles(root, state.parentCommit ?? EMPTY_TREE, state.commit);
  if (files.length === 0) {
    return "No files changed in the selected state.";
  }
  return files.join("\n");
}

function renderTouchedFiles(root: string, branchQuery: string): string {
  const branchState = resolveLatestStateForBranch(root, branchQuery);
  const branch = stateDisplayBranch(branchState);
  const touched = new Set<string>();
  for (const state of listStates(root)) {
    if (stateDisplayBranch(state) !== branch) {
      continue;
    }
    for (const file of getStateChangedFiles(root, state.parentCommit ?? EMPTY_TREE, state.commit)) {
      touched.add(file);
    }
  }

  if (touched.size === 0) {
    return `No files touched on ${branch}.`;
  }

  return Array.from(touched).sort((left, right) => left.localeCompare(right)).join("\n");
}

function renderBackupsList(root: string): string {
  const backupsRoot = join(root, ".jjk", "backups");
  if (!existsSync(backupsRoot)) {
    return "No backups saved yet.";
  }

  const files = readdirSync(backupsRoot)
    .map((name) => {
      const path = join(backupsRoot, name);
      const stats = statSync(path);
      return {
        name,
        path,
        size: stats.size,
        createdAt: stats.mtime.toISOString(),
      };
    })
    .sort((left, right) => right.createdAt.localeCompare(left.createdAt));

  if (files.length === 0) {
    return "No backups saved yet.";
  }

  const separator = "  ";
  const nameWidth = Math.max(12, ...files.map((entry) => entry.name.length));
  const sizeWidth = Math.max(8, ...files.map((entry) => formatFileSize(entry.size).length));
  const lines = [
    `${"#".padEnd(2)}${separator}${"backup".padEnd(nameWidth)}${separator}${"size".padEnd(sizeWidth)}${separator}modified`,
  ];

  files.forEach((entry, index) => {
    lines.push(
      `${String(index + 1).padEnd(2)}${separator}${entry.name.padEnd(nameWidth)}${separator}${formatFileSize(entry.size).padEnd(sizeWidth)}${separator}${formatDate(entry.createdAt)}`,
    );
  });

  return lines.join("\n");
}

function renderSnapshotLog(root: string): string {
  const history = getWorkspaceSnapshotHistory(root);
  if (history.entries.length === 0) {
    return "No workspace snapshots recorded yet.";
  }

  const lines = [
    `${"#".padEnd(2)}  ${"id".padEnd(8)}  ${"reason".padEnd(28)}  ${"branch".padEnd(18)}  ${"head".padEnd(12)}  created`,
  ];

  history.entries.forEach((snapshot, index) => {
    const current = index === history.index ? "*" : " ";
    const branch = snapshot.git.currentBranch ?? "(detached)";
    const head = snapshot.git.headCommit ? shortCommit(snapshot.git.headCommit, 12) : "-";
    lines.push(
      `${current}${String(index + 1).padEnd(2)}  ${shortCommit(snapshot.id, 8).padEnd(8)}  ${snapshot.reason.slice(0, 28).padEnd(28)}  ${branch.slice(0, 18).padEnd(18)}  ${head.padEnd(12)}  ${formatDate(snapshot.createdAt)}`,
    );
  });

  return lines.join("\n");
}

function renderBackupPreview(snapshot: { createdAt: string; reason: string; repo: RepoData; git: { currentBranch: string | null; headCommit: string | null; branches: Record<string, string>; }; }): string {
  const history = snapshot.repo.currentStateHistory ?? null;
  const currentState = history ? history.entries[history.index] ?? null : null;
  const laneCount = Object.keys(snapshot.repo.lanes).length;
  const branchCount = Object.keys(snapshot.git.branches).length;
  return [
    `backup preview: ${snapshot.reason}`,
    `created: ${formatDate(snapshot.createdAt)}`,
    `current branch: ${snapshot.git.currentBranch ?? "(detached)"}`,
    `head: ${snapshot.git.headCommit ? shortCommit(snapshot.git.headCommit, 12) : "-"}`,
    `repo states: ${snapshot.repo.states.length}`,
    `lanes: ${laneCount}`,
    `git branches: ${branchCount}`,
    `current state id: ${currentState ?? "-"}`,
  ].join("\n");
}

function applyStateReplay(root: string, sourceState: StateRecord, targetBranchQuery: string, kind: "replay" | "merge-state"): StateRecord {
  if (hasDirtyWorktree(root)) {
    saveState(root, {
      kind: "auto",
      description: `auto pre-${kind} checkpoint before ${sourceState.id}`,
    });
  }

  const targetBranch = normalizeBranchName(targetBranchQuery);
  let targetState: StateRecord | null = null;
  try {
    targetState = resolveLatestStateForBranch(root, targetBranch);
  } catch {
    targetState = null;
  }

  createOrSwitchBranch(root, targetBranch, targetState?.commit ?? getHeadCommit(root) ?? undefined, {
    force: true,
    reset: true,
  });

  const logicalParentCommit = sourceState.parentStateId
    ? resolveState(root, sourceState.parentStateId).commit
    : sourceState.parentCommit;
  const applied = pickStateChanges(root, logicalParentCommit, sourceState.commit);
  if (!applied) {
    throw new Error(`No changes to ${kind} from ${shortStateId(sourceState.id)}.`);
  }

  const result = saveState(root, {
    kind: "cherry",
    description: `${kind} ${sourceState.id} ${sourceState.label}`,
    label: `cherry_${branchSegment(sourceState.label)}`,
    metadata: {
      ...(targetState?.id ? { base: targetState.id } : {}),
      cherry: sourceState.id,
    },
  }, {
    forceCurrentBranch: targetBranch,
    allowMainBranchSave: targetBranch === "main",
    continuationBranch: targetBranch,
    suppressReturnBranchFork: true,
  });

  const activation = activateState(root, result.state, "returned", {
    syncHistoryBeforeNavigate: false,
  });
  console.log(`${kind} ${sourceState.id} onto ${targetBranch}`);
  console.log(renderStateSummary(result.state));
  console.log(activation);
  recordWorkspaceSnapshot(root, `${kind}:${result.state.id}`);
  return result.state;
}

function renderBackupSummary(root: string, path: string, snapshot: unknown): string {
  if (
    typeof snapshot !== "object" ||
    snapshot === null ||
    !("createdAt" in snapshot) ||
    !("repo" in snapshot) ||
    !("git" in snapshot)
  ) {
    return `restore preview: ${formatRelativePath(root, path)}`;
  }

  const typed = snapshot as {
    createdAt: string;
    reason: string;
    repo: RepoData;
    git: {
      currentBranch: string | null;
      headCommit: string | null;
      branches: Record<string, string>;
    };
  };

  return renderBackupPreview(typed);
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
  const visibleStates = states.filter((state) => !isDeletedState(state));
  const branchName = getCurrentBranchName(root);
  const headCommit = getHeadCommit(root);
  const repo = loadRepo(root);
  const historyStateId = getCurrentStateHistoryEntry(repo);

  if (historyStateId) {
    const historyState = visibleStates.find((state) => state.id === historyStateId) ?? null;
    if (historyState && stateMatchesWorkspace(historyState, branchName, headCommit)) {
      return historyState;
    }
  }

  if (headCommit) {
    for (let index = visibleStates.length - 1; index >= 0; index -= 1) {
      const state = visibleStates[index];
      if (state && stateMatchesWorkspace(state, branchName, headCommit)) {
        return state;
      }
    }

    for (let index = visibleStates.length - 1; index >= 0; index -= 1) {
      const state = visibleStates[index];
      if (state && state.commit === headCommit) {
        return state;
      }
    }
  }

  if (historyStateId) {
    return visibleStates.find((state) => state.id === historyStateId) ?? null;
  }

  const branch = branchName ?? getCurrentBranch(root);
  const laneName = repo.branchLaneMap[branch];
  return laneName ? visibleStates.find((state) => state.id === repo.lanes[laneName]?.currentStateId) ?? null : null;
}

function getCurrentStateHistoryEntry(repo: RepoData): string | null {
  const history = repo.currentStateHistory;
  if (!history || history.index < 0 || history.index >= history.entries.length) {
    return null;
  }
  return history.entries[history.index] ?? null;
}

function resolveMarkerTarget(root: string, query: string): StateRecord {
  const target = query.length > 0
    ? resolveState(root, query)
    : resolveCurrentState(root, loadRepo(root).states);
  if (!target) {
    throw new Error("No current state is available.");
  }
  return target;
}

function renderConfigView(root: string): string {
  const repo = loadRepo(root);
  const aliases = Object.entries(listAliases(root));
  const lockedBranches = repo.settings.lockedBranches ?? [];
  return [
    `default branch: ${repo.settings.defaultBranch ?? "unset"}`,
    `snapshot refs in git: ${repo.settings.showWorkspaceSnapshotsInGit ? "on" : "off"}`,
    `aliases: ${aliases.length > 0 ? aliases.map(([name, query]) => `${name}=${query}`).join(", ") : "none"}`,
    `locked branches: ${lockedBranches.length > 0 ? lockedBranches.join(", ") : "none"}`,
  ].join("\n");
}

function renderRecentStates(root: string, limit = 8): string {
  const repo = loadRepo(root);
  const history = repo.currentStateHistory?.entries ?? [];
  if (history.length === 0) {
    return "No visited states yet.";
  }

  const recentIds = history.slice(Math.max(0, history.length - limit));
  return recentIds
    .map((stateId, index) => {
      const state = repo.states.find((candidate) => candidate.id === stateId);
      if (!state) {
        return `${index + 1}. ${stateId}`;
      }
      return `${index + 1}. ${renderStateSummary(state)}`;
    })
    .join("\n");
}

function openStateFiles(root: string, state: StateRecord): string {
  const output = runGitTextCommand(
    root,
    ["show", "--name-only", "--pretty=format:", state.commit],
    "",
  )
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

  if (output.length === 0) {
    return `No files recorded for ${shortStateId(state.id)} ${state.label}.`;
  }

  const editor = process.env.EDITOR?.trim();
  if (editor) {
    Bun.spawnSync([editor, ...output], {
      cwd: root,
      stdout: "inherit",
      stderr: "inherit",
    });
  }

  return `open files for ${shortStateId(state.id)} ${state.label}\n${output.join("\n")}`;
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

  const children = repo.states.filter((state) => state.parentStateId === currentState.id && !isDeletedState(state));
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
      const relativeNext = formatRelativePath(start, next);
      if (isGitIgnored(start, relativeNext)) {
        continue;
      }
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
      if (isDeletedState(candidate)) {
        return false;
      }
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

function resolveUndoFallbackState(root: string, stateId: string): StateRecord | null {
  const repo = loadRepo(root);
  const history = repo.currentStateHistory ?? { entries: [], index: -1 };
  for (let index = history.index - 1; index >= 0; index -= 1) {
    const candidate = repo.states.find((state) => state.id === history.entries[index] && !isDeletedState(state));
    if (candidate && candidate.id !== stateId) {
      return candidate;
    }
  }

  const source = repo.states.find((state) => state.id === stateId) ?? null;
  if (source?.parentStateId) {
    const parent = repo.states.find((state) => state.id === source.parentStateId && !isDeletedState(state));
    if (parent) {
      return parent;
    }
  }

  return repo.states
    .slice()
    .reverse()
    .find((state) => state.id !== stateId && !isDeletedState(state)) ?? null;
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

  if (command === "help" || command === "/help" || command === "--help" || command === "-h" || command === "-help") {
    printHelp();
    return;
  }

  if (command === "shell-init") {
    console.log(renderShellInit(args[1] ?? process.env.SHELL?.split("/").pop() ?? "zsh"));
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
  ensureWorkspaceSnapshot(root, "current");

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
      await handleSave(root, buildSaveRequest(command, args.slice(1).join(" ")));
      return;
    case "where": {
      const currentState = resolveCurrentState(root, loadRepo(root).states);
      if (!currentState) {
        throw new Error("No current state is available.");
      }
      console.log(
        `${shortStateId(currentState.id)} [${currentState.kind}] ${currentState.label} on ${stateDisplayBranch(currentState)} (workspace ${getCurrentBranchName(root) ?? "detached"})`,
      );
      return;
    }
    case "star": {
      const target = resolveMarkerTarget(root, args.slice(1).join(" ").trim());
      const starred = starState(root, target.id);
      recordWorkspaceSnapshot(root, `star:${starred.id}`);
      console.log(`starred ${shortStateId(starred.id)} ${starred.label}`);
      console.log(renderStateSummary(starred));
      return;
    }
    case "unstar": {
      const target = resolveMarkerTarget(root, args.slice(1).join(" ").trim());
      const unstarred = unstarState(root, target.id);
      recordWorkspaceSnapshot(root, `unstar:${unstarred.id}`);
      console.log(`unstarred ${shortStateId(unstarred.id)} ${unstarred.label}`);
      console.log(renderStateSummary(unstarred));
      return;
    }
    case "pin":
    case "unpin": {
      const target = resolveMarkerTarget(root, args.slice(1).join(" ").trim());
      const updated = command === "pin" ? pinState(root, target.id) : unpinState(root, target.id);
      recordWorkspaceSnapshot(root, `${command}:${updated.id}`);
      console.log(`${command === "pin" ? "pinned" : "unpinned"} ${shortStateId(updated.id)} ${updated.label}`);
      console.log(renderStateSummary(updated));
      return;
    }
    case "thumbsup":
    case "thumbsdown": {
      const target = resolveMarkerTarget(root, args.slice(1).join(" ").trim());
      const tag = command;
      const updated = toggleStateTag(root, target.id, tag);
      recordWorkspaceSnapshot(root, `${tag}:${updated.id}`);
      const enabled = updated.tags.includes(tag);
      console.log(`${enabled ? "enabled" : "disabled"} ${tag} on ${shortStateId(updated.id)} ${updated.label}`);
      console.log(renderStateSummary(updated));
      return;
    }
    case "archive": {
      const target = resolveMarkerTarget(root, args.slice(1).join(" ").trim());
      const archived = deleteState(root, target.id);
      recordWorkspaceSnapshot(root, `archive:${archived.id}`);
      console.log(`archived ${shortStateId(archived.id)} to ${archived.metadata?.deletedBranch}`);
      return;
    }
    case "quarantine": {
      const target = resolveMarkerTarget(root, args.slice(1).join(" ").trim());
      const quarantined = annotateState(root, target.id, (metadata) => ({
        ...(metadata ?? {}),
        quarantinedAt: nowIso(),
        status: "quarantined",
      }));
      recordWorkspaceSnapshot(root, `quarantine:${quarantined.id}`);
      console.log(`quarantined ${shortStateId(quarantined.id)} ${quarantined.label}`);
      console.log(renderStateSummary(quarantined));
      return;
    }
    case "mark": {
      const stateQuery = args[1] ?? "";
      const status = args[2] ?? "";
      if (stateQuery.length === 0 || status.length === 0) {
        throw new Error("Usage: jjk mark <state> <status>");
      }
      const marked = annotateState(root, resolveState(root, stateQuery).id, (metadata) => ({
        ...(metadata ?? {}),
        status,
      }));
      recordWorkspaceSnapshot(root, `mark:${marked.id}`);
      console.log(`marked ${shortStateId(marked.id)} as ${status}`);
      console.log(renderStateSummary(marked));
      return;
    }
    case "assign-note": {
      const parsed = parseStateLabelAndMessage(args.slice(1).join(" "));
      if (parsed.description.length === 0) {
        throw new Error("Usage: jjk assign-note <state>, <person/note>");
      }
      const target = resolveState(root, parsed.description);
      const [assignee, ...noteParts] = (parsed.message ?? "").split("/").map((part) => part.trim());
      const note = noteParts.join("/").trim();
      const assigned = annotateState(root, target.id, (metadata) => ({
        ...(metadata ?? {}),
        assignee: assignee && note.length > 0 ? assignee : metadata?.assignee,
        note: note.length > 0 ? note : metadata?.note,
      }));
      recordWorkspaceSnapshot(root, `assign-note:${assigned.id}`);
      console.log(`assigned note on ${shortStateId(assigned.id)} ${assigned.label}`);
      console.log(renderStateSummary(assigned));
      return;
    }
    case "ready": {
      const target = resolveMarkerTarget(root, args.slice(1).join(" ").trim());
      const readyState = annotateState(root, target.id, (metadata) => ({
        ...(metadata ?? {}),
        status: "ready",
      }));
      recordWorkspaceSnapshot(root, `ready:${readyState.id}`);
      console.log(`ready ${shortStateId(readyState.id)} ${readyState.label}`);
      console.log(renderStateSummary(readyState));
      return;
    }
    case "publish": {
      const target = resolveMarkerTarget(root, args.slice(1).join(" ").trim());
      const published = annotateState(root, target.id, (metadata) => ({
        ...(metadata ?? {}),
        status: "published",
        publishedAt: nowIso(),
      }));
      recordWorkspaceSnapshot(root, `publish:${published.id}`);
      console.log(`published ${shortStateId(published.id)} ${published.label}`);
      console.log(renderStateSummary(published));
      return;
    }
    case "handoff": {
      const parsed = parseStateLabelAndMessage(args.slice(1).join(" "));
      const target = resolveState(root, parsed.description);
      const handoff = annotateState(root, target.id, (metadata) => ({
        ...(metadata ?? {}),
        handoff: (parsed.message ?? target.description).trim(),
      }));
      recordWorkspaceSnapshot(root, `handoff:${handoff.id}`);
      console.log(`handoff recorded for ${shortStateId(handoff.id)} ${handoff.label}`);
      console.log(renderStateSummary(handoff));
      return;
    }
    case "copy-id": {
      const target = resolveMarkerTarget(root, args.slice(1).join(" ").trim());
      console.log(target.id);
      return;
    }
    case "recent": {
      const limit = Number.parseInt(args[1] ?? "", 10);
      console.log(renderRecentStates(root, Number.isFinite(limit) && limit > 0 ? limit : 8));
      return;
    }
    case "aliases": {
      const subcommand = (args[1] ?? "").trim().toLowerCase();
      if (subcommand === "add") {
        const name = args[2] ?? "";
        const query = args.slice(3).join(" ").trim();
        if (name.trim().length === 0 || query.length === 0) {
          throw new Error("Usage: jjk aliases add <name> <query>");
        }
        setAlias(root, name, query);
        recordWorkspaceSnapshot(root, `aliases:add:${name}`);
        console.log(`alias added: ${name} -> ${query}`);
        return;
      }
      if (subcommand === "remove" || subcommand === "rm" || subcommand === "delete") {
        const name = args[2] ?? "";
        if (name.trim().length === 0) {
          throw new Error("Usage: jjk aliases remove <name>");
        }
        removeAlias(root, name);
        recordWorkspaceSnapshot(root, `aliases:remove:${name}`);
        console.log(`alias removed: ${name}`);
        return;
      }
      const aliases = Object.entries(listAliases(root));
      if (aliases.length === 0) {
        console.log("No aliases recorded yet.");
        return;
      }
      console.log(aliases.map(([name, query]) => `${name} -> ${query}`).join("\n"));
      return;
    }
    case "default-branch": {
      const input = args.slice(1).join(" ").trim();
      if (input.length === 0) {
        console.log(renderConfigView(root));
        return;
      }
      const normalized = normalizeBranchName(input);
      setDefaultBranch(root, normalized);
      recordWorkspaceSnapshot(root, `default-branch:${normalized}`);
      console.log(`default branch set to ${normalized}`);
      return;
    }
    case "config": {
      console.log(renderConfigView(root));
      return;
    }
    case "open": {
      const target = resolveMarkerTarget(root, args.slice(1).join(" ").trim());
      console.log(openStateFiles(root, target));
      return;
    }
    case "checkpoint": {
      await handleSave(
        root,
        buildSaveRequest("save", args.slice(1).join(" ").trim() || "checkpoint"),
        {
          allowMainBranchSave: true,
          continuationBranch: null,
          suppressReturnBranchFork: true,
        },
      );
      return;
    }
    case "autosave": {
      const subcommand = (args[1] ?? "").trim().toLowerCase();
      if (subcommand !== "now") {
        throw new Error("Usage: jjk autosave now");
      }
      if (!hasDirtyWorktree(root)) {
        console.log("worktree clean; nothing to autosave.");
        return;
      }
      await handleSave(
        root,
        buildSaveRequest("auto", args.slice(2).join(" ").trim() || "autosave now"),
        {
          allowMainBranchSave: true,
          continuationBranch: null,
          suppressReturnBranchFork: true,
        },
      );
      return;
    }
    case "lock": {
      const input = args.slice(1).join(" ").trim();
      if (input.length === 0) {
        throw new Error("Usage: jjk lock <branch>");
      }
      const branch = normalizeBranchName(input);
      setBranchLock(root, branch, true);
      recordWorkspaceSnapshot(root, `lock:${branch}`);
      console.log(`locked ${branch}`);
      return;
    }
    case "unlock": {
      const input = args.slice(1).join(" ").trim();
      if (input.length === 0) {
        throw new Error("Usage: jjk unlock <branch>");
      }
      const branch = normalizeBranchName(input);
      setBranchLock(root, branch, false);
      recordWorkspaceSnapshot(root, `unlock:${branch}`);
      console.log(`unlocked ${branch}`);
      return;
    }
    case "clean": {
      const aliases = listAliases(root);
      const repo = loadRepo(root);
      const cleanedAliases = Object.entries(aliases).filter(([, query]) => {
        try {
          resolveState(root, query);
          return true;
        } catch {
          return false;
        }
      });
      repo.settings.aliases = Object.fromEntries(cleanedAliases);
      saveRepo(root, repo);
      recordWorkspaceSnapshot(root, "clean");
      console.log(`cleaned ${Object.keys(aliases).length - cleanedAliases.length} stale aliases`);
      return;
    }
    case "gc": {
      const history = loadRepo(root).currentStateHistory;
      if (!history) {
        console.log("nothing to gc.");
        return;
      }
      recordWorkspaceSnapshot(root, "gc");
      console.log("gc completed");
      return;
    }
    case "branch": {
      const input = args.slice(1).join(" ").trim();
      if (input.length === 0) {
        console.log(renderBranchList(root));
        return;
      }

      const currentState = resolveCurrentState(root, loadRepo(root).states) ?? resolveDefaultState(root);
      const branch = normalizeBranchName(input);
      createBranchAtState(root, branch, currentState.id);
      recordWorkspaceSnapshot(root, `branch:${branch}`);
      console.log(`created branch ${branch} at ${shortStateId(currentState.id)} ${currentState.label}`);
      return;
    }
    case "branch-from":
    case "split": {
      const rawArgs = args.slice(1);
      if (rawArgs.length < 2) {
        throw new Error(`Usage: jjk ${command} <state> <${command === "split" ? "new-branch" : "label"}>`);
      }
      const stateQuery = rawArgs.slice(0, -1).join(" ").trim();
      const branch = normalizeBranchName(rawArgs.at(-1)!);
      const state = resolveState(root, stateQuery);
      const lane = command === "split"
        ? splitState(root, state.id, branch)
        : branchFromState(root, state.id, branch);
      recordWorkspaceSnapshot(root, `${command}:${lane.branch}`);
      console.log(`${command === "split" ? "split" : "branched"} ${shortStateId(state.id)} to ${lane.branch}`);
      return;
    }
    case "checkout": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Usage: jjk checkout <branch>");
      }
      const lane = resolveLane(root, query);
      const branch = lane?.branch ?? query;
      switchBranch(root, branch);
      const currentState = resolveCurrentState(root, loadRepo(root).states);
      syncCurrentStateHistory(root, currentState?.id ?? null);
      recordWorkspaceSnapshot(root, `checkout:${branch}`);
      console.log(`checked out ${branch}`);
      return;
    }
    case "fork": {
      const worktreeRequested = args.includes("--worktree");
      const input = args
        .slice(1)
        .filter((arg) => arg !== "--worktree")
        .join(" ")
        .trim();

      if (!worktreeRequested) {
        if (input.length === 0) {
          throw new Error("Usage: jjk fork <name> [--worktree]");
        }
        await handleSave(root, buildSaveRequest("new", input));
        return;
      }

      if (input.length === 0) {
        const created = createStateWorktree(root, resolveDefaultState(root), "fork");
        recordWorkspaceSnapshot(root, `fork-worktree:${created.branch}`);
        printWorktreeReady(root, created);
        return;
      }

      const sourceState = tryResolveState(root, input);
      if (sourceState) {
        const created = createStateWorktree(root, sourceState, "fork");
        recordWorkspaceSnapshot(root, `fork-worktree:${created.branch}`);
        printWorktreeReady(root, created);
        return;
      }

      const previousBranch = getCurrentBranchName(root);
      const previousCommit = getHeadCommit(root);
      const previousState = resolveCurrentState(root, loadRepo(root).states);
      await handleSave(root, buildSaveRequest("new", input));
      const forkState = resolveCurrentState(root, loadRepo(root).states);
      if (!forkState) {
        throw new Error("Unable to resolve the new fork state.");
      }
      const forkBranch = stateDisplayBranch(forkState);

      if (previousBranch) {
        createOrSwitchBranch(root, previousBranch, previousCommit ?? undefined, {
          force: true,
          reset: true,
        });
      } else if (previousCommit) {
        switchToDetachedCommit(root, previousCommit, {
          discardChanges: true,
        });
      }

      syncCurrentStateHistory(root, previousState?.id ?? null);
      const worktreePath = uniqueWorktreePath(root, forkBranch);
      addWorktree(root, worktreePath, forkBranch);
      ensureWorktreeSharesJjkStore(root, worktreePath);
      recordWorkspaceSnapshot(root, `fork-worktree:${forkState.id}`);
      console.log(`fork ready: ${forkBranch}`);
      console.log(renderStateSummary(forkState));
      printWorktreeReady(root, {
        branch: forkBranch,
        path: worktreePath,
        state: forkState,
      });
      return;
    }
    case "worktree": {
      const query = args.slice(1).join(" ").trim();
      const sourceState = query.length > 0 ? resolveState(root, query) : resolveDefaultState(root);
      const created = createStateWorktree(root, sourceState, "worktree");
      recordWorkspaceSnapshot(root, `worktree:${created.branch}`);
      printWorktreeReady(root, created);
      return;
    }
    case "stash": {
      const parsed = parseStateLabelAndMessage(args.slice(1).join(" "));
      const stashed = stashWorkspace(root, parsed);
      recordWorkspaceSnapshot(root, `stash:${stashed.state.id}`);
      console.log(`stashed changes into ${stateDisplayBranch(stashed.state)}`);
      console.log(renderStateSummary(stashed.state));
      return;
    }
    case "note": {
      const parsed = splitNoteArgs(args.slice(1).join(" "));
      const target = resolveState(root, parsed.state);
      const noted = noteState(root, target.id, parsed.message);
      recordWorkspaceSnapshot(root, `note:${noted.id}`);
      console.log(`noted ${shortStateId(noted.id)} ${noted.label}`);
      console.log(renderStateSummary(noted));
      return;
    }
    case "nice":
      await handleSave(
        root,
        buildSaveRequest("nice", args.slice(1).join(" ")),
        {
          suppressReturnBranchFork: true,
        },
      );
      return;
    case "inspect": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Usage: jjk inspect <state>");
      }

      const repo = loadRepo(root);
      const state = resolveState(root, query, { includeDeleted: true });
      console.log(renderStateInspection(repo, state));
      return;
    }
    case "search": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Usage: jjk search <query>");
      }

      const repo = loadRepo(root);
      const matches = findStateMatches(repo.states, query);
      if (matches.length === 0) {
        console.log(`No states matched \`${query}\`.`);
        return;
      }

      console.log(`Search results for \`${query}\`:`);
      console.log(renderStateChoiceTable(matches.map((match) => match.state), { colorize: shouldColorizeOutput() }));
      return;
    }
    case "see": {
      const filters = parseStateViewFilters(args.slice(1));
      const repo = loadRepo(root);
      const colorize = shouldColorizeOutput();
      const visibleStates = filterStatesForView(
        listStates(root, { includeDeleted: filters.includeDeleted }),
        filters,
      );
      if (visibleStates.length === 0) {
        console.log("No states matched the selected filters.");
        return;
      }

      const filteredRepo = { ...repo, states: visibleStates };
      const currentState = resolveCurrentState(root, repo.states);
      const currentStateId = currentState && visibleStates.some((state) => state.id === currentState.id)
        ? currentState.id
        : null;
      console.log(renderGraph(filteredRepo, {
        currentStateId,
        colorize,
        includeDeleted: filters.includeDeleted,
      }));
      console.log("");
      console.log(
        renderStateTable(visibleStates, {
          colorize,
          currentStateId,
          repo: filteredRepo,
          includeDeleted: filters.includeDeleted,
        }),
      );
      return;
    }
    case "log": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Usage: jjk log <branch>");
      }
      const repo = loadRepo(root);
      const branch = resolveBranchName(root, query);
      const branchStates = repo.states.filter(
        (state) => !isDeletedState(state) && stateDisplayBranch(state) === branch,
      );
      const branchRepo: RepoData = {
        ...repo,
        states: branchStates,
      };
      const currentState = branchStates[branchStates.length - 1] ?? null;
      console.log(renderLogGraph(branchRepo, {
        currentStateId: currentState?.id ?? null,
        colorize: shouldColorizeOutput(),
        includeDeleted: false,
      }));
      return;
    }
    case "graph": {
      const filters = parseStateViewFilters(args.slice(1), { allowBranch: true });
      const repo = loadRepo(root);
      const colorize = shouldColorizeOutput();
      if (filters.branch) {
        filters.branch = resolveBranchQuery(root, filters.branch);
      }
      const visibleStates = filterStatesForView(
        listStates(root, { includeDeleted: filters.includeDeleted }),
        filters,
      );
      if (visibleStates.length === 0) {
        console.log("No states matched the selected filters.");
        return;
      }

      const filteredRepo = { ...repo, states: visibleStates };
      const currentState = resolveCurrentState(root, repo.states);
      const currentStateId = currentState && visibleStates.some((state) => state.id === currentState.id)
        ? currentState.id
        : null;
      console.log(renderLogGraph(filteredRepo, {
        currentStateId,
        colorize,
        includeDeleted: filters.includeDeleted,
      }));
      return;
    }
    case "timeline": {
      const repo = loadRepo(root);
      const colorize = shouldColorizeOutput();
      const currentState = resolveCurrentState(root, repo.states);
      const states = listStates(root);
      console.log(
        renderStateTable(states, {
          colorize,
          currentStateId: currentState?.id ?? null,
          repo,
        }),
      );
      return;
    }
    case "favorites": {
      const repo = loadRepo(root);
      const colorize = shouldColorizeOutput();
      const states = listStates(root).filter((state) => stateHasStar(state));
      if (states.length === 0) {
        console.log("No starred states yet.");
        return;
      }
      const currentState = resolveCurrentState(root, repo.states);
      const currentStateId = currentState && states.some((state) => state.id === currentState.id)
        ? currentState.id
        : null;
      console.log(
        renderStateTable(states, {
          colorize,
          currentStateId,
          repo,
        }),
      );
      return;
    }
    case "amend": {
      const currentState = resolveCurrentState(root, loadRepo(root).states);
      if (!currentState) {
        throw new Error("No current state is available.");
      }

      const amendment = parseStateLabelAndMessage(args.slice(1).join(" "));
      const amended = amendState(root, currentState.id, {
        description: amendment.description || currentState.description,
        label: amendment.label,
        message: amendment.message,
      });
      syncCurrentStateHistory(root, amended.id);
      recordWorkspaceSnapshot(root, `amend:${amended.id}`);
      console.log(`amended ${shortStateId(amended.id)} ${amended.label}`);
      console.log(renderStateSummary(amended));
      return;
    }
    case "show":
    case "patch": {
      const atomicChain = args.includes("--atomic-chain");
      const query = args.slice(1).filter((arg) => arg !== "--atomic-chain").join(" ").trim();
      const state = query.length > 0 ? resolveState(root, query) : resolveDefaultState(root);
      console.log(atomicChain ? renderAtomicChain(root, state) : renderAtomicStateDiff(root, state));
      return;
    }
    case "files": {
      const query = args.slice(1).join(" ").trim();
      const state = query.length > 0 ? resolveState(root, query) : resolveDefaultState(root);
      console.log(renderStateFiles(root, state));
      return;
    }
    case "touched": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Usage: jjk touched <branch>");
      }
      console.log(renderTouchedFiles(root, query));
      return;
    }
    case "move": {
      const rawArgs = args.slice(1);
      if (rawArgs.length < 2) {
        throw new Error("Usage: jjk move <state> <branch>");
      }
      const stateQuery = rawArgs.slice(0, -1).join(" ").trim();
      const branch = normalizeBranchName(rawArgs.at(-1)!);
      const state = resolveState(root, stateQuery);
      const moved = moveState(root, state.id, branch);
      recordWorkspaceSnapshot(root, `move:${moved.id}`);
      console.log(`moved ${shortStateId(moved.id)} to ${branch}`);
      console.log(renderStateSummary(moved));
      return;
    }
    case "git": {
      const subcommand = (args[1] ?? "").trim().toLowerCase();
      if (subcommand === "log") {
        const colorize = shouldColorizeOutput();
        console.log(
          runGitTextCommand(
            root,
            ["log", "--all", "--oneline", "--graph", "--decorate"],
            "No git commits yet.",
            { colorize },
          ),
        );
        return;
      }
      throw new Error("Usage: jjk git log");
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
    case "heads": {
      console.log(renderBranchHeads(root));
      return;
    }
    case "root": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Usage: jjk root <state>");
      }
      const state = resolveState(root, query);
      const trail = collectStateTrail(root, state);
      console.log(renderStateSummary(trail[0] ?? state));
      return;
    }
    case "trail": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Usage: jjk trail <state>");
      }
      const state = resolveState(root, query);
      console.log(renderStateTrail(collectStateTrail(root, state)));
      return;
    }
    case "children": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Usage: jjk children <state>");
      }
      const state = resolveState(root, query);
      const children = listStates(root).filter(
        (candidate) => candidate.parentStateId === state.id,
      );
      console.log(renderStateList(children));
      return;
    }
    case "parents": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Usage: jjk parents <state>");
      }
      const state = resolveState(root, query);
      const repo = loadRepo(root);
      const parent = state.parentStateId
        ? repo.states.find((candidate) => candidate.id === state.parentStateId && !isDeletedState(candidate))
        : null;
      console.log(parent ? renderStateSummary(parent) : "No parent state is available.");
      return;
    }
    case "status": {
      const repo = loadRepo(root);
      const branch = getCurrentBranch(root);
      const laneName = repo.branchLaneMap[branch];
      const visibleStates = listStates(root);
      const latestState = visibleStates.length > 0 ? visibleStates[visibleStates.length - 1] : null;
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
    case "delete": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Provide a state to delete.");
      }
      const currentState = resolveCurrentState(root, loadRepo(root).states);
      const state = resolveState(root, query);
      const fallback = currentState?.id === state.id ? resolveUndoFallbackState(root, state.id) : null;
      const deleted = deleteState(root, state.id);
      recordWorkspaceSnapshot(root, `delete:${deleted.id}`);
      console.log(`deleted ${shortStateId(deleted.id)} to ${deleted.metadata?.deletedBranch}`);
      if (fallback) {
        console.log(activateState(root, fallback, "returned"));
        recordWorkspaceSnapshot(root, `return:${fallback.id}`);
      }
      return;
    }
    case "recover": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Provide a deleted state to recover.");
      }
      const state = resolveState(root, query, { includeDeleted: true });
      const recovered = recoverState(root, state.id);
      recordWorkspaceSnapshot(root, `recover:${recovered.id}`);
      console.log(`recovered ${shortStateId(recovered.id)} to ${stateDisplayBranch(recovered)}`);
      console.log(renderStateSummary(recovered));
      return;
    }
    case "undo": {
      const snapshot = undoWorkspaceSnapshot(root);
      const currentState = resolveCurrentState(root, loadRepo(root).states);
      if (currentState) {
        console.log(`undid to ${shortStateId(currentState.id)} ${currentState.label}`);
      } else {
        console.log(`undid to snapshot ${snapshot.id}`);
      }
      return;
    }
    case "redo": {
      const snapshot = redoWorkspaceSnapshot(root);
      const currentState = resolveCurrentState(root, loadRepo(root).states);
      if (currentState) {
        console.log(`redid to ${shortStateId(currentState.id)} ${currentState.label}`);
      } else {
        console.log(`redid to snapshot ${snapshot.id}`);
      }
      return;
    }
    case "diff": {
      const atomic = args.includes("--atomic");
      const diffArgs = args.slice(1).filter((arg) => arg !== "--atomic");
      const queryA = diffArgs[0] ?? "";
      const queryB = diffArgs[1] ?? "";

      if (queryA.length === 0) {
        if (atomic) {
          console.log(renderAtomicStateDiff(root, resolveDefaultState(root)));
          return;
        }
        const currentState = resolveDefaultState(root);
        console.log(
          runGitTextCommand(
            root,
            ["diff", currentState.commit],
            "No diff against the latest saved state.",
          ),
        );
        return;
      }

      const stateA = resolveState(root, queryA);
      if (queryB.length === 0) {
        const currentState = resolveDefaultState(root);
        if (atomic) {
          console.log(compareAtomicStates(root, currentState, stateA));
          return;
        }
        console.log(
          runGitTextCommand(
            root,
            ["diff", currentState.commit, stateA.commit],
            "No diff against the selected state.",
          ),
        );
        return;
      }

      const stateB = resolveState(root, queryB);
      if (atomic) {
        console.log(compareAtomicStates(root, stateA, stateB));
        return;
      }
      console.log(
        runGitTextCommand(
          root,
          ["diff", stateA.commit, stateB.commit],
          "No diff between the selected states.",
        ),
      );
      return;
    }
    case "compare-branch": {
      const branchA = args[1] ?? "";
      const branchB = args[2] ?? "";
      if (branchA.trim().length === 0 || branchB.trim().length === 0) {
        throw new Error("Usage: jjk compare-branch <a> <b>");
      }

      const stateA = resolveLatestStateForBranch(root, branchA);
      const stateB = resolveLatestStateForBranch(root, branchB);
      console.log(`branch a: ${renderStateSummary(stateA)}`);
      console.log(`branch b: ${renderStateSummary(stateB)}`);
      console.log("");
      console.log(
        runGitTextCommand(
          root,
          ["diff", stateA.commit, stateB.commit],
          "No diff between the selected branch tips.",
        ),
      );
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
      recordWorkspaceSnapshot(root, `pick:${picked.state.id}`);
      return;
    }
    case "replay": {
      const sourceQuery = args[1] ?? "";
      const ontoIndex = args.findIndex((arg) => arg === "onto");
      const branchQuery = ontoIndex >= 0 ? args.slice(ontoIndex + 1).join(" ").trim() : "";
      if (sourceQuery.length === 0 || branchQuery.length === 0) {
        throw new Error("Usage: jjk replay <state> onto <branch>");
      }

      const sourceState = resolveState(root, sourceQuery);
      applyStateReplay(root, sourceState, branchQuery, "replay");
      return;
    }
    case "merge-state": {
      const sourceQuery = args[1] ?? "";
      const intoIndex = args.findIndex((arg) => arg === "into");
      const branchQuery = intoIndex >= 0 ? args.slice(intoIndex + 1).join(" ").trim() : "";
      if (sourceQuery.length === 0 || branchQuery.length === 0) {
        throw new Error("Usage: jjk merge-state <state> into <branch>");
      }

      const sourceState = resolveState(root, sourceQuery);
      applyStateReplay(root, sourceState, branchQuery, "merge-state");
      return;
    }
    case "revert-state": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Usage: jjk revert-state <state>");
      }

      const sourceState = resolveState(root, query);
      if (hasDirtyWorktree(root)) {
        saveState(root, {
          kind: "auto",
          description: `auto pre-revert checkpoint before ${sourceState.id}`,
        });
      }

      const reverted = revertStateChanges(root, sourceState.commit, sourceState.parentCommit);
      if (!reverted) {
        console.log(`revert produced no changes for ${sourceState.id}`);
        return;
      }

      const currentState = resolveCurrentState(root, loadRepo(root).states);
      const result = saveState(root, {
        kind: "save",
        description: `revert ${sourceState.id} ${sourceState.label}`,
        label: `revert_${branchSegment(sourceState.label)}`,
        metadata: {
          ...(currentState?.id ? { base: currentState.id } : {}),
          cherry: sourceState.id,
        },
      }, {
        suppressReturnBranchFork: true,
      });
      syncCurrentStateHistory(root, result.state.id);
      recordWorkspaceSnapshot(root, `revert-state:${result.state.id}`);
      console.log(`reverted ${shortStateId(sourceState.id)} on ${getCurrentBranch(root)}`);
      console.log(renderStateSummary(result.state));
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
      recordWorkspaceSnapshot(root, `promote:${promoted.id}`);
      console.log(`promoted ${shortStateId(state.id)} to ${targetKind}`);
      console.log(renderStateSummary(promoted));
      return;
    }
    case "return":
      await handleReturn(root, args.slice(1).join(" "));
      recordWorkspaceSnapshot(root, `return:${resolveCurrentState(root, loadRepo(root).states)?.id ?? "unknown"}`);
      return;
    case "continue": {
      const currentState = resolveCurrentState(root, loadRepo(root).states);
      if (!currentState) {
        throw new Error("No current state is available.");
      }
      const branch = stateDisplayBranch(currentState);
      const latest = resolveLatestStateForBranch(root, branch);
      if (stateDisplayBranch(latest) === "main") {
        updateBranchTarget(root, "main", latest.id);
        syncCurrentStateHistory(root, latest.id);
        console.log(`continued to ${shortStateId(latest.id)} on main`);
      } else {
        console.log(activateState(root, latest, "continued"));
      }
      recordWorkspaceSnapshot(root, `continue:${latest.id}`);
      return;
    }
    case "lastest":
    case "latest": {
      const query = args.slice(1).join(" ").trim();
      if (query.length === 0) {
        throw new Error("Usage: jjk lastest <branch>");
      }
      console.log(renderStateSummary(resolveLatestStateForBranch(root, query)));
      return;
    }
    case "back":
      {
        const target = resolveHistoryState(root, -1);
        console.log(
          activateState(root, target.state, "back", {
            historyIndex: target.index,
            syncHistoryBeforeNavigate: false,
          }),
        );
        recordWorkspaceSnapshot(root, `back:${target.state.id}`);
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
        recordWorkspaceSnapshot(root, `forward:${target.state.id}`);
      }
      return;
    case "up": {
      const repo = loadRepo(root);
      const currentState = resolveCurrentState(root, repo.states);
      if (!currentState?.parentStateId) {
        throw new Error("No parent state is available.");
      }
      const target = resolveState(root, currentState.parentStateId);
      console.log(activateState(root, target, "up"));
      recordWorkspaceSnapshot(root, `up:${target.id}`);
      return;
    }
    case "down": {
      const target = await resolveChildState(root);
      console.log(activateState(root, target, "down"));
      recordWorkspaceSnapshot(root, `down:${target.id}`);
      return;
    }
    case "prev": {
      const repo = loadRepo(root);
      const currentState = resolveCurrentState(root, repo.states);
      if (!currentState?.parentStateId) {
        throw new Error("No parent state is available.");
      }
      const target = resolveState(root, currentState.parentStateId);
      console.log(activateState(root, target, "prev"));
      recordWorkspaceSnapshot(root, `prev:${target.id}`);
      return;
    }
    case "next": {
      const target = await resolveChildState(root);
      console.log(activateState(root, target, "next"));
      recordWorkspaceSnapshot(root, `next:${target.id}`);
      return;
    }
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
      recordWorkspaceSnapshot(root, `update:${updated.branch}`);
      console.log(`updated ${updated.branch} to ${targetLabel}`);
      return;
    }
    case "rename-branch": {
      const rawArgs = args.slice(1);
      if (rawArgs.length < 2) {
        throw new Error("Usage: jjk rename-branch <old> <new>");
      }
      const oldBranchQuery = rawArgs.slice(0, -1).join(" ").trim();
      const newBranch = normalizeBranchName(rawArgs.at(-1)!);
      const renamed = renameBranch(root, oldBranchQuery, newBranch);
      recordWorkspaceSnapshot(root, `rename-branch:${renamed.branch}`);
      console.log(`renamed branch to ${renamed.branch}`);
      return;
    }
    case "rename-state": {
      const rawArgs = args.slice(1);
      if (rawArgs.length < 2) {
        throw new Error("Usage: jjk rename-state <state> <new-label>");
      }
      const stateQuery = rawArgs.slice(0, -1).join(" ").trim();
      const nextLabel = rawArgs.at(-1)!;
      const state = resolveState(root, stateQuery);
      const renamed = renameState(root, state.id, nextLabel);
      recordWorkspaceSnapshot(root, `rename-state:${renamed.id}`);
      console.log(`renamed ${shortStateId(renamed.id)} to ${renamed.label}`);
      console.log(renderStateSummary(renamed));
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
        recordWorkspaceSnapshot(root, `timeshift-save:${record.id}`);
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
          recordWorkspaceSnapshot(root, `timeshift-restore:${record.id}`);
          console.log(`timeshift restored to ${record.branch} at ${shortStateId(state.id)}`);
        } else {
          createOrSwitchBranch(root, record.branch, undefined, {
            force: true,
            reset: true,
          });
          syncCurrentStateHistory(root, resolveCurrentState(root, loadRepo(root).states)?.id ?? null);
          recordWorkspaceSnapshot(root, `timeshift-restore:${record.id}`);
          console.log(`timeshift restored to branch ${record.branch}`);
        }
        console.log(`saved cwd: ${record.relativeCwd}`);
        return;
      }

      console.log(renderTimeshifts(listTimeshifts(root)));
      return;
    }
    case "backups": {
      console.log(renderBackupsList(root));
      return;
    }
    case "snapshot-log": {
      console.log(renderSnapshotLog(root));
      return;
    }
    case "restore":
    case "load":
    case "backup": {
      if (command === "backup") {
        const label = args.slice(1).join(" ").trim();
        const path = createBackup(root, label || undefined);
        const size = statSync(path).size;
        console.log(`backup saved: ${formatRelativePath(root, path)} (${formatFileSize(size)})`);
        return;
      }

      const restoreArgs = args.slice(1).filter((arg) => arg !== "--preview");
      const query = restoreArgs.join(" ").trim();
      if (query.length === 0) {
        throw new Error("Usage: jjk load <backupfile>");
      }

      const preview = command === "restore" && args.includes("--preview");
      if (preview) {
        const path = resolveBackupPath(root, query);
        const snapshot = JSON.parse(readFileSync(path, "utf8"));
        console.log(renderBackupSummary(root, path, snapshot));
        return;
      }

      recordWorkspaceSnapshot(root, `before-load:${query}`);
      const loaded = loadBackup(root, query);
      recordWorkspaceSnapshot(root, `load:${loaded.path}`);
      console.log(`loaded backup: ${formatRelativePath(root, loaded.path)}`);
      return;
    }
    case "export": {
      const stateQuery = args[1] ?? "";
      const outputPath = args[2] ?? "";
      if (stateQuery.length === 0 || outputPath.length === 0) {
        throw new Error("Usage: jjk export <state> <file>");
      }

      const state = resolveState(root, stateQuery);
      const path = createBackup(root, outputPath);
      const size = statSync(path).size;
      console.log(`exported ${shortStateId(state.id)} to ${formatRelativePath(root, path)} (${formatFileSize(size)})`);
      return;
    }
    case "import": {
      const path = args.slice(1).join(" ").trim();
      if (path.length === 0) {
        throw new Error("Usage: jjk import <file>");
      }

      recordWorkspaceSnapshot(root, `before-import:${path}`);
      const loaded = loadBackup(root, path);
      recordWorkspaceSnapshot(root, `import:${loaded.path}`);
      console.log(`imported backup: ${formatRelativePath(root, loaded.path)}`);
      return;
    }
    default:
      await handleSave(root, buildSaveRequest("new", args.join(" ")));
  }
}
