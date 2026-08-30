import { createInterface } from "node:readline/promises";
import { stdin, stdout } from "node:process";
import { runCli } from "./commands";
import { parseWords } from "./utils";
import { JJK_VERSION } from "./version";

export async function runRepl(cwd: string): Promise<void> {
  const rl = createInterface({ input: stdin, output: stdout });
  console.log(`jjk interactive shell (${JJK_VERSION})`);
  console.log("Type `help` for commands, `exit` to quit.");

  while (true) {
    const line = (await rl.question("jjk> ")).trim();
    if (line.length === 0) {
      continue;
    }

    if (line === "exit" || line === "quit" || line === ".exit") {
      rl.close();
      return;
    }

    const args = parseWords(line);
    await runCli(args, cwd);
  }
}
