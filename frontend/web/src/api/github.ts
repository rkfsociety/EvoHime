import type { GithubAuthInfo, PullRequestScope, PullRequestSummary } from "../types";
import { apiRequest } from "./client";

export function getGithubAuth() {
  return apiRequest<GithubAuthInfo>(
    "/api/auth/github",
    undefined,
    "Не удалось получить GitHub auth",
  );
}

export function listPullRequests(scope: PullRequestScope) {
  return apiRequest<PullRequestSummary[]>(
    `/api/github/pull-requests?scope=${encodeURIComponent(scope)}`,
    undefined,
    "Не удалось загрузить pull request'ы",
  );
}
