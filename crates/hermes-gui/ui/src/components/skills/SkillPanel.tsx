import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Trash2, Plus, Pencil, Save, X, Zap, Sparkles, ChevronDown } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { Button, EmptyState, ui } from "../common/ui";
import { Select } from "../common/Select";
import { ConfirmPopover } from "../common/ConfirmPopover";
import { toast } from "../../utils/toast";

interface SkillItem {
  name: string;
  description: string;
  triggers: string[];
  scope: string;
  body: string;
  builtin: boolean;
}

type Mode = "view" | "edit" | "create";

interface DraftSkill {
  name: string;
  description: string;
  triggers: string;
  body: string;
  scope: string;
}

const EMPTY_DRAFT: DraftSkill = {
  name: "",
  description: "",
  triggers: "",
  body: "",
  scope: "User",
};

function toDraft(s: SkillItem): DraftSkill {
  return {
    name: s.name,
    description: s.description,
    triggers: s.triggers.join(", "),
    body: s.body,
    scope: s.scope,
  };
}

export function SkillPanel({ embedded = false }: { embedded?: boolean }) {
  const t = useUiStore((state) => state.t);
  const [skills, setSkills] = useState<SkillItem[]>([]);
  const [builtinSkills, setBuiltinSkills] = useState<SkillItem[]>([]);
  const [userSkills, setUserSkills] = useState<SkillItem[]>([]);
  const [selected, setSelected] = useState<SkillItem | null>(null);
  const [mode, setMode] = useState<Mode>("view");
  const [draft, setDraft] = useState<DraftSkill>(EMPTY_DRAFT);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [bodyOpen, setBodyOpen] = useState(false);

  const fetchSkills = async () => {
    const items = await invoke<SkillItem[]>("list_skills");
    setSkills(items);
    setBuiltinSkills(items.filter((s) => s.builtin));
    setUserSkills(items.filter((s) => !s.builtin));
  };

  useEffect(() => {
    fetchSkills();
  }, []);

  const select = (s: SkillItem) => {
    setSelected(s);
    setMode("view");
    setError(null);
    setBodyOpen(false);
  };

  const startCreate = () => {
    setSelected(null);
    setDraft(EMPTY_DRAFT);
    setMode("create");
    setError(null);
    setBodyOpen(false);
  };

  const startEdit = () => {
    if (!selected) return;
    setDraft(toDraft(selected));
    setMode("edit");
    setError(null);
  };

  const cancel = () => {
    setMode("view");
    setError(null);
  };

  const save = async () => {
    setError(null);
    const triggers = draft.triggers
      .split(",")
      .map((x) => x.trim())
      .filter((x) => x.length > 0);
    if (!draft.name.trim()) {
      setError(t("skills.nameRequired"));
      return;
    }
    setBusy(true);
    try {
      await invoke("save_skill", {
        name: draft.name.trim(),
        description: draft.description.trim(),
        triggers,
        body: draft.body,
        scope: draft.scope,
      });
      await fetchSkills();
      const next = await invoke<SkillItem | null>("get_skill", {
        name: draft.name.trim(),
      });
      if (next) setSelected(next);
      setMode("view");
      setBodyOpen(false);
      toast.success(t("toast.skillSaved"));
    } catch (e) {
      const msg = String(e);
      setError(msg);
      toast.error(msg);
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async (name: string, scope: string) => {
    try {
      await invoke("delete_skill", { name, scope });
      if (selected?.name === name) {
        setSelected(null);
        setMode("view");
      }
      setConfirmDelete(null);
      await fetchSkills();
      toast.success(t("toast.skillDeleted"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className={`flex-1 flex h-full ${ui.page}`}>
      <div className="w-64 border-r border-app-border dark:border-slate-800 flex flex-col bg-app-sidebar dark:bg-slate-900/50 select-none">
        <header className={`${ui.header} !px-3`}>
          <h2 className="text-base font-semibold text-app-fg dark:text-slate-100">
            {embedded ? t("skills.mine") : t("skills.title")}
          </h2>
          <button
            type="button"
            onClick={startCreate}
            className="p-1.5 rounded-lg hover:bg-app-muted dark:hover:bg-slate-800 text-app-fg-secondary transition-colors duration-[var(--motion-fast)]"
            title={t("skills.newTitle")}
          >
            <Plus size={16} />
          </button>
        </header>
        <div className="flex-1 overflow-y-auto p-2 space-y-3">
          {builtinSkills.length > 0 && (
            <div className="space-y-0.5">
              <p className="px-2 pt-0.5 flex items-center gap-1 text-[10px] font-medium uppercase tracking-wider text-app-fg-tertiary">
                <Sparkles size={11} />
                {t("skills.builtin")}
              </p>
              {builtinSkills.map((skill) => (
                <div
                  key={skill.name}
                  className={`flex items-center gap-2 px-3 py-2 rounded-lg cursor-pointer text-sm ${
                    selected?.name === skill.name && mode === "view"
                      ? ui.sessionActive
                      : ui.sessionIdle
                  }`}
                  onClick={() => select(skill)}
                >
                  <span className="truncate flex-1">{skill.name}</span>
                  <span className="shrink-0 text-[10px] px-1.5 py-0.5 rounded-md bg-app-muted dark:bg-slate-800 text-app-fg-tertiary">
                    {t("skills.builtin")}
                  </span>
                </div>
              ))}
            </div>
          )}

          <div className="space-y-0.5">
            <p className="px-2 pt-0.5 text-[10px] font-medium uppercase tracking-wider text-app-fg-tertiary">
              {t("skills.mine")}
            </p>
            {userSkills.length === 0 ? (
              <p className="px-2 py-1.5 text-xs text-app-fg-tertiary leading-relaxed">
                {t("skills.mineEmpty")}
              </p>
            ) : (
              userSkills.map((skill) => {
                const key = `${skill.scope}/${skill.name}`;
                return (
                  <div
                    key={key}
                    className={`group relative flex items-center gap-2 px-3 py-2 rounded-lg cursor-pointer text-sm ${
                      selected?.name === skill.name && mode === "view"
                        ? ui.sessionActive
                        : ui.sessionIdle
                    }`}
                    onClick={() => select(skill)}
                  >
                    <span className="truncate flex-1">{skill.name}</span>
                    <span className="text-[10px] uppercase text-app-fg-tertiary">
                      {displayScope(skill.scope, t).slice(0, 1)}
                    </span>
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        setConfirmDelete(key);
                      }}
                      className="p-1 text-app-fg-tertiary hover:text-app-danger"
                      title={t("skills.delete")}
                    >
                      <Trash2 size={12} />
                    </button>
                    <ConfirmPopover
                      open={confirmDelete === key}
                      message={t("skills.deleteConfirm")}
                      onCancel={() => setConfirmDelete(null)}
                      onConfirm={() => void handleDelete(skill.name, skill.scope)}
                    />
                  </div>
                );
              })
            )}
          </div>

          {skills.length === 0 && (
            <EmptyState
              icon={<Zap size={22} strokeWidth={1.75} />}
              title={t("skills.emptyTitle")}
              description={t("skills.empty")}
              action={
                <Button size="sm" onClick={startCreate}>
                  <Plus size={14} />
                  {t("skills.newTitle")}
                </Button>
              }
            />
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-5">
        {mode === "view" && selected && (
          <SkillIntroCard
            skill={selected}
            bodyOpen={bodyOpen}
            onToggleBody={() => setBodyOpen((v) => !v)}
            onEdit={startEdit}
          />
        )}

        {mode === "view" && !selected && (
          <p className="text-sm text-app-fg-secondary text-center mt-12">
            {t("skills.selectHint")}
          </p>
        )}

        {(mode === "edit" || mode === "create") && (
          <SkillEditor
            draft={draft}
            onChange={setDraft}
            mode={mode}
            busy={busy}
            error={error}
            onSave={save}
            onCancel={cancel}
          />
        )}
      </div>
    </div>
  );
}

/** User-facing intro card — not a raw SKILL.md dump. */
function SkillIntroCard({
  skill,
  bodyOpen,
  onToggleBody,
  onEdit,
}: {
  skill: SkillItem;
  bodyOpen: boolean;
  onToggleBody: () => void;
  onEdit: () => void;
}) {
  const t = useUiStore((s) => s.t);
  const desc = skill.description.trim();
  const hasBody = skill.body.trim().length > 0;

  return (
    <div className="space-y-5 max-w-2xl">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 space-y-1.5">
          <div className="flex items-center gap-2 flex-wrap">
            <h3 className="text-xl font-semibold tracking-tight text-app-fg dark:text-slate-100">
              {skill.name}
            </h3>
            {skill.builtin && (
              <span className="shrink-0 text-[10px] px-1.5 py-0.5 rounded-md bg-app-muted dark:bg-slate-800 text-app-fg-tertiary">
                {t("skills.builtin")}
              </span>
            )}
          </div>
          <div className="flex items-center gap-2 flex-wrap text-xs text-app-fg-tertiary">
            {skill.builtin ? (
              <span>{t("skills.builtinHint")}</span>
            ) : (
              <span>
                {t("skills.scope")}: {displayScope(skill.scope, t)}
              </span>
            )}
          </div>
        </div>
        {!skill.builtin && (
          <Button size="sm" variant="secondary" onClick={onEdit}>
            <Pencil size={12} />
            {t("skills.edit")}
          </Button>
        )}
      </div>

      <IntroSection title={t("skills.whatItDoes")}>
        {desc ? (
          <p className="text-sm leading-relaxed text-app-fg dark:text-slate-100 whitespace-pre-wrap">
            {desc}
          </p>
        ) : (
          <p className="text-sm text-app-fg-tertiary italic">{t("skills.noDescription")}</p>
        )}
      </IntroSection>

      <IntroSection title={t("skills.whenUseful")}>
        {skill.triggers.length > 0 ? (
          <div className="flex flex-wrap gap-1.5">
            {skill.triggers.map((tr) => (
              <span
                key={tr}
                className="text-xs px-2 py-0.5 rounded-md bg-app-primary-soft dark:bg-blue-950/40 text-app-primary dark:text-blue-300"
              >
                {tr}
              </span>
            ))}
          </div>
        ) : (
          <p className="text-sm text-app-fg-tertiary">{t("skills.noTriggers")}</p>
        )}
      </IntroSection>

      <IntroSection title={t("skills.howToUse")}>
        <p className="text-sm leading-relaxed text-app-fg-secondary dark:text-slate-300">
          {skill.builtin ? t("skills.howToUseBuiltin") : t("skills.howToUseCustom")}
        </p>
      </IntroSection>

      <IntroSection title={t("skills.effect")}>
        <p className="text-sm leading-relaxed text-app-fg-secondary dark:text-slate-300">
          {skill.builtin ? t("skills.effectBuiltin") : t("skills.effectCustom")}
        </p>
      </IntroSection>

      {hasBody && (
        <div className={`${ui.cardMuted} overflow-hidden`}>
          <button
            type="button"
            onClick={onToggleBody}
            className="w-full flex items-center justify-between gap-2 px-3.5 py-2.5 text-left text-sm font-medium text-app-fg-secondary hover:bg-app-muted/50 dark:hover:bg-slate-800/40 transition-colors"
            aria-expanded={bodyOpen}
          >
            <span>{t("skills.advancedBody")}</span>
            <span className="flex items-center gap-1.5 text-xs font-normal text-app-fg-tertiary">
              {bodyOpen ? t("skills.advancedBodyHide") : t("skills.advancedBodyShow")}
              <ChevronDown
                size={14}
                className={`transition-transform ${bodyOpen ? "rotate-180" : ""}`}
              />
            </span>
          </button>
          {bodyOpen && (
            <div className="border-t border-app-border dark:border-slate-700/80 px-3.5 py-3 space-y-2">
              <p className="text-[11px] text-app-fg-tertiary leading-snug">
                {t("skills.bodyHint")}
              </p>
              <pre className="text-xs whitespace-pre-wrap font-mono overflow-y-auto max-h-[45vh] text-app-fg-secondary dark:text-slate-400">
                {skill.body}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function IntroSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="space-y-1.5">
      <h4 className="text-[11px] font-semibold uppercase tracking-wider text-app-fg-tertiary">
        {title}
      </h4>
      {children}
    </section>
  );
}

function SkillEditor({
  draft,
  onChange,
  mode,
  busy,
  error,
  onSave,
  onCancel,
}: {
  draft: DraftSkill;
  onChange: (d: DraftSkill) => void;
  mode: Mode;
  busy: boolean;
  error: string | null;
  onSave: () => void;
  onCancel: () => void;
}) {
  const t = useUiStore((state) => state.t);
  const set = <K extends keyof DraftSkill>(k: K, v: DraftSkill[K]) =>
    onChange({ ...draft, [k]: v });

  return (
    <div className="space-y-4 max-w-2xl">
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold text-app-fg dark:text-slate-100">
          {mode === "create" ? t("skills.editorNew") : t("skills.editorEdit")}
        </h3>
        <div className="flex gap-2">
          <Button size="sm" variant="secondary" onClick={onCancel} disabled={busy}>
            <X size={12} />
            {t("skills.cancel")}
          </Button>
          <Button size="sm" onClick={onSave} disabled={busy}>
            <Save size={12} />
            {busy ? t("skills.saving") : t("skills.save")}
          </Button>
        </div>
      </div>

      {error && (
        <p className="text-sm text-red-600 dark:text-red-300 bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 rounded-xl px-3 py-2">
          {error}
        </p>
      )}

      <div className="space-y-1">
        <label className="block text-xs uppercase tracking-wide text-app-fg-secondary">
          {t("skills.name")}
        </label>
        <input
          type="text"
          value={draft.name}
          onChange={(e) => set("name", e.target.value)}
          disabled={mode === "edit"}
          placeholder={t("skills.namePlaceholder")}
          className={`${ui.input} font-mono disabled:opacity-60`}
        />
        <p className="text-[11px] text-app-fg-tertiary">{t("skills.nameHint")}</p>
      </div>

      <div className="space-y-1">
        <label className="block text-xs uppercase tracking-wide text-app-fg-secondary">
          {t("skills.description")}
        </label>
        <textarea
          value={draft.description}
          onChange={(e) => set("description", e.target.value)}
          placeholder={t("skills.descriptionPlaceholder")}
          rows={3}
          className={`${ui.input} resize-y`}
        />
      </div>

      <div className="space-y-1">
        <label className="block text-xs uppercase tracking-wide text-app-fg-secondary">
          {t("skills.triggers")}
        </label>
        <input
          type="text"
          value={draft.triggers}
          onChange={(e) => set("triggers", e.target.value)}
          placeholder={t("skills.triggersPlaceholder")}
          className={ui.input}
        />
      </div>

      <div className="space-y-1">
        <label className="block text-xs uppercase tracking-wide text-app-fg-secondary">
          {t("skills.scope")}
        </label>
        <Select
          value={draft.scope}
          onChange={(v) => set("scope", v)}
          disabled={mode === "edit"}
          options={[
            { value: "User", label: t("skills.userScopeOption") },
            { value: "Project", label: t("skills.projectScopeOption") },
          ]}
        />
        <p className="text-[11px] text-app-fg-tertiary leading-snug">{t("skills.scopeHint")}</p>
        <p className="text-[11px] text-app-fg-tertiary">
          {draft.scope === "Project" ? t("scope.projectHint") : t("scope.userHint")}
        </p>
      </div>

      <div className="space-y-1">
        <label className="block text-xs uppercase tracking-wide text-app-fg-secondary">
          {t("skills.body")}
        </label>
        <p className="text-[11px] text-app-fg-tertiary leading-snug">{t("skills.bodyHint")}</p>
        <textarea
          value={draft.body}
          onChange={(e) => set("body", e.target.value)}
          rows={12}
          className={`${ui.input} resize-y font-mono text-xs`}
        />
      </div>
    </div>
  );
}

function displayScope(scope: string, t: ReturnType<typeof useUiStore.getState>["t"]) {
  return scope === "Project" ? t("scope.project") : t("scope.user");
}
