import { describe, expect, test } from "bun:test";
import { defaultLabel, parseWords, slugify } from "../src/utils";

describe("utils", () => {
  test("slugify normalizes names", () => {
    expect(slugify("Feature Harvest Lane")).toBe("feature-harvest-lane");
  });

  test("defaultLabel includes kind", () => {
    expect(defaultLabel("nice", "green tests after parser cleanup")).toContain("nice");
  });

  test("parseWords keeps quoted content together", () => {
    expect(parseWords('return "last good place"')).toEqual([
      "return",
      "last good place",
    ]);
  });
});
