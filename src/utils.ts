import { randomUUID } from "node:crypto";
import { relative } from "node:path";
import type { StateKind, StateMatch, StateRecord } from "./types";

export function nowIso(): string {
  return new Date().toISOString();
}

export function shortId(): string {
  return randomUUID().replace(/-/g, "").slice(0, 8);
}

export function slugify(value: string): string {
  const slug = value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "state";
}

export function defaultLabel(kind: StateKind, description: string): string {
  const trimmed = description.trim();
  if (trimmed.length === 0) {
    return kind;
  }

  const base = trimmed.length > 48 ? `${trimmed.slice(0, 45)}...` : trimmed;
  return `${kind} ${base}`.trim();
}

export function formatDate(value: string): string {
  return new Intl.DateTimeFormat("en", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

export function formatRelativePath(root: string, target: string): string {
  const rel = relative(root, target);
  return rel.length === 0 ? "." : rel;
}

function fuzzyScore(query: string, candidate: string): number {
  const q = query.toLowerCase().trim();
  const c = candidate.toLowerCase();

  if (q.length === 0) {
    return 0;
  }

  if (c === q) {
    return 10_000;
  }

  if (c.includes(q)) {
    return 5_000 - (c.indexOf(q) * 5) - Math.abs(c.length - q.length);
  }

  let qi = 0;
  let score = 0;
  let streak = 0;

  for (let ci = 0; ci < c.length; ci += 1) {
    if (c[ci] === q[qi]) {
      qi += 1;
      streak += 1;
      score += 25 + streak * 5;
      if (qi === q.length) {
        return score;
      }
    } else {
      streak = 0;
    }
  }

  return -1;
}

export function findStateMatches(
  states: StateRecord[],
  query: string,
): StateMatch[] {
  return states
    .map((state) => {
      const corpus = [
        state.id,
        state.kind,
        state.label,
        state.description,
        state.branch,
        state.lane,
      ].join(" ");
      const score = fuzzyScore(query, corpus);
      return { state, score };
    })
    .filter((match) => match.score >= 0)
    .sort((left, right) => right.score - left.score);
}

export function ensureDescription(kind: StateKind, description: string): string {
  const trimmed = description.trim();
  if (trimmed.length > 0) {
    return trimmed;
  }

  switch (kind) {
    case "step":
      return "small meaningful checkpoint";
    case "nice":
      return "good place worth remembering";
    case "star":
      return "memorable anchor state";
    case "auto":
      return "auto grouped state";
    default:
      return "saved state";
  }
}

export function parseWords(input: string): string[] {
  const values = input.match(/"([^"]*)"|'([^']*)'|[^\s]+/g) ?? [];
  return values.map((value) => value.replace(/^['"]|['"]$/g, ""));
}

export function pad(value: string, length: number): string {
  return value.length >= length ? value : `${value}${" ".repeat(length - value.length)}`;
}
