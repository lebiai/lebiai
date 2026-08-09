import { Sparkles } from "lucide-react";
import { useChatStore } from "../../store/chatStore";
import { useUiStore } from "../../store/uiStore";
import { Button, ui } from "../common/ui";

export function ProposedSkillModal() {
  const { proposedSkills, acceptProposedSkill, dismissProposedSkill } = useChatStore();
  const t = useUiStore((s) => s.t);
  const candidate = proposedSkills[0];
  if (!candidate) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 backdrop-blur-[2px] p-4">
      <div
        className="w-full max-w-xl max-h-[80vh] flex flex-col rounded-2xl bg-app-surface dark:bg-slate-900 shadow-2xl border border-app-border dark:border-slate-700"
        role="dialog"
        aria-modal="true"
        aria-labelledby="proposed-skill-title"
      >
        <header className="flex items-start gap-3 px-4 py-3.5 border-b border-app-border dark:border-slate-800 shrink-0">
          <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-app-accent-soft dark:bg-violet-950/50 text-app-accent shrink-0">
            <Sparkles size={16} />
          </div>
          <div className="min-w-0">
            <h2
              id="proposed-skill-title"
              className="text-sm font-semibold text-app-fg dark:text-slate-100"
            >
              {t("propose.title")}
            </h2>
            <p className="text-xs text-app-fg-secondary dark:text-slate-400 mt-0.5 leading-relaxed">
              {t("propose.subtitle")}
            </p>
          </div>
        </header>

        <div className="px-4 py-3 overflow-y-auto flex-1 text-sm space-y-3">
          <div>
            <div className="text-xs uppercase tracking-wide text-app-fg-tertiary">
              {t("propose.name")}
            </div>
            <div className="font-mono text-app-fg dark:text-slate-100 mt-0.5">
              {candidate.name}
            </div>
          </div>
          <div>
            <div className="text-xs uppercase tracking-wide text-app-fg-tertiary">
              {t("propose.description")}
            </div>
            <div className="text-app-fg-secondary dark:text-slate-300 mt-0.5">
              {candidate.description}
            </div>
          </div>
          {candidate.triggers.length > 0 && (
            <div>
              <div className="text-xs uppercase tracking-wide text-app-fg-tertiary">
                {t("propose.triggers")}
              </div>
              <div className="flex flex-wrap gap-1 mt-1">
                {candidate.triggers.map((tr) => (
                  <span
                    key={tr}
                    className="text-xs px-1.5 py-0.5 rounded-md bg-app-muted dark:bg-slate-800 text-app-fg-secondary"
                  >
                    {tr}
                  </span>
                ))}
              </div>
            </div>
          )}
          <div>
            <div className="text-xs uppercase tracking-wide text-app-fg-tertiary mb-1">
              {t("propose.body")}
            </div>
            <pre className={`${ui.card} p-3 text-xs whitespace-pre-wrap font-mono text-app-fg-secondary dark:text-slate-300 max-h-48 overflow-y-auto`}>
              {candidate.body}
            </pre>
          </div>
        </div>

        <footer className="px-4 py-3 border-t border-app-border dark:border-slate-800 flex justify-end gap-2 bg-app-muted/40 dark:bg-slate-950/40 rounded-b-2xl shrink-0">
          <Button
            size="sm"
            variant="secondary"
            onClick={() => dismissProposedSkill(candidate.name)}
          >
            {t("propose.reject")}
          </Button>
          <Button size="sm" onClick={() => void acceptProposedSkill(candidate.name)}>
            {t("propose.accept")}
          </Button>
        </footer>
      </div>
    </div>
  );
}
