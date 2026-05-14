import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChatStore } from "../../store/chatStore";
import { Sparkles, Check, X } from "lucide-react";

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
    });
    setResult((prev) =>
      prev
        ? { ...prev, memoryCandidates: prev.memoryCandidates.filter((m) => m.fact !== c.fact) }
        : null
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

            {result.memoryCandidates.length > 0 && (
              <section className="space-y-3">
                <h3 className="text-sm font-medium text-gray-500 uppercase tracking-wide">
                  Memory Candidates
                </h3>
                {result.memoryCandidates.map((c, i) => (
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
                                ? { ...prev, memoryCandidates: prev.memoryCandidates.filter((_, j) => j !== i) }
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

            {result.conflicts.length > 0 && (
              <section className="space-y-3">
                <h3 className="text-sm font-medium text-gray-500 uppercase tracking-wide">
                  Conflicts
                </h3>
                {result.conflicts.map((c, i) => (
                  <div
                    key={i}
                    className="p-3 rounded-lg border border-yellow-200 dark:border-yellow-700 bg-yellow-50 dark:bg-yellow-900/20 space-y-1"
                  >
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-medium uppercase text-yellow-700 dark:text-yellow-300">
                        {c.kind}
                      </span>
                      <span className="text-xs text-gray-400">with: {c.with}</span>
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
