import { useChatStore } from "../../store/chatStore";

export function ProposedSkillModal() {
  const { proposedSkills, acceptProposedSkill, dismissProposedSkill } = useChatStore();
  const candidate = proposedSkills[0];
  if (!candidate) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-[640px] max-h-[80vh] flex flex-col rounded-lg bg-white dark:bg-gray-900 shadow-xl border border-gray-200 dark:border-gray-700">
        <header className="px-4 py-3 border-b border-gray-200 dark:border-gray-700">
          <h2 className="text-sm font-semibold">Proposed skill</h2>
          <p className="text-xs text-gray-500 mt-0.5">
            The agent drafted a reusable skill from this session. Review before saving.
          </p>
        </header>

        <div className="px-4 py-3 overflow-y-auto flex-1 text-sm space-y-3">
          <div>
            <div className="text-xs uppercase tracking-wide text-gray-500">Name</div>
            <div className="font-mono">{candidate.name}</div>
          </div>
          <div>
            <div className="text-xs uppercase tracking-wide text-gray-500">Description</div>
            <div>{candidate.description}</div>
          </div>
          {candidate.triggers.length > 0 && (
            <div>
              <div className="text-xs uppercase tracking-wide text-gray-500">Triggers</div>
              <div className="flex flex-wrap gap-1 mt-1">
                {candidate.triggers.map((t) => (
                  <span
                    key={t}
                    className="text-xs px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800"
                  >
                    {t}
                  </span>
                ))}
              </div>
            </div>
          )}
          <div>
            <div className="text-xs uppercase tracking-wide text-gray-500 mb-1">Body</div>
            <pre className="text-xs bg-gray-50 dark:bg-gray-800 rounded p-2 whitespace-pre-wrap font-mono">
              {candidate.body}
            </pre>
          </div>
        </div>

        <footer className="px-4 py-3 border-t border-gray-200 dark:border-gray-700 flex justify-end gap-2">
          <button
            onClick={() => dismissProposedSkill(candidate.name)}
            className="px-3 py-1.5 text-sm rounded border border-gray-300 dark:border-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800"
          >
            Reject
          </button>
          <button
            onClick={() => acceptProposedSkill(candidate.name)}
            className="px-3 py-1.5 text-sm rounded bg-blue-600 text-white hover:bg-blue-700"
          >
            Save as skill
          </button>
        </footer>
      </div>
    </div>
  );
}
