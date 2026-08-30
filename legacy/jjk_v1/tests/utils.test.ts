import { describe, expect, test } from "bun:test";
import { branchSegment, defaultLabel, parseStateLabelAndMessage, parseWords, slugify } from "../src/utils";

describe("utils", () => {
  test("slugify normalizes names", () => {
    expect(slugify("Feature Harvest Lane")).toBe("feature-harvest-lane");
  });

  test("branchSegment keeps readable branch names with underscores", () => {
    expect(branchSegment("Fast mode only")).toBe("Fast_mode_only");
  });

  test("defaultLabel uses the description without a kind prefix", () => {
    expect(defaultLabel("nice", "green tests after parser cleanup")).toBe(
      "green tests after parser cleanup",
    );
  });

  test("parseWords keeps quoted content together", () => {
    expect(parseWords('return "last good place"')).toEqual([
      "return",
      "last good place",
    ]);
  });

  test("parseStateLabelAndMessage splits label and message on the first comma", () => {
    expect(parseStateLabelAndMessage("green, parser rewrite in progress")).toEqual({
      description: "green",
      label: "green",
      message: "parser rewrite in progress",
    });
  });

  test("parseStateLabelAndMessage keeps plain labels untouched", () => {
    expect(parseStateLabelAndMessage("green parser rewrite")).toEqual({
      description: "green parser rewrite",
    });
  });
});
