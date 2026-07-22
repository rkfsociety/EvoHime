import { Component, type ErrorInfo, type ReactNode } from "react";

type PanelErrorBoundaryProps = {
  panelLabel: string;
  children: ReactNode;
};

type PanelErrorBoundaryState = {
  error: Error | null;
};

export class PanelErrorBoundary extends Component<PanelErrorBoundaryProps, PanelErrorBoundaryState> {
  state: PanelErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): PanelErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(`Ошибка панели «${this.props.panelLabel}»`, error, info.componentStack);
  }

  private retry = () => {
    this.setState({ error: null });
  };

  render() {
    if (!this.state.error) {
      return this.props.children;
    }

    return (
      <section className="panelErrorState" role="alert">
        <strong>Панель «{this.props.panelLabel}» не загрузилась</strong>
        <span>Остальная рабочая область продолжает работать.</span>
        <button type="button" onClick={this.retry}>Повторить</button>
      </section>
    );
  }
}
