import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChatStore } from "../../store/chatStore";
import { Sparkles, Check, X, Merge, SkipForward, ArrowRightLeft } from "lucide-react";

interface SkillCandidateView {
  name: string;
  description: string;
  triggers: string[];
  body: string;
  rationale: string;
  confidence: string;
}

interface MemoryCandidateView {
  fact: string;
  tags: string[];
  scope: string;
  confidence: string;
  rationale: string;
  supersedes: string[];
}

interface ConflictView {
  with: string;
  kind: string;
  explain: string;
  options: string[];
}

interface ReflectionResult {
  summary: string;
  skillCandidates: SkillCandidateView[];
  memoryCandidates: MemoryCandidateView[];
  conflicts: ConflictView[];
}

type ConflictAction = "keep_new" | "keep_old" | "merge" | "scope_split" | "skip";

export function ReflectPanel() {
  const { activeSessionId } = useChatStore();
  const [result, setResult] = useState<ReflectionResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleRun = async () => {
    if (!activeSessionId) {
      setError("No active session to reflect on.");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const r = await invoke<ReflectionResult>("run_reflection", {
        sessionId: activeSessionId,
      });
      setResult(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const acceptSkill = async (c: SkillCandidateView) => {
    await invoke("accept_skill_candidate", {
      name: c.name,
      description: c.description,
      triggers: c.triggers,
      body: c.body,
    });
    setResult((prev) =>
      prev
        ? { ...prev, skillCandidates: prev.skillCandidates.filter((s) => s.name !== c.name) }
        : null
    );
  };

  const acceptMemory = async (c: MemoryCandidateView) => {
    await invoke("accept_memory_candidate", {
      fact: c.fact,
      tags: c.tags,
      scope: c.scope,
      confidence: c.confidence,
      supersedes: c.supersedes,
    });
    setResult((prev) =>
      prev
        ? { ...prev, memoryCandidates: prev.memoryCandidates.filter((m) => m.fact !== c.fact) }
        : null
    );
  };

  // Conflict ↔ candidate linkage: a memory candidate "owns" a conflict when
  // its `supersedes` contains the conflict's `with` id. Render those as a
  // single conflict card; render unlinked (orphan) conflicts as a note-only
  // card with a Dismiss action.
  const { linkedConflicts, orphanConflicts } = useMemo(() => {
    if (!result) return { linkedConflicts: [], orphanConflicts: [] };
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
    await invoke("handle_conflict", {
      fact: candidate.fact,
      tags: candidate.tags,
      scope: candidate.scope,
      confidence: candidate.confidence,
      supersedes: candidate.supersedes,
      oldId: conflict.with,
      action,
      mergedBody: mergedBody ?? null,
    });
    setResult((prev) =>
      prev
        ? {
            ...prev,
            conflicts: prev.conflicts.filter((c) => c.with !== conflict.with),
            memoryCandidates: prev.memoryCandidates.filter((m) => m.fact !== candidate.fact),
          }
        : null
    );
  };

  const dismissOrphan = (conflict: ConflictView) => {
    setResult((prev) =>
      prev ? { ...prev, conflicts: prev.conflicts.filter((c) => c.with !== conflict.with) } : null
    );
  };

  return (
    <div className="flex-1 flex flex-col h-full">
      <header className="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700">
        <h2 className="text-lg font-semibold">Reflection</h2>
        <button
          onClick={handleRun}
          disabled={loading || !activeSessionId}
          className="flex items-center gap-1 px-3 py-1.5 text-sm rounded-lg bg-purple-600 text-white hover:bg-purple-700 disabled:opacity-50 transition-colors"
        >
          <Sparkles size={14} />
          {loading ? "Reflecting..." : "Run Reflection"}
        </button>
      </header>

      <div className="flex-1 overflow-y-auto p-4 space-y-6">
        {error && <p className="text-sm text-red-500">{error}</p>}

        {!result && !loading && (
          <p className="text-sm text-gray-500 text-center mt-8">
            Run reflection on the current session to extract skills and memories.
          </p>
        )}

        {result && (
          <>
            <section>
              <p className="text-sm text-gray-600 dark:text-gray-400">{result.summary}</p>
            </section>

            {result.skillCandidates.length > 0 && (
              <section className="space-y-3">
                <h3 className="text-sm font-medium text-gray-500 uppercase tracking-wide">
                  Skill Candidates
                </h3>
                {result.skillCandidates.map((c) => (
                  <div
                    key={c.name}
                    className="p-3 rounded-lg border border-gray-200 dark:border-gray-700 space-y-2"
                  >
                    <div className="flex items-start justify-between">
                      <div>
                        <span className="font-medium text-sm font-mono">{c.name}</span>
                        <p className="text-xs text-gray-500 mt-0.5">{c.description}</p>
                      </div>
                      <div className="flex gap-1">
                        <button
                          onClick={() => acceptSkill(c)}
                          className="p-1.5 rounded hover:bg-green-100 dark:hover:bg-green-900 text-green-600"
                          title="Accept"
                        >
                          <Check size={14} />
                        </button>
                        <button
                          onClick={() =>
                            setResult((prev) =>
                              prev
                                ? { ...prev, skillCandidates: prev.skillCandidates.filter((s) => s.name !== c.name) }
                                : null
                            )
                          }
                          className="p-1.5 rounded hover:bg-red-100 dark:hover:bg-red-900 text-red-500"
                          title="Reject"
                        >
                          <X size={14} />
                        </button>
                      </div>
                    </div>
                    <p className="text-xs text-gray-400 italic">{c.rationale}</p>
                  </div>
                ))}
              </section>
            )}

            {result.memoryCandidates.filter((c) => !linkedFacts.has(c.fact)).length > 0 && (
              <section className="space-y-3">
                <h3 className="text-sm font-medium text-gray-500 uppercase tracking-wide">
                  Memory Candidates
                </h3>
                {result.memoryCandidates
                  .filter((c) => !linkedFacts.has(c.fact))
                  .map((c, i) => (
                    <div
                      key={i}
                      className="p-3 rounded-lg border border-gray-200 dark:border-gray-700 space-y-2"
                    >
                      <div className="flex items-start justify-between">
                        <p className="text-sm flex-1">{c.fact}</p>
                        <div className="flex gap-1 shrink-0">
                          <button
                            onClick={() => acceptMemory(c)}
                            className="p-1.5 rounded hover:bg-green-100 dark:hover:bg-green-900 text-green-600"
                            title="Accept"
                          >
                            <Check size={14} />
                          </button>
                          <button
                            onClick={() =>
                              setResult((prev) =>
                                prev
                                  ? { ...prev, memoryCandidates: prev.memoryCandidates.filter((m) => m.fact !== c.fact) }
                                  : null
                              )
                            }
                            className="p-1.5 rounded hover:bg-red-100 dark:hover:bg-red-900 text-red-500"
                            title="Reject"
                          >
                            <X size={14} />
                          </button>
                        </div>
                      </div>
                      <div className="flex gap-2 flex-wrap">
                        <span className="text-xs text-gray-400">{c.scope} / {c.confidence}</span>
                        {c.tags.map((t) => (
                          <span key={t} className="text-xs px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-700">
                            {t}
                          </span>
                        ))}
                      </div>
                      <p className="text-xs text-gray-400 italic">{c.rationale}</p>
                    </div>
                  ))}
              </section>
            )}

            {(linkedConflicts.length > 0 || orphanConflicts.length > 0) && (
              <section className="space-y-3">
                <h3 className="text-sm font-medium text-gray-500 uppercase tracking-wide">
                  Conflicts
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
                    className="p-3 rounded-lg border border-yellow-200 dark:border-yellow-700 bg-yellow-50 dark:bg-yellow-900/20 space-y-2"
                  >
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-medium uppercase text-yellow-700 dark:text-yellow-300">
                          {c.kind}
                        </span>
                        <span className="text-xs text-gray-400">with: {c.with}</span>
                      </div>
                      <button
                        onClick={() => dismissOrphan(c)}
                        className="text-xs text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
                      >
                        Dismiss
                      </button>
                    </div>
                    <p className="text-sm">{c.explain}</p>
                  </div>
                ))}
              </section>
            )}
          </>
        )}
      </div>
    </div>
  );
}

interface ConflictCardProps {
  conflict: ConflictView;
  candidate: MemoryCandidateView;
  onResolve: (
    conflict: ConflictView,
    candidate: MemoryCandidateView,
    action: ConflictAction,
    mergedBody?: string
  ) => Promise<void>;
}

function ConflictCard({ conflict, candidate, onResolve }: ConflictCardProps) {
  const [mergeMode, setMergeMode] = useState(false);
  const [mergeBody, setMergeBody] = useState(candidate.fact);
  const [busy, setBusy] = useState(false);

  const run = async (action: ConflictAction, body?: string) => {
    setBusy(true);
    try {
      await onResolve(conflict, candidate, action, body);
    } finally {
      setBusy(false);
    }
  };

  const oppositeScope = candidate.scope === "Project" ? "User" : "Project";

  return (
    <div className="p-3 rounded-lg border border-yellow-200 dark:border-yellow-700 bg-yellow-50 dark:bg-yellow-900/20 space-y-3">
      <div className="flex items-center gap-2">
        <span className="text-xs font-medium uppercase text-yellow-700 dark:text-yellow-300">
          {conflict.kind}
        </span>
        <span className="text-xs text-gray-400">with: {conflict.with}</span>
      </div>
      <p className="text-sm text-gray-700 dark:text-gray-200">{conflict.explain}</p>

      <div className="rounded border border-yellow-200/60 dark:border-yellow-700/60 bg-white/60 dark:bg-gray-900/40 p-2">
        <div className="text-[10px] uppercase tracking-wide text-gray-500 mb-1">New candidate</div>
        <p className="text-sm">{candidate.fact}</p>
        <div className="mt-1 text-xs text-gray-400">
          {candidate.scope} / {candidate.confidence}
        </div>
      </div>

      {mergeMode ? (
        <div className="space-y-2">
          <label className="block text-xs text-gray-500">Merged body</label>
          <textarea
            value={mergeBody}
            onChange={(e) => setMergeBody(e.target.value)}
            rows={4}
            className="w-full resize-y rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-2.5 py-1.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-blue-500"
          />
          <div className="flex justify-end gap-2">
            <button
              onClick={() => setMergeMode(false)}
              disabled={busy}
              className="text-xs px-2.5 py-1 rounded border border-gray-300 dark:border-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800"
            >
              Cancel
            </button>
            <button
              onClick={() => run("merge", mergeBody)}
              disabled={busy || !mergeBody.trim()}
              className="text-xs px-2.5 py-1 rounded bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50"
            >
              Apply merge
            </button>
          </div>
        </div>
      ) : (
        <div className="flex flex-wrap gap-1.5">
          <ActionButton
            label="Keep new"
            title="Persist the new candidate; mark old as superseded"
            icon={<Check size={12} />}
            tone="primary"
            disabled={busy}
            onClick={() => run("keep_new")}
          />
          <ActionButton
            label="Keep old"
            title="Discard the new candidate; keep the existing memory"
            icon={<X size={12} />}
            disabled={busy}
            onClick={() => run("keep_old")}
          />
          <ActionButton
            label="Merge"
            title="Edit and save a combined body that supersedes the old"
            icon={<Merge size={12} />}
            disabled={busy}
            onClick={() => {
              setMergeBody(candidate.fact);
              setMergeMode(true);
            }}
          />
          <ActionButton
            label={`Scope-split → ${oppositeScope}`}
            title={`Save the candidate in ${oppositeScope} scope; keep both`}
            icon={<ArrowRightLeft size={12} />}
            disabled={busy}
            onClick={() => run("scope_split")}
          />
          <ActionButton
            label="Skip"
            title="Defer — no write"
            icon={<SkipForward size={12} />}
            disabled={busy}
            onClick={() => run("skip")}
          />
        </div>
      )}
    </div>
  );
}

interface ActionButtonProps {
  label: string;
  title: string;
  icon: React.ReactNode;
  tone?: "primary" | "default";
  disabled?: boolean;
  onClick: () => void;
}

function ActionButton({ label, title, icon, tone, disabled, onClick }: ActionButtonProps) {
  const base =
    "inline-flex items-center gap-1 text-xs px-2 py-1 rounded border disabled:opacity-50";
  const classes =
    tone === "primary"
      ? `${base} bg-blue-600 text-white border-blue-600 hover:bg-blue-700`
      : `${base} border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800`;
  return (
    <button title={title} disabled={disabled} onClick={onClick} className={classes}>
      {icon}
      {label}
    </button>
  );
}
