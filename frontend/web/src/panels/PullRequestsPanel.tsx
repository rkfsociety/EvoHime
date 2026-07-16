import { useState } from "react";
import type { PullRequestScope, PullRequestSummary } from "../types";
import {
  createPullRequest,
  getPullRequest,
  type GithubPullRequestDetail,
} from "../api/github";
import { formatRelativeAge } from "../lib/format";

type PullRequestsPanelProps = {
  githubLogin: string | null;
  pullRequestSearch: string;
  pullRequestScope: PullRequestScope;
  pullRequestsLoading: boolean;
  pullRequestsError: string | null;
  visiblePullRequests: PullRequestSummary[];
  onSearchChange: (value: string) => void;
  onScopeChange: (scope: PullRequestScope) => void;
};

function checkLabel(status: string | null, conclusion: string | null) {
  return conclusion ?? status ?? "unknown";
}

export function PullRequestsPanel({
  githubLogin,
  pullRequestSearch,
  pullRequestScope,
  pullRequestsLoading,
  pullRequestsError,
  visiblePullRequests,
  onSearchChange,
  onScopeChange,
}: PullRequestsPanelProps) {
  const [selectedNumber, setSelectedNumber] = useState<number | null>(null);
  const [detail, setDetail] = useState<GithubPullRequestDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [createTitle, setCreateTitle] = useState("");
  const [createBody, setCreateBody] = useState("");
  const [createBase, setCreateBase] = useState("");
  const [createHead, setCreateHead] = useState("");
  const [createLoading, setCreateLoading] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);

  const openPullRequest = async (number: number) => {
    setSelectedNumber(number);
    setDetail(null);
    setDetailError(null);
    setDetailLoading(true);
    try {
      setDetail(await getPullRequest(number));
    } catch (error) {
      setDetailError(String(error));
    } finally {
      setDetailLoading(false);
    }
  };

  const submitCreate = async () => {
    if (!createTitle.trim()) {
      setCreateError("Укажи заголовок pull request");
      return;
    }
    setCreateLoading(true);
    setCreateError(null);
    try {
      const created = await createPullRequest({
        title: createTitle.trim(),
        body: createBody,
        ...(createBase.trim() ? { base: createBase.trim() } : {}),
        ...(createHead.trim() ? { head: createHead.trim() } : {}),
      });
      setDetail(created);
      setSelectedNumber(created.number);
      setCreateOpen(false);
      setCreateTitle("");
      setCreateBody("");
      setCreateBase("");
      setCreateHead("");
    } catch (error) {
      setCreateError(String(error));
    } finally {
      setCreateLoading(false);
    }
  };

  if (selectedNumber !== null) {
    return (
      <div className="pullRequestsPage">
        <button
          type="button"
          className="secondaryButton"
          onClick={() => {
            setSelectedNumber(null);
            setDetail(null);
            setDetailError(null);
          }}
        >
          ← К списку PR
        </button>

        {detailLoading ? <p className="pullRequestsEmpty">Загружаю PR, diff и проверки...</p> : null}
        {detailError ? <p className="pullRequestsError">{detailError}</p> : null}
        {detail ? (
          <section className="pullRequestDetail">
            <div className="pullRequestDetailHeader">
              <div>
                <span className="pullRequestState">{detail.state}</span>
                <h3>#{detail.number} {detail.title}</h3>
                <p>
                  {detail.author?.login ?? "unknown"} · {detail.headRefName} → {detail.baseRefName}
                </p>
              </div>
              <a href={detail.url} target="_blank" rel="noreferrer" className="secondaryButton">
                Открыть на GitHub
              </a>
            </div>

            <div className="pullRequestDetailGrid">
              <section className="pullRequestDetailCard">
                <h4>Описание</h4>
                <p className="pullRequestMarkdown">{detail.body || "Описание отсутствует."}</p>
                <p className="pullRequestMuted">
                  {detail.isDraft ? "Draft" : "Ready for review"} · merge state: {detail.mergeStateStatus ?? "unknown"}
                </p>
              </section>

              <section className="pullRequestDetailCard">
                <h4>Checks ({detail.checks.length})</h4>
                {detail.checks.length === 0 ? <p className="pullRequestMuted">Проверок пока нет.</p> : null}
                {detail.checks.map((check) => (
                  <div className="pullRequestCheck" key={`${check.name}-${check.workflowName ?? ""}`}>
                    <strong>{check.name}</strong>
                    <span>{checkLabel(check.status, check.conclusion)}</span>
                    {check.detailsUrl ? <a href={check.detailsUrl} target="_blank" rel="noreferrer">Открыть</a> : null}
                  </div>
                ))}
              </section>
            </div>

            <section className="pullRequestDetailCard">
              <h4>Комментарии ({detail.comments.length})</h4>
              {detail.comments.length === 0 ? <p className="pullRequestMuted">Комментариев нет.</p> : null}
              {detail.comments.map((comment, index) => (
                <article className="pullRequestComment" key={`${comment.createdAt ?? "comment"}-${index}`}>
                  <strong>{comment.author?.login ?? "unknown"}</strong>
                  <span>{comment.createdAt ? formatRelativeAge(comment.createdAt) : ""}</span>
                  <p>{comment.body}</p>
                </article>
              ))}
            </section>

            <section className="pullRequestDetailCard">
              <h4>Review comments ({detail.reviews.length})</h4>
              {detail.reviews.length === 0 ? <p className="pullRequestMuted">Review-комментариев нет.</p> : null}
              {detail.reviews.map((review, index) => (
                <article className="pullRequestComment" key={`${review.createdAt ?? "review"}-${index}`}>
                  <strong>{review.author?.login ?? "unknown"}</strong>
                  <span>{review.state ?? "review"}</span>
                  <p>{review.body}</p>
                </article>
              ))}
            </section>

            <section className="pullRequestDetailCard">
              <h4>Diff</h4>
              <pre className="pullRequestDiff">{detail.diff || "Diff отсутствует."}</pre>
            </section>
          </section>
        ) : null}
      </div>
    );
  }

  return (
    <div className="pullRequestsPage">
      <section className="pullRequestsHero">
        <div>
          <h3>Пулл-реквесты</h3>
          <p>Просматривай PR, diff, comments и CI checks от имени {githubLogin ?? "твоего аккаунта"}.</p>
        </div>
        <div className="pullRequestsMeta">
          <strong>{pullRequestsLoading ? "…" : `${visiblePullRequests.length}`}</strong>
          <span>pull request'ов</span>
        </div>
      </section>

      <div className="pullRequestsSearchRow">
        <label className="pullRequestsSearch">
          <span>Поиск pull-request'ов</span>
          <input value={pullRequestSearch} onChange={(event) => onSearchChange(event.target.value)} placeholder="Поиск pull-request'ов" />
        </label>
        <button type="button" className="secondaryButton" onClick={() => setCreateOpen((value) => !value)}>
          {createOpen ? "Закрыть" : "Создать PR"}
        </button>
      </div>

      {createOpen ? (
        <section className="pullRequestDetailCard pullRequestCreateForm">
          <h4>Новый pull request</h4>
          <input value={createTitle} onChange={(event) => setCreateTitle(event.target.value)} placeholder="Заголовок" />
          <textarea value={createBody} onChange={(event) => setCreateBody(event.target.value)} placeholder="Описание" rows={5} />
          <div className="pullRequestFormRow">
            <input value={createHead} onChange={(event) => setCreateHead(event.target.value)} placeholder="Head branch (необязательно)" />
            <input value={createBase} onChange={(event) => setCreateBase(event.target.value)} placeholder="Base branch (необязательно)" />
          </div>
          {createError ? <p className="pullRequestsError">{createError}</p> : null}
          <button type="button" className="primaryButton" disabled={createLoading} onClick={() => void submitCreate()}>
            {createLoading ? "Создаю..." : "Создать pull request"}
          </button>
        </section>
      ) : null}

      <div className="pullRequestsTabs">
        <button type="button" className={pullRequestScope === "all" ? "pullRequestsTab active" : "pullRequestsTab"} onClick={() => onScopeChange("all")}>Все</button>
        <button type="button" className={pullRequestScope === "review_requested" ? "pullRequestsTab active" : "pullRequestsTab"} onClick={() => onScopeChange("review_requested")}>Проверяемые мной</button>
        <button type="button" className={pullRequestScope === "created" ? "pullRequestsTab active" : "pullRequestsTab"} onClick={() => onScopeChange("created")}>Созданные мной</button>
      </div>

      <div className="pullRequestsBody">
        {pullRequestsError ? <p className="pullRequestsError">{pullRequestsError}</p> : null}
        <div className="pullRequestsList">
          {visiblePullRequests.length === 0 ? (
            <div className="pullRequestsEmpty">
              <strong>Пока нет pull request'ов</strong>
              <p>{pullRequestsLoading ? "Подтягиваю список из GitHub..." : "PR появятся здесь после загрузки репозитория."}</p>
            </div>
          ) : (
            visiblePullRequests.map((pullRequest) => (
              <button type="button" key={pullRequest.number} className="pullRequestItem" onClick={() => void openPullRequest(pullRequest.number)}>
                <div className="pullRequestLine">
                  <strong>{pullRequest.title}</strong>
                  <span>{formatRelativeAge(pullRequest.updatedAt)}</span>
                </div>
                <div className="pullRequestSubline">
                  <span>{pullRequest.author?.login ?? "unknown"} / {pullRequest.headRefName}</span>
                  <span>{pullRequest.baseRefName}</span>
                </div>
                <div className="pullRequestFooter">
                  <span className="pullRequestState">{pullRequest.state}</span>
                  <span>#{pullRequest.number}</span>
                </div>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
