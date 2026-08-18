import { useUiStore } from "../../store/uiStore";
import { getUserManual } from "../../content/userManual";
import { ui } from "../common/ui";

export function UserManual() {
  const language = useUiStore((s) => s.language);
  const doc = getUserManual(language);

  return (
    <div className="space-y-5 fade-up-in">
      <section className={`${ui.card} p-5 space-y-2`}>
        <p className="text-[11px] font-medium tracking-wide text-app-fg-tertiary uppercase">
          {doc.kicker}
        </p>
        <h3 className="text-lg font-semibold text-app-fg dark:text-slate-100">
          {doc.title}
        </h3>
        <p className="text-sm leading-relaxed text-app-fg-secondary">{doc.intro}</p>
      </section>

      <nav className={`${ui.cardMuted} px-4 py-3 flex flex-wrap gap-x-3 gap-y-1.5`}>
        {doc.sections.map((sec) => (
          <a
            key={sec.id}
            href={`#manual-${sec.id}`}
            className="text-xs text-app-primary hover:underline underline-offset-2"
          >
            {sec.title}
          </a>
        ))}
      </nav>

      {doc.sections.map((sec) => (
        <section
          key={sec.id}
          id={`manual-${sec.id}`}
          className={`${ui.card} p-5 space-y-3 scroll-mt-4`}
        >
          <h4 className="text-sm font-semibold text-app-fg dark:text-slate-100">
            {sec.title}
          </h4>
          {sec.lead && (
            <p className="text-sm leading-relaxed text-app-fg-secondary">{sec.lead}</p>
          )}
          {sec.steps && (
            <ol className="space-y-2 pl-0">
              {sec.steps.map((step, i) => (
                <li key={i} className="flex gap-3 text-sm leading-relaxed text-app-fg">
                  <span className="shrink-0 mt-0.5 h-5 w-5 rounded-full bg-app-primary-soft dark:bg-blue-950/50 text-app-primary dark:text-blue-300 text-[11px] font-semibold flex items-center justify-center">
                    {i + 1}
                  </span>
                  <span className="min-w-0">{step}</span>
                </li>
              ))}
            </ol>
          )}
          {sec.paragraphs?.map((p, i) => (
            <p key={i} className="text-sm leading-relaxed text-app-fg-secondary">
              {p}
            </p>
          ))}
          {sec.scenes && (
            <ul className="space-y-3">
              {sec.scenes.map((scene) => (
                <li
                  key={scene.title}
                  className="rounded-xl border border-app-border dark:border-slate-700/80 bg-app-muted/40 dark:bg-slate-800/40 p-3.5 space-y-1.5"
                >
                  <p className="text-sm font-medium text-app-fg">{scene.title}</p>
                  <p className="text-xs leading-relaxed text-app-fg-secondary">
                    {scene.situation}
                  </p>
                  <p className="text-sm leading-relaxed text-app-fg">
                    <span className="text-app-fg-tertiary">
                      {language === "en-US" ? "You say · " : "你可以这样说 · "}
                    </span>
                    {scene.say}
                  </p>
                  <p className="text-xs leading-relaxed text-app-fg-secondary">
                    {scene.then}
                  </p>
                </li>
              ))}
            </ul>
          )}
          {sec.tips && (
            <ul className="space-y-1.5">
              {sec.tips.map((tip) => (
                <li
                  key={tip}
                  className="text-sm leading-relaxed text-app-fg-secondary pl-3 border-l-2 border-app-border dark:border-slate-700"
                >
                  {tip}
                </li>
              ))}
            </ul>
          )}
        </section>
      ))}
    </div>
  );
}
