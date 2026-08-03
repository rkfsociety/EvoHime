import assert from "node:assert/strict";
import test from "node:test";
import {
  canRememberApprovalPath,
  isFileWriteReview,
  isPatchReview,
  isUnavailableReview,
} from "./approval-review.ts";

const baseRequest = {
  type: "approval.required",
  approval_id: "approval-id",
  task_id: "task-id",
  tool_name: "filesystem.write",
  permission: "filesystem.write",
  scope: "src/lib.rs",
  risk_level: "medium",
  created_at: "2026-07-29T00:00:00Z",
};

test("patch reviews are recognized and never remember their path", () => {
  const request = {
    ...baseRequest,
    tool_name: "filesystem.patch",
    review: {
      kind: "unified_diff",
      path: "src/lib.rs",
      diff: "@@ -1 +1 @@\n-old\n+new",
    },
  };

  assert.equal(isPatchReview(request), true);
  assert.equal(canRememberApprovalPath(request), false);
});

test("ordinary path approvals remain rememberable", () => {
  assert.equal(isPatchReview(baseRequest), false);
  assert.equal(canRememberApprovalPath(baseRequest), true);
});

test("non-path approval scopes remain one-shot", () => {
  assert.equal(canRememberApprovalPath({ ...baseRequest, scope: "workspace" }), false);
});

test("file_write reviews are recognized", () => {
  const request = {
    ...baseRequest,
    review: { kind: "file_write", path: "new.txt", change: "create", new_bytes: 5 },
  };

  assert.equal(isFileWriteReview(request), true);
});

test("patch reviews are not mistaken for file_write reviews", () => {
  const request = {
    ...baseRequest,
    tool_name: "filesystem.patch",
    review: { kind: "unified_diff", path: "a.rs", diff: "@@ -1 +1 @@" },
  };

  assert.equal(isFileWriteReview(request), false);
  assert.equal(isPatchReview(request), true);
});

test("unavailable reviews are recognized", () => {
  const request = {
    ...baseRequest,
    tool_name: "shell.execute",
    review: { kind: "unavailable", reason: "shell command execution cannot be safely predicted" },
  };

  assert.equal(isUnavailableReview(request), true);
});

test("no review present is not mistaken for an unavailable review", () => {
  assert.equal(isUnavailableReview(baseRequest), false);
});
