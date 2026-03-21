import { afterEach, describe, expect, test } from "bun:test";
import { runCli } from "../src/commands";

describe("version output", () => {
  const originalLog = console.log;

  afterEach(() => {
    console.log = originalLog;
  });

  test("prints the jjk_v1 version with -v", async () => {
    const logs: string[] = [];
    console.log = (...args: unknown[]) => {
      logs.push(args.join(" "));
    };

    await runCli(["-v"], process.cwd());

    expect(logs).toEqual(["0.0.1_jjk_v1"]);
  });

  test("prints the jjk_v1 version with --version", async () => {
    const logs: string[] = [];
    console.log = (...args: unknown[]) => {
      logs.push(args.join(" "));
    };

    await runCli(["--version"], process.cwd());

    expect(logs).toEqual(["0.0.1_jjk_v1"]);
  });
});
