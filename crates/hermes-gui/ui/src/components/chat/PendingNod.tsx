/**
 * 选题单等你点头时才出现。不是「这一期」看板：没有日期、件数、接力链。
 * 唯一必停的一步，不能让用户自己去翻。
 */
import { useState } from "react";
import { Check, Pencil } from "lucide-react";
import type { DecisionView, EpisodeView } from "../../types";
import { useChatStore } from "../../store/chatStore";
import { useUiStore } from "../../store/uiStore";

function when(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

const STATUS_TONE: Record<DecisionView["status"], string> = {
  pending: "text-amber-600 dark:text-amber-400",
  approved: "text-emerald-600 dark:text-emerald-400",
  revised: "text-sky-600 dark:text-sky-400",
  settled: "text-app-fg-tertiary dark:text-slate-500",
};

function DecisionCard({
  decision,
  relPath,
}: {
  decision: DecisionView;
  relPath: string;
}) {
  const t = useUiStore((s) => s.t);
  const answerDecision = useChatStore((s) => s.answerDecision);
  const [revising, setRevising] = useState(false);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);

  const answer = async (approve: boolean) => {
    if (busy) return;
    setBusy(true);
    try {
      await answerDecision(relPath, approve, approve ? undefined : note);
      setRevising(false);
      setNote("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rounded-lg border border-amber-300/60 dark:border-amber-500/30 bg-amber-50/60 dark:bg-amber-950/20 px-2.5 py-2">
      <div className="flex items-baseline gap-2 text-app-sub">
        <span className="font-medium text-app-fg dark:text-slate-200">
          {decision.kindLabel}
        </span>
        <span className={STATUS_TONE[decision.status]}>
          {decision.statusLabel}
        </span>
        <span className="min-w-0 flex-1 truncate text-app-fg-tertiary dark:text-slate-500">
          {t("decision.byWhom", {
            who: decision.byName,
            at: when(decision.at),
          })}
        </span>
      </div>

      {decision.items.length > 0 && (
        <ol className="mt-1.5 space-y-0.5">
          {decision.items.map((item, i) => (
            <li
              key={`${i}-${item.title}`}
              className="flex items-baseline gap-1.5 text-app-sub leading-relaxed text-app-fg-secondary dark:text-slate-300"
            >
              <span className="shrink-0 text-app-fg-tertiary dark:text-slate-600">
                {i + 1}.
              </span>
              <span className="min-w-0 flex-1">{item.title}</span>
            </li>
          ))}
        </ol>
      )}

      {decision.why && (
        <p className="mt-1.5 text-app-sub leading-relaxed text-app-fg-secondary dark:text-slate-400">
          {t("decision.why", { why: decision.why })}
        </p>
      )}

      {revising ? (
        <div className="mt-2 flex items-center gap-1.5">
          <input
            autoFocus
            value={note}
            onChange={(e) => setNote(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void answer(false);
              if (e.key === "Escape") setRevising(false);
            }}
            placeholder={t("decision.notePlaceholder")}
            className="min-w-0 flex-1 rounded-md border border-app-border dark:border-slate-600 bg-app-surface dark:bg-slate-800 px-2 py-1 text-app-sub text-app-fg dark:text-slate-100 placeholder:text-app-fg-secondary focus:outline-none focus:border-app-primary/60 select-text"
          />
          <button
            type="button"
            disabled={busy}
            onClick={() => void answer(false)}
            className="shrink-0 rounded-md border border-app-border dark:border-slate-600 px-2 py-1 text-app-sub text-app-fg-secondary hover:text-app-fg transition-colors duration-[var(--motion-fast)] disabled:opacity-50"
          >
            {t("decision.sendRevise")}
          </button>
        </div>
      ) : (
        <div className="mt-2 flex items-center gap-1.5">
          <button
            type="button"
            disabled={busy}
            onClick={() => void answer(true)}
            className="inline-flex items-center gap-1 rounded-md bg-app-primary px-2 py-1 text-xs font-medium text-white hover:opacity-90 transition-opacity duration-[var(--motion-fast)] disabled:opacity-50"
          >
            <Check size={11} strokeWidth={2.5} />
            {t("decision.approve")}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => setRevising(true)}
            className="inline-flex items-center gap-1 rounded-md border border-app-border dark:border-slate-600 px-2 py-1 text-app-sub text-app-fg-secondary hover:text-app-fg transition-colors duration-[var(--motion-fast)] disabled:opacity-50"
          >
            <Pencil size={11} strokeWidth={2} />
            {t("decision.revise")}
          </button>
        </div>
      )}
    </div>
  );
}

export function PendingNod({ episode }: { episode: EpisodeView }) {
  const pending = episode.items.filter(
    (item) => item.decision?.status === "pending",
  );
  if (pending.length === 0) return null;

  return (
    <div className="shrink-0 border-b border-app-border dark:border-slate-800 bg-app-muted/40 dark:bg-slate-900/40 px-4 py-2 space-y-2">
      {pending.map((item) => (
        <DecisionCard
          key={item.relPath}
          decision={item.decision!}
          relPath={item.relPath}
        />
      ))}
    </div>
  );
}
