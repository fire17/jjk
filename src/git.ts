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
}

export function exportFromJj(cwd: string): void {
  if (!isJjRepo(cwd)) {
    return;
  }
  run(["jj", "git", "export"], { cwd, allowFailure: true });
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
): {
  commit: string;
  parentCommit: string | null;
  changedFiles: number;
} {
  exportFromJj(cwd);
  const parentCommit = getHeadCommit(cwd);
  run(["git", "add", "--all", "--", "."], { cwd });
  const changedFiles = countStatusEntries(cwd);
  run(["git", "commit", "--allow-empty", "-m", message], { cwd });
  const commit = run(["git", "rev-parse", "--verify", "HEAD"], { cwd }).stdout;
  return { commit, parentCommit, changedFiles };
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
