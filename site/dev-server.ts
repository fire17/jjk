#!/usr/bin/env bun
import { extname, join } from "node:path";

const root = join(import.meta.dir);

const contentTypes: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "application/javascript; charset=utf-8",
};

Bun.serve({
  port: 4173,
  async fetch(request) {
    const url = new URL(request.url);
    const pathname = url.pathname === "/" ? "/index.html" : url.pathname;
    const file = Bun.file(join(root, pathname));

    if (!(await file.exists())) {
      return new Response("Not found", { status: 404 });
    }

    return new Response(file, {
      headers: {
        "content-type": contentTypes[extname(pathname)] ?? "text/plain; charset=utf-8",
      },
    });
  },
});

console.log("jjk site on http://127.0.0.1:4173");
