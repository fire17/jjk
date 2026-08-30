import { watch } from "node:fs";
import { relative } from "node:path";
import { saveState } from "./store";
import { shortStateId } from "./utils";

function shouldIgnore(pathname: string): boolean {
  return (
    pathname.startsWith(".git/") ||
    pathname.startsWith(".jj/") ||
    pathname.startsWith(".jjk/") ||
    pathname.includes("node_modules/")
  );
}

export async function runWatch(root: string, debounceMs: number): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let lastPath = "workspace";

  console.log(`Watching ${root}`);
  console.log(`Press Ctrl+C to stop.`);

  const watcher = watch(root, { recursive: true }, (_event, filename) => {
    if (!filename) {
      return;
    }

    const rel = relative(root, `${root}/${filename}`);
    if (shouldIgnore(rel)) {
      return;
    }

    lastPath = rel;
    if (timer) {
      clearTimeout(timer);
    }

    timer = setTimeout(() => {
      const result = saveState(root, {
        kind: "auto",
        description: `auto grouped change near ${lastPath}`,
      });
      console.log(`saved ${shortStateId(result.state.id)} ${result.state.label}`);
    }, debounceMs);
  });

  await new Promise<void>((resolve) => {
    process.on("SIGINT", () => {
      watcher.close();
      if (timer) {
        clearTimeout(timer);
      }
      resolve();
    });
  });
}
