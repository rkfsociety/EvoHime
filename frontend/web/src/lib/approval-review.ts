import type {
  ApprovalRequiredEvent,
  UnifiedDiffReview,
  FileWriteReview,
  UnavailableReview,
} from "../protocol";
import { isRememberableApprovalScope } from "./approval-scope.ts";

export type PatchReviewRequest = ApprovalRequiredEvent & {
  review: UnifiedDiffReview;
};

export function isPatchReview(request: ApprovalRequiredEvent): request is PatchReviewRequest {
  return request.tool_name === "filesystem.patch" && request.review?.kind === "unified_diff";
}

export type FileWriteReviewRequest = ApprovalRequiredEvent & {
  review: FileWriteReview;
};

export function isFileWriteReview(
  request: ApprovalRequiredEvent,
): request is FileWriteReviewRequest {
  return request.review?.kind === "file_write";
}

export type UnavailableReviewRequest = ApprovalRequiredEvent & {
  review: UnavailableReview;
};

export function isUnavailableReview(
  request: ApprovalRequiredEvent,
): request is UnavailableReviewRequest {
  return request.review?.kind === "unavailable";
}

export function canRememberApprovalPath(request: ApprovalRequiredEvent) {
  return !isPatchReview(request) && isRememberableApprovalScope(request.scope);
}
