import type { BootNotice } from "../lib/boot-notices";

type BootNoticeBannerProps = {
  notices: BootNotice[];
  onDismiss?: () => void;
  compact?: boolean;
};

export function BootNoticeBanner({ notices, onDismiss, compact = false }: BootNoticeBannerProps) {
  if (notices.length === 0) {
    return null;
  }

  return (
    <div
      className={compact ? "bootNoticeBanner bootNoticeBannerCompact" : "bootNoticeBanner"}
      role="alert"
      aria-live="polite"
    >
      <div className="bootNoticeBannerBody">
        <strong>{notices.length === 1 ? "Проблема при запуске" : "Проблемы при запуске"}</strong>
        <ul className="bootNoticeList">
          {notices.map((notice) => (
            <li key={notice.id}>{notice.message}</li>
          ))}
        </ul>
      </div>
      {onDismiss ? (
        <button type="button" className="bootNoticeDismiss" onClick={onDismiss} aria-label="Скрыть предупреждения запуска">
          ×
        </button>
      ) : null}
    </div>
  );
}
