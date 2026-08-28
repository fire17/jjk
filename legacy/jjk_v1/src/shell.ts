export interface RunOptions {
  cwd: string;
  env?: Record<string, string>;
  allowFailure?: boolean;
}

export interface RunResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

export function run(command: string[], options: RunOptions): RunResult {
  const proc = Bun.spawnSync(command, {
    cwd: options.cwd,
    env: {
      ...process.env,
      ...(options.env ?? {}),
    },
    stdout: "pipe",
    stderr: "pipe",
  });

  const result: RunResult = {
    stdout: proc.stdout.toString().trim(),
    stderr: proc.stderr.toString().trim(),
    exitCode: proc.exitCode,
  };

  if (result.exitCode !== 0 && !options.allowFailure) {
    const details = [result.stderr, result.stdout].filter(Boolean).join("\n");
    throw new Error(
      details.length > 0
        ? details
        : `Command failed: ${command.join(" ")}`,
    );
  }

  return result;
}
