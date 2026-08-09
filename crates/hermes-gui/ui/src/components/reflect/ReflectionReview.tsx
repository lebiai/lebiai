import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, X, Merge, SkipForward, ArrowRightLeft } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import type {
  ConflictView,
  MemoryCandidateView,
  ReflectionResult,
  SkillCandidateView,
} from "../../types";
import { Button, ui } from "../common/ui";
import { toast } from "../../utils/toast";
import { playSeal } from "../../utils/ritual";

type ConflictAction = "keep_new" | "keep_old" | "merge" | "scope_split" | "skip";

interface ReflectionReviewProps {
  result: ReflectionResult;
  onChange: (next: ReflectionResult | null) => void;
}

/** Shared accept/reject/conflict UI for manual Reflect and session-end review. */
export function ReflectionReview({ result, onChange }: ReflectionReviewProps) {
  const t = useUiStore((state) => state.t);

  const acceptSkill = async (c: SkillCandidateView) => {
    try {
      await invoke("accept_skill_candidate", {
        name: c.name,
        description: c.description,
        triggers: c.triggers,
        body: c.body,
      });
      onChange({
        ...result,
        skillCandidates: result.skillCandidates.filter((s) => s.name !== c.name),
      });
      playSeal(t("ritual.sealSkill"));
      toast.success(t("toast.skillAccepted"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const rejectSkill = (c: SkillCandidateView) => {
    onChange({
      ...result,
      skillCandidates: result.skillCandidates.filter((s) => s.name !== c.name),
    });
    toast.info(t("toast.skillRejected"));
  };

  const acceptMemory = async (c: MemoryCandidateView) => {
    try {
      await invoke("accept_memory_candidate", {
        fact: c.fact,
        tags: c.tags,
        scope: c.scope,
        confidence: c.confidence,
        supersedes: c.supersedes,
        zone: c.zone ?? null,
      });
      onChange({
        ...result,
        memoryCandidates: result.memoryCandidates.filter((m) => m.fact !== c.fact),
      });
      playSeal(t("ritual.sealMemory"));
      toast.success(t("toast.memoryAccepted"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const rejectMemory = (c: MemoryCandidateView) => {
    onChange({
      ...result,
      memoryCandidates: result.memoryCandidates.filter((m) => m.fact !== c.fact),
    });
    toast.info(t("toast.memoryRejected"));
  };

  const { linkedConflicts, orphanConflicts } = useMemo(() => {
    const linked: { conflict: ConflictView; candidate: MemoryCandidateView }[] = [];
    const orphan: ConflictView[] = [];
    for (const c of result.conflicts) {
      const match = result.memoryCandidates.find((m) => m.supersedes.includes(c.with));
      if (match) linked.push({ conflict: c, candidate: match });
      else orphan.push(c);
    }
    return { linkedConflicts: linked, orphanConflicts: orphan };
  }, [result]);

  const linkedFacts = useMemo(
    () => new Set(linkedConflicts.map(({ candidate }) => candidate.fact)),
    [linkedConflicts]
  );

  const resolveConflict = async (
    conflict: ConflictView,
    candidate: MemoryCandidateView,
    action: ConflictAction,
    mergedBody?: string
  ) => {
    try {
      await invoke("handle_conflict", {
        fact: candidate.fact,
        tags: candidate.tags,
        scope: candidate.scope,
        confidence: candidate.confidence,
        supersedes: candidate.supersedes,
        oldId: conflict.with,
        action,
        mergedBody: mergedBody ?? null,
        zone: candidate.zone ?? null,
      });
      onChange({
        ...result,
        conflicts: result.conflicts.filter((c) => c.with !== conflict.with),
        memoryCandidates: result.memoryCandidates.filter((m) => m.fact !== candidate.fact),
      });
      if (action === "keep_new" || action === "merge" || action === "scope_split") {
        playSeal(t("ritual.sealMemory"));
      }
      toast.success(t("toast.conflictResolved"));
    } catch (e) {
      toast.error(String(e));
      throw e;
    }
  };

  const dismissOrphan = (conflict: ConflictView) => {
    onChange({
      ...result,
      conflicts: result.conflicts.filter((c) => c.with !== conflict.with),
    });
  };

  const freeMemories = result.memoryCandidates.filter((c) => !linkedFacts.has(c.fact));

  return (
    <div className="space-y-6">
      {result.summary && (
        <section className={`${ui.cardMuted} p-3.5`}>
          <p className="text-sm text-app-fg-secondary dark:text-slate-300 leading-relaxed">
            {result.summary}
          </p>
        </section>
      )}

      {result.skillCandidates.length > 0 && (
        <section className="space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-app-fg-secondary">
            {t("reflect.skillCandidates")}
          </h3>
          {result.skillCandidates.map((c) => (
            <div key={c.name} className={`${ui.card} p-3.5 space-y-2`}>
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <span className="font-medium text-sm font-mono text-app-fg dark:text-slate-100">
                    {c.name}
                  </span>
                  <p className="text-xs text-app-fg-secondary mt-0.5">{c.description}</p>
                </div>
                <div className="flex gap-1 shrink-0">
                  <button
                    type="button"
                    onClick={() => void acceptSkill(c)}
                    className="p-1.5 rounded-lg hover:bg-emerald-100 dark:hover:bg-emerald-900/40 text-app-success"
                    title={t("reflect.accept")}
                  >
                    <Check size={14} />
                  </button>
                  <button
                    type="button"
                    onClick={() => rejectSkill(c)}
                    className="p-1.5 rounded-lg hover:bg-red-100 dark:hover:bg-red-900/40 text-app-danger"
                    title={t("reflect.reject")}
                  >
                    <X size={14} />
                  </button>
                </div>
              </div>
              {c.rationale && (
                <p className="text-xs text-app-fg-tertiary italic leading-relaxed">{c.rationale}</p>
              )}
            </div>
          ))}
        </section>
      )}

      {freeMemories.length > 0 && (
        <section className="space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-app-fg-secondary">
            {t("reflect.memoryCandidates")}
          </h3>
          {freeMemories.map((c) => (
            <div key={c.fact} className={`${ui.card} p-3.5 space-y-2`}>
              <div className="flex items-start justify-between gap-3">
                <div className="flex-1 min-w-0 space-y-1.5">
                  {isWorkEpisodeCandidate(c) && (
                    <span className="inline-flex text-[10px] font-medium tracking-wide uppercase px-1.5 py-0.5 rounded bg-app-primary/10 text-app-primary dark:bg-sky-500/15 dark:text-sky-300">
                      {t("reflect.workEpisodeBadge")}
                    </span>
                  )}
                  <p className="text-sm text-app-fg dark:text-slate-100 whitespace-pre-wrap">
                    {c.fact}
                  </p>
                </div>
                <div className="flex gap-1 shrink-0">
                  <button
                    type="button"
                    onClick={() => void acceptMemory(c)}
                    className="p-1.5 rounded-lg hover:bg-emerald-100 dark:hover:bg-emerald-900/40 text-app-success"
                    title={t("reflect.accept")}
                  >
                    <Check size={14} />
                  </button>
                  <button
                    type="button"
                    onClick={() => rejectMemory(c)}
                    className="p-1.5 rounded-lg hover:bg-red-100 dark:hover:bg-red-900/40 text-app-danger"
                    title={t("reflect.reject")}
                  >
                    <X size={14} />
                  </button>
                </div>
              </div>
              <div className="flex gap-2 flex-wrap">
                <span className="text-xs text-app-fg-tertiary">
                  {displayScope(c.scope, t)} / {displayConfidence(c.confidence, t)}
                  {c.zone ? ` / ${c.zone}` : ""}
                </span>
                {c.tags.map((tag) => (
                  <span
                    key={tag}
                    className="text-xs px-1.5 py-0.5 rounded-md bg-app-muted dark:bg-slate-800 text-app-fg-secondary"
                  >
                    {tag}
                  </span>
                ))}
              </div>
              {c.rationale && (
                <p className="text-xs text-app-fg-tertiary italic">{c.rationale}</p>
              )}
            </div>
          ))}
        </section>
      )}

      {(linkedConflicts.length > 0 || orphanConflicts.length > 0) && (
        <section className="space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-app-fg-secondary">
            {t("reflect.conflicts")}
          </h3>
          {linkedConflicts.map(({ conflict, candidate }) => (
            <ConflictCard
              key={conflict.with}
              conflict={conflict}
              candidate={candidate}
              onResolve={resolveConflict}
            />
          ))}
          {orphanConflicts.map((c) => (
            <div
              key={c.with}
              className="p-3.5 rounded-xl border border-amber-200 dark:border-amber-800/70 bg-amber-50 dark:bg-amber-950/25 space-y-2"
            >
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2 min-w-0">
                  <span className="text-xs font-medium uppercase text-amber-800 dark:text-amber-300">
                    {c.kind}
                  </span>
                  <span className="text-xs text-app-fg-tertiary truncate">
                    {t("reflect.with", { id: c.with })}
                  </span>
                </div>
                <button
                  type="button"
                  onClick={() => dismissOrphan(c)}
                  className="text-xs text-app-fg-secondary hover:text-app-fg shrink-0"
                >
                  {t("reflect.dismiss")}
                </button>
              </div>
              <p className="text-sm text-app-fg dark:text-slate-100">{c.explain}</p>
            </div>
          ))}
        </section>
      )}
    </div>
  );
}

function ConflictCard({
  conflict,
  candidate,
  onResolve,
}: {
  conflict: ConflictView;
  candidate: MemoryCandidateView;
  onResolve: (
    conflict: ConflictView,
    candidate: MemoryCandidateView,
    action: ConflictAction,
    mergedBody?: string
  ) => Promise<void>;
}) {
  const t = useUiStore((state) => state.t);
  const [mergeMode, setMergeMode] = useState(false);
  const [mergeBody, setMergeBody] = useState(candidate.fact);
  const [busy, setBusy] = useState(false);

  const run = async (action: ConflictAction, body?: string) => {
    setBusy(true);
    try {
      await onResolve(conflict, candidate, action, body);
    } catch {
      // toast already shown
    } finally {
      setBusy(false);
    }
  };

  const oppositeScope = candidate.scope === "Project" ? "User" : "Project";
  const oppositeScopeLabel = displayScope(oppositeScope, t);

  return (
    <div className="p-3.5 rounded-xl border border-amber-200 dark:border-amber-800/70 bg-amber-50 dark:bg-amber-950/25 space-y-3">
      <div className="flex items-center gap-2">
        <span className="text-xs font-medium uppercase text-amber-800 dark:text-amber-300">
          {conflict.kind}
        </span>
        <span className="text-xs text-app-fg-tertiary">
          {t("reflect.with", { id: conflict.with })}
        </span>
      </div>
      <p className="text-sm text-app-fg dark:text-slate-100">{conflict.explain}</p>

      <div className="rounded-xl border border-amber-200/70 dark:border-amber-800/50 bg-app-surface/80 dark:bg-slate-900/50 p-2.5">
        <div className="text-[10px] uppercase tracking-wide text-app-fg-tertiary mb-1">
          {t("reflect.newCandidate")}
        </div>
        <p className="text-sm text-app-fg dark:text-slate-100">{candidate.fact}</p>
        <div className="mt-1 text-xs text-app-fg-tertiary">
          {displayScope(candidate.scope, t)} / {displayConfidence(candidate.confidence, t)}
        </div>
      </div>

      {mergeMode ? (
        <div className="space-y-2">
          <label className="block text-xs text-app-fg-secondary">{t("reflect.mergedBody")}</label>
          <textarea
            value={mergeBody}
            onChange={(e) => setMergeBody(e.target.value)}
            rows={4}
            className={`${ui.input} resize-y font-mono`}
          />
          <div className="flex justify-end gap-2">
            <Button size="sm" variant="secondary" onClick={() => setMergeMode(false)} disabled={busy}>
              {t("reflect.cancel")}
            </Button>
            <Button
              size="sm"
              onClick={() => void run("merge", mergeBody)}
              disabled={busy || !mergeBody.trim()}
            >
              {t("reflect.applyMerge")}
            </Button>
          </div>
        </div>
      ) : (
        <div className="flex flex-wrap gap-1.5">
          <ActionButton
            label={t("reflect.keepNew")}
            title={t("reflect.keepNewTitle")}
            icon={<Check size={12} />}
            tone="primary"
            disabled={busy}
            onClick={() => void run("keep_new")}
          />
          <ActionButton
            label={t("reflect.keepOld")}
            title={t("reflect.keepOldTitle")}
            icon={<X size={12} />}
            disabled={busy}
            onClick={() => void run("keep_old")}
          />
          <ActionButton
            label={t("reflect.merge")}
            title={t("reflect.mergeTitle")}
            icon={<Merge size={12} />}
            disabled={busy}
            onClick={() => {
              setMergeBody(candidate.fact);
              setMergeMode(true);
            }}
          />
          <ActionButton
            label={t("reflect.scopeSplit", { scope: oppositeScopeLabel })}
            title={t("reflect.scopeSplitTitle", { scope: oppositeScopeLabel })}
            icon={<ArrowRightLeft size={12} />}
            disabled={busy}
            onClick={() => void run("scope_split")}
          />
          <ActionButton
            label={t("reflect.skip")}
            title={t("reflect.skipTitle")}
            icon={<SkipForward size={12} />}
            disabled={busy}
            onClick={() => void run("skip")}
          />
        </div>
      )}
    </div>
  );
}

function isWorkEpisodeCandidate(c: MemoryCandidateView): boolean {
  if (c.zone === "work" || c.zone === "episode") return true;
  if (c.fact.includes("【工作情节】")) return true;
  return (c.tags ?? []).some((t) => {
    const x = t.toLowerCase();
    return x === "work-episode" || x === "episode" || x.includes("work-episode");
  });
}

function displayScope(scope: string, t: ReturnType<typeof useUiStore.getState>["t"]) {
  return scope === "Project" ? t("scope.project") : t("scope.user");
}

function displayConfidence(
  confidence: string,
  t: ReturnType<typeof useUiStore.getState>["t"]
) {
  const normalized = confidence.toLowerCase();
  if (normalized === "high") return t("confidence.high");
  if (normalized === "medium") return t("confidence.medium");
  if (normalized === "low") return t("confidence.low");
  return confidence;
}

function ActionButton({
  label,
  title,
  icon,
  tone,
  disabled,
  onClick,
}: {
  label: string;
  title: string;
  icon: React.ReactNode;
  tone?: "primary" | "default";
  disabled?: boolean;
  onClick: () => void;
}) {
  if (tone === "primary") {
    return (
      <Button size="sm" onClick={onClick} disabled={disabled} title={title}>
        {icon}
        {label}
      </Button>
    );
  }
  return (
    <Button size="sm" variant="secondary" onClick={onClick} disabled={disabled} title={title}>
      {icon}
      {label}
    </Button>
  );
}
