import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Sparkles } from "lucide-react";
import { useChatStore } from "../../store/chatStore";
import { useUiStore } from "../../store/uiStore";
import type { ReflectionResult } from "../../types";
import { ReflectionReview } from "./ReflectionReview";
import { Button, EmptyState, PanelShell, ui } from "../common/ui";
import { toast } from "../../utils/toast";

/**
 * Advanced surface: optional extract from the current chat.
 * Not a primary product path — quiet chrome, workbench tokens (primary CTA, neutral empty).
 */
export function ReflectPanel() {
  const { activeSessionId } = useChatStore();
  const t = useUiStore((state) => state.t);
  const [result, setResult] = useState<ReflectionResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Switching chat clears review — avoid stale candidates from another session.
  useEffect(() => {
    setResult(null);
    setError(null);
    setLoading(false);
  }, [activeSessionId]);

  const handleRun = async () => {
    if (!activeSessionId) {
      setError(t("reflect.noActiveSession"));
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const r = await invoke<ReflectionResult>("run_reflection", {
        sessionId: activeSessionId,
      });
      setResult(r);
      toast.success(t("toast.reflectDone"));
    } catch (e) {
      const msg = String(e);
      setError(msg);
      toast.error(msg);
    } finally {
      setLoading(false);
    }
  };

  return (
    <PanelShell
      title={t("reflect.title")}
      subtitle={t("reflect.evolutionHint")}
      actions={
        <div className="flex items-center gap-2">
          <span className={`${ui.sectionLabel} hidden sm:inline`}>
            {t("nav.advanced")}
          </span>
          <Button
            size="sm"
            variant="secondary"
            onClick={handleRun}
            disabled={loading || !activeSessionId}
            className="btn-press"
          >
            <Sparkles size={14} />
            {loading ? t("reflect.running") : t("reflect.run")}
          </Button>
        </div>
      }
      bodyClassName="p-4 space-y-6 max-w-3xl mx-auto w-full"
    >
      {error && (
        <p className="text-sm text-red-600 dark:text-red-300 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-xl px-3 py-2">
          {error}
        </p>
      )}

      {!result && !loading && (
        <EmptyState
          icon={<Sparkles size={22} strokeWidth={1.75} />}
          tone="neutral"
          title={t("reflect.emptyTitle")}
          description={
            activeSessionId
              ? t("reflect.empty")
              : t("reflect.noActiveSession")
          }
          action={
            activeSessionId ? (
              <Button
                size="sm"
                variant="secondary"
                onClick={handleRun}
                className="btn-press"
              >
                <Sparkles size={14} />
                {t("reflect.run")}
              </Button>
            ) : undefined
          }
        />
      )}

      {loading && (
        <EmptyState
          tone="neutral"
          title={t("reflect.running")}
          description={t("reflect.runningHint")}
        />
      )}

      {result && !loading && (
        <div className="fade-up-in">
          <ReflectionReview result={result} onChange={setResult} />
        </div>
      )}
    </PanelShell>
  );
}
