import type { ReactNode } from "react";
import { AgentBrand } from "./AgentBrand";
import { useModalA11y } from "../hooks/useModalA11y";

type SettingsModalProps = {
  onClose: () => void;
  children: ReactNode;
};

export function SettingsModal({ onClose, children }: SettingsModalProps) {
  const modalRef = useModalA11y<HTMLElement>(true, onClose);

  return (
    <div className="settingsBackdrop" onClick={onClose} role="presentation">
      <section
        ref={modalRef}
        className="settingsModal"
        onClick={(event) => event.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-modal-title"
        tabIndex={-1}
      >
        <header className="settingsModalHeader">
          <div>
            <span className="sidebarFooterLabel">Настройки</span>
            <AgentBrand title="Параметры EvoHime" as="h2" markSize="sm" titleId="settings-modal-title" />
          </div>
          <button
            type="button"
            className="settingsCloseButton"
            onClick={onClose}
            aria-label="Закрыть настройки"
          >
            Закрыть
          </button>
        </header>
        <div className="settingsModalBody">{children}</div>
      </section>
    </div>
  );
}
