#!/usr/bin/env bun
import { runCli } from "./commands";

try {
  await runCli(process.argv.slice(2), process.cwd());
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`jjk error: ${message}`);
  process.exit(1);
}
