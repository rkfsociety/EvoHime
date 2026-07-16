import type { GithubAuthInfo, PullRequestScope, PullRequestSummary } from "../types";
import { apiRequest } from "./client";

export type GithubPullRequestComment = {
  author: { login: string } | null;
  body: string;
  createdAt: string | null;
  url: string | null;
  state: string | null;
};

export type GithubCheck = {
  name: string;
  status: string | null;
  conclusion: string | null;
  detailsUrl: string | null;
  workflowName: string | null;
};

export type GithubPullRequestDetail = PullRequestSummary & {
  body: string;
  isDraft: boolean;
  mergeStateStatus: string | null;
  diff: string;
  comments: GithubPullRequestComment[];
  reviews: GithubPullRequestComment[];
  checks: GithubCheck[];
};

export type GithubCreatePullRequestRequest = {
  title: string;
  body: string;
  base?: string;
  head?: string;
};

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

export function getPullRequest(number: number) {
  return apiRequest<GithubPullRequestDetail>(
    `/api/github/pull-requests/${number}`,
    undefined,
    "Не удалось загрузить pull request",
  );
}

export function createPullRequest(request: GithubCreatePullRequestRequest) {
  return apiRequest<GithubPullRequestDetail>(
    "/api/github/pull-requests",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    },
    "Не удалось создать pull request",
  );
}
