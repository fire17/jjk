import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { run } from "./shell";

const EMPTY_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

interface CommitNode {
  hash: string;
  parents: string[];
  children: string[];
  subject: string;
}

export interface WorktreeStatus {
  dirty: boolean;
  changedFiles: number;
  staged: number;
  unstaged: number;
  untracked: number;
}

export interface AheadBehindStatus {
  ahead: number;
  behind: number;
}

export function commandExists(command: string, cwd: string): boolean {
  const shell = process.env.SHELL || "/bin/zsh";
  const result = Bun.spawnSync([shell, "-lc", `command -v ${command}`], {
    cwd,
    stdout: "pipe",
    stderr: "pipe",
  });
  return result.exitCode === 0;
}

export function isGitRepo(cwd: string): boolean {
  return run(["git", "rev-parse", "--is-inside-work-tree"], {
    cwd,
    allowFailure: true,
  }).exitCode === 0;
}

export function initGitRepo(cwd: string): void {
  if (!isGitRepo(cwd)) {
    run(["git", "init", "-b", "main"], { cwd });
  }
}

export function isJjRepo(cwd: string): boolean {
  return run(["jj", "root"], { cwd, allowFailure: true }).exitCode === 0;
}

export function initJjRepo(cwd: string): void {
  if (!commandExists("jj", cwd)) {
    return;
  }

  if (!isJjRepo(cwd)) {
    run(["jj", "git", "init", "--colocate"], { cwd });
  }
}

export function importIntoJj(cwd: string): void {
  if (!isJjRepo(cwd)) {
    return;
  }
  run(["jj", "git", "import"], { cwd, allowFailure: true });
  syncJjKeepRefs(cwd);
}

export function exportFromJj(cwd: string): void {
  if (!isJjRepo(cwd)) {
    return;
  }
  run(["jj", "git", "export"], { cwd, allowFailure: true });
  syncJjKeepRefs(cwd);
}

function syncJjKeepRefs(cwd: string): void {
  if (!shouldShowWorkspaceSnapshotsInGit(cwd)) {
    pruneJjKeepRefs(cwd);
    return;
  }

  normalizeJjKeepCommitMessages(cwd);
}

function shouldShowWorkspaceSnapshotsInGit(cwd: string): boolean {
  const repoFile = join(cwd, ".jjk", "repo.json");
  if (!existsSync(repoFile)) {
    return false;
  }

  try {
    const parsed = JSON.parse(readFileSync(repoFile, "utf8")) as {
      settings?: {
        showWorkspaceSnapshotsInGit?: boolean;
      };
    };
    return parsed.settings?.showWorkspaceSnapshotsInGit === true;
  } catch {
    return false;
  }
}

export function pruneJjKeepRefs(cwd: string): number {
  const keepRefs = run(
    ["git", "for-each-ref", "--format=%(refname)", "refs/jj/keep"],
    { cwd, allowFailure: true },
  ).stdout
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

  for (const ref of keepRefs) {
    run(["git", "update-ref", "-d", ref], { cwd, allowFailure: true });
  }

  return keepRefs.length;
}

function normalizeJjKeepCommitMessages(cwd: string): void {
  const keepRefs = run(
    ["git", "for-each-ref", "--format=%(refname)%09%(subject)", "refs/jj/keep"],
    { cwd, allowFailure: true },
  ).stdout;
  const snapshotRefs = keepRefs
    .split("\n")
    .filter(Boolean)
    .map((line) => line.split("\t"))
    .filter(
      (parts) =>
        parts[0] &&
        (!parts[1] ||
          parts[1].trim().length === 0 ||
          parts[1].trim() === "jjk workspace snapshot"),
    )
    .map((parts) => ({
      ref: parts[0]!,
      commit: parts[0]!.replace("refs/jj/keep/", ""),
    }));

  if (snapshotRefs.length === 0) {
    return;
  }

  const commitGraph = loadCommitGraph(cwd);

  for (const snapshot of snapshotRefs) {
    const message = buildJjSnapshotMessage(snapshot.commit, commitGraph);
    run(
      [
        "jj",
        "describe",
        "--ignore-working-copy",
        "-r",
        snapshot.commit,
        "-m",
        message,
      ],
      { cwd, allowFailure: true },
    );
  }

  run(["jj", "git", "export", "--ignore-working-copy"], { cwd, allowFailure: true });

  for (const snapshot of snapshotRefs) {
    run(["git", "update-ref", "-d", snapshot.ref], { cwd, allowFailure: true });
  }
}

function loadCommitGraph(cwd: string): Map<string, CommitNode> {
  const lines = run(
    ["git", "log", "--all", "--format=%H%x1f%P%x1f%s%x1e", "-n", "400"],
    { cwd, allowFailure: true },
  ).stdout
    .split("\x1e")
    .map((record) => record.trim())
    .filter(Boolean);

  const nodes = new Map<string, CommitNode>();
  for (const line of lines) {
    const [hash = "", parentsRaw = "", subject = ""] = line.split("\x1f");
    if (!hash) {
      continue;
    }
    nodes.set(hash, {
      hash,
      parents: parentsRaw.split(" ").filter(Boolean),
      children: [],
      subject: subject.trim(),
    });
  }

  for (const node of nodes.values()) {
    for (const parent of node.parents) {
      const parentNode = nodes.get(parent);
      if (parentNode) {
        parentNode.children.push(node.hash);
      }
    }
  }

  return nodes;
}

function buildJjSnapshotMessage(commit: string, graph: Map<string, CommitNode>): string {
  const node = graph.get(commit);
  const shortCommit = commit.slice(0, 8);
  const ancestor = node ? findNearestNamedCommit(node.parents, graph, "parents") : null;
  const descendant = node ? findNearestNamedCommit(node.children, graph, "children") : null;
  const subject = buildJjSnapshotSubject(node, shortCommit, ancestor, descendant);
  const body = [
    "Generated automatically by jjk Jujutsu integration.",
    `Snapshot-Commit: ${commit}`,
    `Parent-Count: ${node?.parents.length ?? 0}`,
    `Child-Count: ${node?.children.length ?? 0}`,
    `Nearest-Ancestor-State: ${formatSnapshotContext(ancestor)}`,
    `Nearest-Descendant-State: ${formatSnapshotContext(descendant)}`,
  ].join("\n");
  return `${subject}\n\n${body}`;
}

function buildJjSnapshotSubject(
  node: CommitNode | undefined,
  shortCommit: string,
  ancestor: CommitNode | null,
  descendant: CommitNode | null,
): string {
  const ancestorLabel = shortSnapshotLabel(ancestor);
  const descendantLabel = shortSnapshotLabel(descendant);

  if (ancestorLabel && descendantLabel && ancestor?.hash !== descendant?.hash) {
    return `jjk workspace snapshot between ${ancestorLabel} and ${descendantLabel} (${shortCommit})`;
  }

  if (descendantLabel) {
    return `jjk workspace snapshot before ${descendantLabel} (${shortCommit})`;
  }

  if (ancestorLabel) {
    return `jjk workspace snapshot after ${ancestorLabel} (${shortCommit})`;
  }

  if (!node || node.parents.length === 0) {
    return `jjk workspace root snapshot (${shortCommit})`;
  }

  if (node.children.length === 0) {
    return `jjk workspace leaf snapshot (${shortCommit})`;
  }

  return `jjk workspace orphan snapshot (${shortCommit})`;
}

function shortSnapshotLabel(node: CommitNode | null): string | null {
  if (!node || !node.subject) {
    return null;
  }

  return node.subject.replace(/^jjk\s+/, "");
}

function formatSnapshotContext(node: CommitNode | null): string {
  if (!node) {
    return "none";
  }

  return `${node.hash.slice(0, 8)} ${node.subject}`;
}

function findNearestNamedCommit(
  roots: string[],
  graph: Map<string, CommitNode>,
  direction: "parents" | "children",
): CommitNode | null {
  const queue = [...roots];
  const seen = new Set<string>();

  while (queue.length > 0) {
    const currentHash = queue.shift();
    if (!currentHash || seen.has(currentHash)) {
      continue;
    }
    seen.add(currentHash);

    const current = graph.get(currentHash);
    if (!current) {
      continue;
    }

    if (isMeaningfulSnapshotContext(current.subject)) {
      return current;
    }

    queue.push(...current[direction]);
  }

  return null;
}

function isMeaningfulSnapshotContext(subject: string): boolean {
  const normalized = subject.trim();
  return normalized.length > 0 && !normalized.startsWith("jjk workspace snapshot");
}

export function getCurrentBranch(cwd: string): string {
  return getCurrentBranchName(cwd) ?? "main";
}

export function getCurrentBranchName(cwd: string): string | null {
  const result = run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], {
    cwd,
    allowFailure: true,
  });

  if (result.exitCode === 0 && result.stdout.length > 0) {
    return result.stdout;
  }

  return null;
}

export function hasHead(cwd: string): boolean {
  return run(["git", "rev-parse", "--verify", "HEAD"], {
    cwd,
    allowFailure: true,
  }).exitCode === 0;
}

export function getHeadCommit(cwd: string): string | null {
  const result = run(["git", "rev-parse", "--verify", "HEAD"], {
    cwd,
    allowFailure: true,
  });
  return result.exitCode === 0 && result.stdout.length > 0
    ? result.stdout
    : null;
}

export function ensureLocalExcludes(cwd: string): void {
  const excludePath = join(cwd, ".git", "info", "exclude");
  const existing = existsSync(excludePath)
    ? readFileSync(excludePath, "utf8")
    : "";
  const entries = [".jjk/", ".DS_Store"];
  const additions = entries.filter((entry) => !existing.includes(entry));
  if (additions.length === 0) {
    return;
  }
  const prefix =
    existing.length === 0 || existing.endsWith("\n") ? existing : `${existing}\n`;
  writeFileSync(excludePath, `${prefix}${additions.join("\n")}\n`);
}

export function createSnapshotCommit(
  cwd: string,
  message: string,
  options?: {
    targetBranch?: string;
    parentCommit?: string | null;
  },
): {
  commit: string;
  parentCommit: string | null;
  changedFiles: number;
} {
  exportFromJj(cwd);
  const currentBranch = getCurrentBranchName(cwd);
  const targetBranch = options?.targetBranch;
  if (targetBranch && targetBranch !== currentBranch) {
    return createCommitForRef(cwd, message, targetBranch, options?.parentCommit);
  }
  const parentCommit = options?.parentCommit ?? getHeadCommit(cwd);
  run(["git", "add", "--all", "--", "."], { cwd });
  const changedFiles = countStatusEntries(cwd);
  if (options?.parentCommit && options.parentCommit !== getHeadCommit(cwd)) {
    const tempDir = mkdtempSync(join(tmpdir(), "jjk-index-"));
    const tempIndex = join(tempDir, "index");
    const env = { GIT_INDEX_FILE: tempIndex };

    try {
      run(["git", "read-tree", options.parentCommit], { cwd, env });
      run(["git", "add", "--all", "--", "."], { cwd, env });
      const tree = run(["git", "write-tree"], { cwd, env }).stdout;
      const commit = run(
        ["git", "commit-tree", tree, "-m", message, "-p", options.parentCommit],
        { cwd, env: commitIdentityEnv(env) },
      ).stdout;
      updateRef(cwd, "HEAD", commit);
      return { commit, parentCommit, changedFiles };
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }

  run(["git", "commit", "--allow-empty", "-m", message], {
    cwd,
    env: commitIdentityEnv(),
  });
  const commit = run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout;
  return { commit, parentCommit, changedFiles };
}

function createCommitForRef(
  cwd: string,
  message: string,
  targetBranch: string,
  parentCommitOverride?: string | null,
): {
  commit: string;
  parentCommit: string | null;
  changedFiles: number;
} {
  const tempDir = mkdtempSync(join(tmpdir(), "jjk-index-"));
  const tempIndex = join(tempDir, "index");
  const env = { GIT_INDEX_FILE: tempIndex };

  try {
    const targetHead = run(
      ["git", "rev-parse", "--verify", `refs/heads/${targetBranch}`],
      { cwd, allowFailure: true },
    );
    const parentCommit = targetHead.exitCode === 0
      ? targetHead.stdout
      : parentCommitOverride ?? getHeadCommit(cwd);

    if (parentCommit) {
      run(["git", "read-tree", parentCommit], { cwd, env });
    }

    run(["git", "add", "--all", "--", "."], { cwd, env });
    const changedFiles = countStatusEntries(cwd, env);
    const tree = run(["git", "write-tree"], { cwd, env }).stdout;
    const commitArgs = ["git", "commit-tree", tree, "-m", message];
    if (parentCommit) {
      commitArgs.push("-p", parentCommit);
    }
    const commit = run(commitArgs, { cwd, env: commitIdentityEnv(env) }).stdout;
    updateRef(cwd, `refs/heads/${targetBranch}`, commit);
    return { commit, parentCommit, changedFiles };
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function countStatusEntries(cwd: string, env?: Record<string, string>): number {
  const result = run(["git", "status", "--short", "--untracked-files=all"], {
    cwd,
    env,
  });
  if (result.stdout.length === 0) {
    return 0;
  }
  return result.stdout.split("\n").filter(Boolean).length;
}

export function updateRef(cwd: string, refName: string, commit: string): void {
  run(["git", "update-ref", refName, commit], { cwd });
}

export function restoreHeadWorktree(cwd: string): void {
  run(["git", "restore", "--source=HEAD", "--staged", "--worktree", "--", "."], {
    cwd,
    allowFailure: true,
  });
  run(["git", "clean", "-fd", "--", "."], {
    cwd,
    allowFailure: true,
  });
}

function commitIdentityEnv(env?: Record<string, string>): Record<string, string> {
  return {
    ...(env ?? {}),
    GIT_AUTHOR_NAME: process.env.GIT_AUTHOR_NAME ?? "jjk",
    GIT_AUTHOR_EMAIL: process.env.GIT_AUTHOR_EMAIL ?? "jjk@example.com",
    GIT_COMMITTER_NAME: process.env.GIT_COMMITTER_NAME ?? "jjk",
    GIT_COMMITTER_EMAIL: process.env.GIT_COMMITTER_EMAIL ?? "jjk@example.com",
  };
}

export function createOrSwitchBranch(
  cwd: string,
  branch: string,
  startPoint?: string,
  options?: {
    force?: boolean;
    reset?: boolean;
  },
): void {
  if (options?.reset) {
    const args = ["git", "switch", "-C", branch];
    if (options.force) {
      args.push("--discard-changes");
    }
    if (startPoint) {
      args.push(startPoint);
    }
    run(args, { cwd });
    return;
  }

  const existing = run(
    ["git", "show-ref", "--verify", "--quiet", `refs/heads/${branch}`],
    { cwd, allowFailure: true },
  );

  if (existing.exitCode === 0) {
    run(["git", "switch", branch], { cwd });
    return;
  }

  if (startPoint) {
    run(["git", "switch", "-c", branch, startPoint], { cwd });
    return;
  }

  run(["git", "switch", "-c", branch], { cwd });
}

export function switchToDetachedCommit(
  cwd: string,
  commit: string,
  options?: {
    discardChanges?: boolean;
  },
): void {
  if (options?.discardChanges) {
    run(["git", "checkout", "--detach", "-f", commit], { cwd });
    return;
  }
  run(["git", "switch", "--detach", commit], { cwd });
}

export function worktreeMatchesCommit(cwd: string, commit: string): boolean {
  const tempDir = mkdtempSync(join(tmpdir(), "jjk-match-"));
  const tempIndex = join(tempDir, "index");
  const env = { GIT_INDEX_FILE: tempIndex };

  try {
    run(["git", "add", "--all", "--", "."], { cwd, env });
    const worktreeTree = run(["git", "write-tree"], { cwd, env }).stdout;
    const commitTree = run(["git", "rev-parse", `${commit}^{tree}`], {
      cwd,
      allowFailure: true,
    });
    return commitTree.exitCode === 0 && commitTree.stdout === worktreeTree;
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

export function listRefs(cwd: string, prefix: string): string[] {
  const result = run(["git", "for-each-ref", "--format=%(refname)", prefix], {
    cwd,
    allowFailure: true,
  });
  if (result.exitCode !== 0 || result.stdout.length === 0) {
    return [];
  }
  return result.stdout.split("\n").filter(Boolean);
}

export function getLocalBranchRefs(cwd: string): Record<string, string> {
  const result = run(
    ["git", "for-each-ref", "--format=%(refname:short)%09%(objectname)", "refs/heads"],
    { cwd, allowFailure: true },
  );
  if (result.exitCode !== 0 || result.stdout.length === 0) {
    return {};
  }

  return Object.fromEntries(
    result.stdout
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => line.split("\t"))
      .filter((parts) => parts[0] && parts[1])
      .map((parts) => [parts[0]!, parts[1]!]),
  );
}

export function deleteRef(cwd: string, refName: string): void {
  run(["git", "update-ref", "-d", refName], { cwd, allowFailure: true });
}

export function deleteLocalBranch(cwd: string, branch: string): void {
  run(["git", "branch", "-D", branch], { cwd, allowFailure: true });
}

export function hasRemote(cwd: string, name = "origin"): boolean {
  return run(["git", "remote", "get-url", name], {
    cwd,
    allowFailure: true,
  }).exitCode === 0;
}

export function hasDirtyWorktree(cwd: string): boolean {
  const result = run(["git", "status", "--porcelain", "--untracked-files=all"], {
    cwd,
    allowFailure: true,
  });
  return result.stdout.length > 0;
}

export function getWorktreeStatus(cwd: string): WorktreeStatus {
  const proc = Bun.spawnSync(["git", "status", "--porcelain", "--untracked-files=all"], {
    cwd,
    stdout: "pipe",
    stderr: "pipe",
  });
  const stdout = proc.stdout.toString();

  if (stdout.trim().length === 0) {
    return {
      dirty: false,
      changedFiles: 0,
      staged: 0,
      unstaged: 0,
      untracked: 0,
    };
  }

  let staged = 0;
  let unstaged = 0;
  let untracked = 0;
  const lines = stdout.split("\n").filter(Boolean);

  for (const line of lines) {
    const x = line[0];
    const y = line[1];
    if (x === "?" && y === "?") {
      untracked += 1;
      continue;
    }
    if (x && x !== " ") {
      staged += 1;
    }
    if (y && y !== " ") {
      unstaged += 1;
    }
  }

  return {
    dirty: true,
    changedFiles: lines.length,
    staged,
    unstaged,
    untracked,
  };
}

export function getAheadBehind(cwd: string): AheadBehindStatus | null {
  const branch = getCurrentBranch(cwd);
  const upstream = run(
    ["git", "rev-parse", "--abbrev-ref", `${branch}@{upstream}`],
    { cwd, allowFailure: true },
  );
  if (upstream.exitCode !== 0 || upstream.stdout.length === 0) {
    return null;
  }

  const ahead = run(
    ["git", "rev-list", "--count", `${upstream.stdout}..HEAD`],
    { cwd, allowFailure: true },
  );
  const behind = run(
    ["git", "rev-list", "--count", `HEAD..${upstream.stdout}`],
    { cwd, allowFailure: true },
  );

  return {
    ahead: ahead.exitCode === 0 ? Number.parseInt(ahead.stdout || "0", 10) : 0,
    behind: behind.exitCode === 0 ? Number.parseInt(behind.stdout || "0", 10) : 0,
  };
}

export function pushCurrentBranchAndStateRefs(cwd: string): void {
  exportFromJj(cwd);
  if (!hasRemote(cwd)) {
    throw new Error("No `origin` remote is configured.");
  }

  const branch = getCurrentBranch(cwd);
  run(["git", "push", "origin", branch], { cwd });

  const refs = listRefs(cwd, "refs/jjk/states");
  for (const ref of refs) {
    run(["git", "push", "origin", `${ref}:${ref}`], {
      cwd,
      allowFailure: true,
    });
  }
}

export function fetchStateRefs(cwd: string): void {
  if (!hasRemote(cwd)) {
    throw new Error("No `origin` remote is configured.");
  }

  run([
    "git",
    "fetch",
    "origin",
    "+refs/jjk/states/*:refs/jjk/states/*",
  ], { cwd });
  importIntoJj(cwd);
}

export function pullFastForward(cwd: string): void {
  const branch = getCurrentBranch(cwd);
  const upstream = run(
    ["git", "rev-parse", "--abbrev-ref", `${branch}@{upstream}`],
    { cwd, allowFailure: true },
  );
  if (upstream.exitCode !== 0) {
    return;
  }

  run(["git", "pull", "--ff-only"], { cwd });
  importIntoJj(cwd);
}

export function createBundle(cwd: string, outputPath: string, revision: string): void {
  run(["git", "bundle", "create", outputPath, revision], { cwd });
}

export function pickStateChanges(
  cwd: string,
  parentCommit: string | null,
  commit: string,
): boolean {
  if (parentCommit) {
    return mergeChangedFilesFromState(cwd, parentCommit, commit);
  }

  const base = parentCommit ?? EMPTY_TREE;
  const patch = run(["git", "diff", "--binary", base, commit], { cwd }).stdout;
  if (patch.trim().length === 0) {
    return false;
  }

  const tempDir = mkdtempSync(join(tmpdir(), "jjk-pick-"));
  const patchPath = join(tempDir, "state.patch");

  try {
    writeFileSync(patchPath, `${patch}\n`);
    const plain = run(["git", "apply", patchPath], {
      cwd,
      allowFailure: true,
    });
    if (plain.exitCode === 0) {
      return true;
    }

    const threeWay = run(["git", "apply", "--3way", patchPath], {
      cwd,
      allowFailure: true,
    });
    if (threeWay.exitCode === 0) {
      return true;
    }

    const details = [plain.stderr, threeWay.stderr, plain.stdout, threeWay.stdout]
      .filter(Boolean)
      .join("\n");
    throw new Error(details || "Unable to apply picked state.");
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function mergeChangedFilesFromState(
  cwd: string,
  baseCommit: string,
  targetCommit: string,
): boolean {
  const changed = run(["git", "diff", "--name-only", baseCommit, targetCommit], {
    cwd,
  }).stdout
    .split("\n")
    .filter(Boolean);

  if (changed.length === 0) {
    return false;
  }

  let applied = false;

  for (const relativePath of changed) {
    const baseContent = readFileFromCommit(cwd, baseCommit, relativePath);
    const targetContent = readFileFromCommit(cwd, targetCommit, relativePath);
    const absolutePath = join(cwd, relativePath);
    const currentContent = existsSync(absolutePath)
      ? readFileSync(absolutePath, "utf8")
      : null;

    if (targetContent === null) {
      if (currentContent === baseContent && existsSync(absolutePath)) {
        unlinkSync(absolutePath);
        applied = true;
        continue;
      }
      throw new Error(`Cannot safely delete ${relativePath} while local content differs.`);
    }

    if (baseContent === null) {
      if (currentContent === null) {
        mkdirSync(dirname(absolutePath), { recursive: true });
        writeFileSync(absolutePath, targetContent);
        applied = true;
        continue;
      }
      if (currentContent === targetContent) {
        continue;
      }
      throw new Error(`Cannot safely add ${relativePath}; the path already exists.`);
    }

    if (currentContent === null) {
      throw new Error(`Cannot safely update ${relativePath}; it does not exist locally.`);
    }

    if (currentContent === targetContent) {
      continue;
    }

    if (currentContent === baseContent) {
      writeFileSync(absolutePath, targetContent);
      applied = true;
      continue;
    }

    const merged =
      mergeTextByEdits(cwd, currentContent, baseContent, targetContent) ??
      mergeTextByLine(currentContent, baseContent, targetContent) ??
      mergeTextFile(cwd, currentContent, baseContent, targetContent);
    writeFileSync(absolutePath, merged);
    applied = true;
  }

  return applied;
}

function readFileFromCommit(
  cwd: string,
  commit: string,
  relativePath: string,
): string | null {
  const proc = Bun.spawnSync(["git", "show", `${commit}:${relativePath}`], {
    cwd,
    stdout: "pipe",
    stderr: "pipe",
  });
  if (proc.exitCode !== 0) {
    return null;
  }
  return proc.stdout.toString();
}

function mergeTextFile(
  cwd: string,
  currentContent: string,
  baseContent: string,
  targetContent: string,
): string {
  const tempDir = mkdtempSync(join(tmpdir(), "jjk-merge-"));
  const currentPath = join(tempDir, "current.txt");
  const basePath = join(tempDir, "base.txt");
  const targetPath = join(tempDir, "target.txt");

  try {
    writeFileSync(currentPath, currentContent);
    writeFileSync(basePath, baseContent);
    writeFileSync(targetPath, targetContent);

    const proc = Bun.spawnSync(
      ["git", "merge-file", "-p", currentPath, basePath, targetPath],
      {
        cwd,
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const merged = proc.stdout.toString();

    if (proc.exitCode === 0) {
      return merged;
    }

    throw new Error(proc.stderr.toString().trim() || "Unable to merge picked text changes.");
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function mergeTextByLine(
  currentContent: string,
  baseContent: string,
  targetContent: string,
): string | null {
  const baseLines = splitLines(baseContent);
  const currentLines = splitLines(currentContent);
  const targetLines = splitLines(targetContent);

  if (
    baseLines.length !== currentLines.length ||
    baseLines.length !== targetLines.length
  ) {
    return null;
  }

  const merged: string[] = [];

  for (let index = 0; index < baseLines.length; index += 1) {
    const baseLine = baseLines[index];
    const currentLine = currentLines[index];
    const targetLine = targetLines[index];

    if (currentLine === targetLine) {
      merged.push(currentLine);
      continue;
    }

    if (currentLine === baseLine) {
      merged.push(targetLine);
      continue;
    }

    if (targetLine === baseLine) {
      merged.push(currentLine);
      continue;
    }

    return null;
  }

  const trailingNewline =
    currentContent.endsWith("\n") ||
    baseContent.endsWith("\n") ||
    targetContent.endsWith("\n");

  return `${merged.join("\n")}${trailingNewline ? "\n" : ""}`;
}

function splitLines(content: string): string[] {
  const lines = content.split("\n");
  if (lines.length > 0 && lines[lines.length - 1] === "") {
    lines.pop();
  }
  return lines;
}

interface TextEdit {
  start: number;
  end: number;
  newLines: string[];
}

function mergeTextByEdits(
  cwd: string,
  currentContent: string,
  baseContent: string,
  targetContent: string,
): string | null {
  const currentEdits = computeTextEdits(cwd, baseContent, currentContent);
  const targetEdits = computeTextEdits(cwd, baseContent, targetContent);
  if (!currentEdits || !targetEdits) {
    return null;
  }

  for (const targetEdit of targetEdits) {
    for (const currentEdit of currentEdits) {
      if (!editsOverlap(targetEdit, currentEdit)) {
        continue;
      }
      if (sameEdit(targetEdit, currentEdit)) {
        continue;
      }
      return null;
    }
  }

  const baseLines = splitLines(baseContent);
  const output: string[] = [];
  let position = 0;
  let currentIndex = 0;
  let targetIndex = 0;

  while (position <= baseLines.length) {
    const currentEdit = currentEdits[currentIndex] ?? null;
    const targetEdit = targetEdits[targetIndex] ?? null;

    if (
      currentEdit &&
      currentEdit.start === position &&
      currentEdit.end === position &&
      targetEdit &&
      targetEdit.start === position &&
      targetEdit.end === position
    ) {
      output.push(...currentEdit.newLines);
      currentIndex += 1;
      targetIndex += 1;
      continue;
    }

    if (currentEdit && currentEdit.start === position && currentEdit.end === position) {
      output.push(...currentEdit.newLines);
      currentIndex += 1;
      continue;
    }

    if (targetEdit && targetEdit.start === position && targetEdit.end === position) {
      output.push(...targetEdit.newLines);
      targetIndex += 1;
      continue;
    }

    if (currentEdit && currentEdit.start === position) {
      output.push(...currentEdit.newLines);
      position = currentEdit.end;
      currentIndex += 1;
      continue;
    }

    if (targetEdit && targetEdit.start === position) {
      output.push(...targetEdit.newLines);
      position = targetEdit.end;
      targetIndex += 1;
      continue;
    }

    if (position === baseLines.length) {
      break;
    }

    output.push(baseLines[position]);
    position += 1;
  }

  const trailingNewline =
    currentContent.endsWith("\n") ||
    baseContent.endsWith("\n") ||
    targetContent.endsWith("\n");

  return `${output.join("\n")}${trailingNewline ? "\n" : ""}`;
}

function computeTextEdits(
  cwd: string,
  baseContent: string,
  otherContent: string,
): TextEdit[] | null {
  const tempDir = mkdtempSync(join(tmpdir(), "jjk-diff-"));
  const basePath = join(tempDir, "base.txt");
  const otherPath = join(tempDir, "other.txt");

  try {
    writeFileSync(basePath, baseContent);
    writeFileSync(otherPath, otherContent);
    const proc = Bun.spawnSync(
      ["git", "diff", "--no-index", "--unified=0", "--no-color", basePath, otherPath],
      {
        cwd,
        stdout: "pipe",
        stderr: "pipe",
      },
    );

    if (proc.exitCode !== 0 && proc.exitCode !== 1) {
      return null;
    }

    const text = proc.stdout.toString();
    const lines = text.split("\n");
    const edits: TextEdit[] = [];
    let currentEdit: TextEdit | null = null;

    for (const line of lines) {
      if (line.startsWith("@@")) {
        if (currentEdit) {
          edits.push(currentEdit);
        }
        const match = /@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(line);
        if (!match) {
          return null;
        }
        const baseStart = Number.parseInt(match[1], 10);
        const baseCount = Number.parseInt(match[2] ?? "1", 10);
        const start = baseCount === 0 ? baseStart : baseStart - 1;
        currentEdit = {
          start,
          end: start + baseCount,
          newLines: [],
        };
        continue;
      }

      if (!currentEdit) {
        continue;
      }

      if (line.startsWith("+") && !line.startsWith("+++")) {
        currentEdit.newLines.push(line.slice(1));
      }
    }

    if (currentEdit) {
      edits.push(currentEdit);
    }

    return edits;
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

function editsOverlap(left: TextEdit, right: TextEdit): boolean {
  const leftInsertion = left.start === left.end;
  const rightInsertion = right.start === right.end;

  if (leftInsertion && rightInsertion) {
    return left.start === right.start;
  }

  if (leftInsertion) {
    return left.start > right.start && left.start < right.end;
  }

  if (rightInsertion) {
    return right.start > left.start && right.start < left.end;
  }

  return Math.max(left.start, right.start) < Math.min(left.end, right.end);
}

function sameEdit(left: TextEdit, right: TextEdit): boolean {
  return (
    left.start === right.start &&
    left.end === right.end &&
    left.newLines.join("\n") === right.newLines.join("\n")
  );
}
