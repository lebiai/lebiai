import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "./ui";
import { useUiStore } from "../../store/uiStore";

interface Props {
  children: ReactNode;
  /** Optional label for debugging which region failed. */
  region?: string;
}

interface State {
  error: Error | null;
}

/**
 * Catch render errors so a single panel cannot blank the whole window.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(`[ErrorBoundary${this.props.region ? `:${this.props.region}` : ""}]`, error, info);
  }

  render() {
    if (this.state.error) {
      const t = useUiStore.getState().t;
      return (
        <div className="flex flex-col items-center justify-center h-full min-h-[12rem] px-6 text-center gap-3">
          <p className="text-sm font-medium text-app-fg dark:text-slate-100">
            {t("error.boundaryTitle")}
          </p>
          <p className="text-xs text-app-fg-secondary dark:text-slate-400 max-w-md break-words">
            {this.state.error.message}
          </p>
          <Button
            size="sm"
            variant="secondary"
            onClick={() => this.setState({ error: null })}
          >
            {t("error.boundaryRetry")}
          </Button>
        </div>
      );
    }
    return this.props.children;
  }
}
