import type { ApprovalRequiredEvent, UnifiedDiffReview } from "../protocol";
import { isRememberableApprovalScope } from "./approval-scope.ts";

export type PatchReviewRequest = ApprovalRequiredEvent & {
  review: UnifiedDiffReview;
};

export function isPatchReview(request: ApprovalRequiredEvent): request is PatchReviewRequest {
  return request.tool_name === "filesystem.patch" && request.review?.kind === "unified_diff";
}

export function canRememberApprovalPath(request: ApprovalRequiredEvent) {
  return !isPatchReview(request) && isRememberableApprovalScope(request.scope);
}
