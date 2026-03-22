import { afterEach, describe, expect, test } from "bun:test";
import { runCli } from "../src/commands";

describe("version output", () => {
  const originalLog = console.log;

  afterEach(() => {
    console.log = originalLog;
  });

  test("prints the stable version with -v", async () => {
    const logs: string[] = [];
    console.log = (...args: unknown[]) => {
      logs.push(args.join(" "));
    };

    await runCli(["-v"], process.cwd());

    expect(logs).toEqual(["0.1.1-Stable"]);
  });

  test("prints the stable version with --version", async () => {
    const logs: string[] = [];
    console.log = (...args: unknown[]) => {
      logs.push(args.join(" "));
    };

    await runCli(["--version"], process.cwd());

    expect(logs).toEqual(["0.1.1-Stable"]);
  });
});
