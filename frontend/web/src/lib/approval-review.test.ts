import { describe, expect, it } from "vitest";
import type { ApprovalRequiredEvent } from "../protocol";
import { isFileWriteReview, isUnavailableReview, isPatchReview } from "./approval-review";

const baseEvent: Omit<ApprovalRequiredEvent, "review"> = {
  type: "approval.required",
  approval_id: "00000000-0000-0000-0000-000000000000",
  task_id: "00000000-0000-0000-0000-000000000000",
  tool_name: "filesystem.write",
  permission: "filesystem_write",
  scope: "new.txt",
  risk_level: "medium",
  created_at: new Date().toISOString(),
};

describe("isFileWriteReview", () => {
  it("returns true for a file_write review", () => {
    const request: ApprovalRequiredEvent = {
      ...baseEvent,
      review: { kind: "file_write", path: "new.txt", change: "create", new_bytes: 5 },
    };
    expect(isFileWriteReview(request)).toBe(true);
  });

  it("returns false for a patch review", () => {
    const request: ApprovalRequiredEvent = {
      ...baseEvent,
      tool_name: "filesystem.patch",
      review: { kind: "unified_diff", path: "a.rs", diff: "@@ -1 +1 @@" },
    };
    expect(isFileWriteReview(request)).toBe(false);
    expect(isPatchReview(request)).toBe(true);
  });
});

describe("isUnavailableReview", () => {
  it("returns true for an unavailable review", () => {
    const request: ApprovalRequiredEvent = {
      ...baseEvent,
      tool_name: "shell.execute",
      review: { kind: "unavailable", reason: "shell command execution cannot be safely predicted" },
    };
    expect(isUnavailableReview(request)).toBe(true);
  });

  it("returns false when review is absent", () => {
    const request: ApprovalRequiredEvent = { ...baseEvent, review: undefined };
    expect(isUnavailableReview(request)).toBe(false);
  });
});
