import type { PullRequestScope, PullRequestSummary } from "../types";
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
  return (
    <div className="pullRequestsPage">
      <section className="pullRequestsHero">
        <div>
          <h3>Пул-реквесты</h3>
          <p>
            Просматривайте и отслеживайте работу на GitHub от имени {githubLogin ?? "вашего аккаунта"}.
          </p>
        </div>
        <div className="pullRequestsMeta">
          <strong>{pullRequestsLoading ? "…" : `${visiblePullRequests.length}`}</strong>
          <span>pull request'ов</span>
        </div>
      </section>

      <div className="pullRequestsSearchRow">
        <label className="pullRequestsSearch">
          <span>Поиск pull-request'ов</span>
          <input
            value={pullRequestSearch}
            onChange={(event) => onSearchChange(event.target.value)}
            placeholder="Поиск pull-request'ов"
          />
        </label>
        <button type="button" className="pullRequestsFilterButton" aria-label="Фильтр">
          ⌕
        </button>
      </div>

      <div className="pullRequestsTabs">
        <button
          type="button"
          className={pullRequestScope === "all" ? "pullRequestsTab active" : "pullRequestsTab"}
          onClick={() => onScopeChange("all")}
        >
          Все
        </button>
        <button
          type="button"
          className={pullRequestScope === "review_requested" ? "pullRequestsTab active" : "pullRequestsTab"}
          onClick={() => onScopeChange("review_requested")}
        >
          Проверяемые мной
        </button>
        <button
          type="button"
          className={pullRequestScope === "created" ? "pullRequestsTab active" : "pullRequestsTab"}
          onClick={() => onScopeChange("created")}
        >
          Созданные мной
        </button>
      </div>

      <div className="pullRequestsBody">
        {pullRequestsError ? <p className="pullRequestsError">{pullRequestsError}</p> : null}
        <div className="pullRequestsList">
          {visiblePullRequests.length === 0 ? (
            <div className="pullRequestsEmpty">
              <strong>Пока нет pull request'ов</strong>
              <p>
                {pullRequestsLoading
                  ? "Подтягиваю список из GitHub..."
                  : "Если в репозитории будут pull request'ы, они появятся здесь."}
              </p>
            </div>
          ) : (
            visiblePullRequests.map((pullRequest) => (
              <a
                key={pullRequest.number}
                className="pullRequestItem"
                href={pullRequest.url}
                target="_blank"
                rel="noreferrer"
              >
                <div className="pullRequestLine">
                  <strong>{pullRequest.title}</strong>
                  <span>{formatRelativeAge(pullRequest.updatedAt)}</span>
                </div>
                <div className="pullRequestSubline">
                  <span>
                    {pullRequest.author?.login ?? "unknown"} / {pullRequest.headRefName}
                  </span>
                  <span>{pullRequest.baseRefName}</span>
                </div>
                <div className="pullRequestFooter">
                  <span className="pullRequestState">{pullRequest.state}</span>
                  <span>#{pullRequest.number}</span>
                </div>
              </a>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
