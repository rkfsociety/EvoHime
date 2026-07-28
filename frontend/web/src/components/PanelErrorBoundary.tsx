import { Component, type ErrorInfo, type ReactNode } from "react";
import { CopyButton } from "./CopyButton";

type PanelErrorBoundaryProps = {
  panelLabel: string;
  children: ReactNode;
};

type PanelErrorBoundaryState = {
  error: Error | null;
  componentStack: string | null;
};

export class PanelErrorBoundary extends Component<PanelErrorBoundaryProps, PanelErrorBoundaryState> {
  state: PanelErrorBoundaryState = { error: null, componentStack: null };

  static getDerivedStateFromError(error: Error): Partial<PanelErrorBoundaryState> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.setState({ componentStack: info.componentStack ?? null });
    console.error(`Ошибка панели «${this.props.panelLabel}»`, error, info.componentStack);
  }

  private retry = () => {
    this.setState({ error: null, componentStack: null });
  };

  render() {
    const { error, componentStack } = this.state;
    if (!error) {
      return this.props.children;
    }

    const trace = [error.stack ?? error.message, componentStack].filter(Boolean).join("\n\n");

    return (
      <section className="panelErrorState" role="alert">
        <strong>Панель «{this.props.panelLabel}» не загрузилась</strong>
        <span>Остальная рабочая область продолжает работать.</span>
        <details className="panelErrorDetails">
          <summary>Подробности ошибки</summary>
          <pre>{trace}</pre>
          <CopyButton value={trace} />
        </details>
        <button type="button" onClick={this.retry}>Повторить</button>
      </section>
    );
  }
}
