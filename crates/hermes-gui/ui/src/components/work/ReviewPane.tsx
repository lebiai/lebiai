import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useUiStore } from "../../store/uiStore";
import { useWorkDrawerStore } from "../../store/workDrawerStore";
import { toast } from "../../utils/toast";
import { Button, Chip, ui } from "../common/ui";

export type ReviewEntry = {
  markdown: string;
  focus: string;
  done: string[];
  outputs: string[];
  stillOwe: string[];
  path: string;
  from: string;
  to: string;
  createdAt: string;
  empty: boolean;
};

type Body = {
  focus: string;
  done: string[];
  outputs: string[];
  stillOwe: string[];
};

function parseMarkdownBody(md: string): Body {
  const out: Body = { focus: "", done: [], outputs: [], stillOwe: [] };
  let section = "";
  for (const raw of md.split("\n")) {
    const t = raw.trim();
    if (!t || t.startsWith("# ")) continue;
    if (t === "## 做成了什么" || t === "## What landed") {
      section = "done";
      continue;
    }
    if (t === "## 产出" || t === "## Files written") {
      section = "outputs";
      continue;
    }
    if (t === "## 还欠" || t === "## Still open") {
      section = "owe";
      continue;
    }
    if (t.startsWith("## ")) {
      section = "other";
      continue;
    }
    if (t.startsWith("- ")) {
      const rest = t.slice(2).replace(/^`|`$/g, "");
      if (section === "done") out.done.push(rest);
      else if (section === "outputs") out.outputs.push(rest);
      else if (section === "owe") out.stillOwe.push(rest);
      continue;
    }
    if (!section) {
      out.focus = out.focus ? `${out.focus} ${t}` : t;
    }
  }
  return out;
}

function displayBody(row: ReviewEntry): Body {
  const structured: Body = {
    focus: row.focus.trim(),
    done: row.done,
    outputs: row.outputs,
    stillOwe: row.stillOwe,
  };
  if (structured.focus || structured.done.length || structured.outputs.length || structured.stillOwe.length) {
    return structured;
  }
  return parseMarkdownBody(row.markdown ?? "");
}

function bodyEmpty(b: Body): boolean {
  return !b.focus && b.done.length === 0 && b.outputs.length === 0 && b.stillOwe.length === 0;
}

const CHIPS: {
  w: number;
  label: "review.chipFri" | "review.chipSat" | "review.chipMon" | "review.chipOff";
}[] = [
  { w: 5, label: "review.chipFri" },
  { w: 6, label: "review.chipSat" },
  { w: 1, label: "review.chipMon" },
  { w: 0, label: "review.chipOff" },
];

function shortDay(iso: string): string {
  const p = iso.split("-");
  if (p.length < 3) return iso;
  const m = Number(p[1]);
  const d = Number(p[2]);
  if (!m || !d) return iso;
  return `${m}/${d}`;
}

function createdDay(iso: string): string {
  return shortDay(iso.slice(0, 10));
}

function rangeLabel(
  from: string,
  to: string,
  span: string,
  t: (k: "review.rangeWeek" | "review.range7" | "review.rangeCustom", p: { range: string }) => string
): string {
  const range = `${shortDay(from)}–${shortDay(to)}`;
  if (span === "last_7_days") return t("review.range7", { range });
  if (span === "custom") return t("review.rangeCustom", { range });
  return t("review.rangeWeek", { range });
}

export function ReviewPane() {
  const t = useUiStore((s) => s.t);
  const { prefs, refreshPrefs, setTab } = useWorkDrawerStore();
  const [from, setFrom] = useState(prefs?.from ?? "");
  const [to, setTo] = useState(prefs?.to ?? "");
  const [busy, setBusy] = useState(false);
  const [custom, setCustom] = useState(false);
  const [ledger, setLedger] = useState<ReviewEntry[]>([]);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [emptyRun, setEmptyRun] = useState(false);
  const [setupOpen, setSetupOpen] = useState(false);

  useEffect(() => {
    void refreshPrefs();
  }, [refreshPrefs]);

  useEffect(() => {
    void invoke<ReviewEntry[]>("list_reviews")
      .then(setLedger)
      .catch(() => setLedger([]));
  }, []);

  useEffect(() => {
    if (prefs && !custom) {
      setFrom(prefs.from);
      setTo(prefs.to);
    }
  }, [prefs?.from, prefs?.to, custom]);

  const selected = selectedPath
    ? ledger.find((r) => r.path === selectedPath) ?? null
    : null;
  const reading = !!selected;
  const weekday = prefs?.weekday ?? 0;
  const span = custom ? "custom" : (prefs?.defaultSpan ?? "this_week");

  const setWeekday = async (w: number) => {
    try {
      await invoke("set_review_prefs", {
        weekday: w,
        defaultSpan: prefs?.defaultSpan ?? "this_week",
      });
      await refreshPrefs();
    } catch {
      toast.error(t("review.savePrefError"));
    }
  };

  const setSpan = async (next: string) => {
    setCustom(false);
    setEmptyRun(false);
    try {
      await invoke("set_review_prefs", {
        weekday: prefs?.weekday ?? 0,
        defaultSpan: next,
      });
      await refreshPrefs();
    } catch {
      toast.error(t("review.savePrefError"));
    }
  };

  const run = async (a: string, b: string) => {
    setBusy(true);
    setEmptyRun(false);
    try {
      const out = await invoke<ReviewEntry>("run_period_review", { from: a, to: b });
      if (out.empty) {
        setEmptyRun(true);
        setLedger((prev) => prev.filter((r) => !(r.from === a && r.to === b)));
        setSelectedPath(null);
        return;
      }
      setLedger((prev) => [out, ...prev.filter((r) => !(r.from === a && r.to === b))]);
      setSelectedPath(out.path);
      setSetupOpen(false);
      await refreshPrefs();
      toast.success(t("review.saved"));
    } catch (e) {
      const msg = String(e);
      if (msg.includes("license_locked")) toast.error(t("review.locked"));
      else toast.error(t("review.fail"));
    } finally {
      setBusy(false);
    }
  };

  const closeReader = () => setSelectedPath(null);

  if (reading && selected) {
    const body = displayBody(selected);
    return (
      <div className="flex-1 min-h-0 flex flex-col">
        <header className="shrink-0 px-5 pt-2 pb-3 flex items-center gap-2 border-b border-app-border/70 dark:border-slate-800">
          <button
            type="button"
            onClick={closeReader}
            className="text-[13px] text-app-primary dark:text-blue-300 py-1"
          >
            {t("review.close")}
          </button>
          <span className={`ml-auto ${ui.sectionLabel}`}>{t("review.body")}</span>
        </header>
        <div className="flex-1 min-h-0 overflow-y-auto px-6 py-7">
          <article>
            <h2 className="text-[20px] font-medium tracking-tight text-app-fg dark:text-slate-50">
              {shortDay(selected.from)}–{shortDay(selected.to)}
            </h2>
            {selected.createdAt && (
              <p className="mt-1.5 text-[12px] text-app-fg-tertiary">
                {t("review.alreadyWhen", { date: createdDay(selected.createdAt) })}
              </p>
            )}
            {bodyEmpty(body) ? (
              <p className="mt-8 text-[14px] leading-relaxed text-app-fg-secondary">
                {t("review.bodyEmpty")}
              </p>
            ) : (
              <>
                {body.focus && (
                  <p className="mt-7 text-[15px] leading-[1.75] text-app-fg dark:text-slate-100">
                    {body.focus}
                  </p>
                )}
                {body.done.length > 0 && (
                  <Section title={t("review.done")}>
                    {body.done.map((d) => (
                      <li key={d} className="text-[14px] leading-relaxed py-0.5">
                        {d}
                      </li>
                    ))}
                  </Section>
                )}
                {body.outputs.length > 0 && (
                  <Section title={t("review.outputs")}>
                    {body.outputs.map((p) => (
                      <li
                        key={p}
                        className="font-mono text-[12px] text-app-fg-secondary break-all py-0.5"
                      >
                        {p}
                      </li>
                    ))}
                  </Section>
                )}
                {body.stillOwe.length > 0 && (
                  <Section title={t("review.stillOwe")}>
                    {body.stillOwe.map((d) => (
                      <li key={d} className="text-[14px] leading-relaxed py-0.5">
                        {d}
                      </li>
                    ))}
                  </Section>
                )}
                {body.stillOwe.length > 0 && (
                  <button
                    type="button"
                    onClick={() => setTab("zaiban")}
                    className="mt-5 text-[13px] text-app-primary dark:text-blue-300"
                  >
                    {t("review.openZaiban")}
                  </button>
                )}
              </>
            )}
          </article>
        </div>
        <footer className="shrink-0 px-5 py-3 border-t border-app-border/70 dark:border-slate-800 flex items-center gap-3">
          <Button
            variant="secondary"
            size="sm"
            disabled={busy}
            onClick={() => void run(selected.from, selected.to)}
          >
            {busy ? t("review.running") : t("review.again")}
          </Button>
        </footer>
      </div>
    );
  }

  return (
    <div className="flex-1 min-h-0 overflow-y-auto px-5 py-4 space-y-6">
      {ledger.length > 0 && (
        <PastList
          ledger={ledger}
          onOpen={(row) => {
            setSelectedPath(row.path);
            setEmptyRun(false);
            setSetupOpen(false);
          }}
          label={t("review.past")}
          written={(d) => t("review.pastWhen", { date: createdDay(d) })}
        />
      )}

      <section className="space-y-3">
        <p className="text-[15px] tracking-tight text-app-fg dark:text-slate-100">
          {rangeLabel(from, to, span, t)}
        </p>
        {emptyRun && (
          <p className="text-[13px] text-app-fg-tertiary leading-relaxed">{t("review.empty")}</p>
        )}
        {(() => {
          const existing = ledger.find((r) => r.from === from && r.to === to);
          if (existing && !emptyRun) {
            return (
              <>
                <p className="text-[13px] text-app-fg-secondary">
                  {t("review.alreadyWhen", { date: createdDay(existing.createdAt) })}
                </p>
                <Button
                  variant="primary"
                  className="w-full"
                  onClick={() => {
                    setSelectedPath(existing.path);
                    setSetupOpen(false);
                  }}
                >
                  {t("review.openBody")}
                </Button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void run(from, to)}
                  className="text-[12px] text-app-primary dark:text-blue-300"
                >
                  {busy ? t("review.running") : t("review.again")}
                </button>
              </>
            );
          }
          return (
            <Button
              variant="primary"
              className="w-full"
              disabled={busy || !from || !to}
              onClick={() => void run(from, to)}
            >
              {busy ? t("review.running") : t("review.run")}
            </Button>
          );
        })()}
        {ledger.length > 0 && (
          <button
            type="button"
            onClick={() => setSetupOpen((v) => !v)}
            className="text-[12px] text-app-primary dark:text-blue-300"
          >
            {t("review.setup")}
          </button>
        )}
      </section>

      {(ledger.length === 0 || setupOpen) && (
        <section className="space-y-2.5">
          <div className="flex flex-wrap gap-1.5">
            <Chip active={span === "this_week" && !custom} onClick={() => void setSpan("this_week")}>
              {t("review.spanWeek")}
            </Chip>
            <Chip
              active={span === "last_7_days" && !custom}
              onClick={() => void setSpan("last_7_days")}
            >
              {t("review.span7")}
            </Chip>
            <button
              type="button"
              onClick={() => setCustom((v) => !v)}
              className="px-2.5 py-1 text-[12px] text-app-primary dark:text-blue-300 hover:opacity-80"
            >
              {t("review.changeRange")}
            </button>
          </div>
          {custom && (
            <div className="flex items-center gap-2">
              <input
                type="date"
                value={from}
                onChange={(e) => {
                  setFrom(e.target.value);
                  setEmptyRun(false);
                }}
                className={`${ui.input} flex-1 min-w-0 py-1.5 text-[12px]`}
              />
              <span className="text-app-fg-tertiary text-[11px]">–</span>
              <input
                type="date"
                value={to}
                onChange={(e) => {
                  setTo(e.target.value);
                  setEmptyRun(false);
                }}
                className={`${ui.input} flex-1 min-w-0 py-1.5 text-[12px]`}
              />
            </div>
          )}
        </section>
      )}

      <section>
        <p className={`${ui.sectionLabel} mb-2`}>{t("review.whenAsk")}</p>
        <Cadence weekday={weekday} setWeekday={setWeekday} hint={t("review.whenHint")} t={t} />
      </section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mt-8 pt-6 border-t border-app-border/80 dark:border-slate-800">
      <h3 className="text-[11px] font-medium tracking-wide text-app-fg-tertiary mb-3">
        {title}
      </h3>
      <ul className="space-y-2.5 text-app-fg dark:text-slate-200">{children}</ul>
    </section>
  );
}

function Cadence({
  weekday,
  setWeekday,
  hint,
  t,
}: {
  weekday: number;
  setWeekday: (w: number) => void;
  hint: string;
  t: (k: (typeof CHIPS)[number]["label"]) => string;
}) {
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap gap-1.5">
        {CHIPS.map((c) => (
          <Chip key={c.label} active={weekday === c.w} onClick={() => setWeekday(c.w)}>
            {t(c.label)}
          </Chip>
        ))}
      </div>
      {weekday === 0 && (
        <p className="text-[11px] text-app-fg-tertiary leading-relaxed">{hint}</p>
      )}
    </div>
  );
}

function PastList({
  ledger,
  onOpen,
  label,
  written,
}: {
  ledger: ReviewEntry[];
  onOpen: (row: ReviewEntry) => void;
  label: string;
  written: (createdAt: string) => string;
}) {
  return (
    <section>
      <p className={`${ui.sectionLabel} mb-2`}>{label}</p>
      <ul className="space-y-1">
        {ledger.map((row) => {
          const preview = displayBody(row).focus;
          return (
            <li key={row.path}>
              <button
                type="button"
                onClick={() => onOpen(row)}
                className="w-full text-left px-3 py-2.5 rounded-xl hover:bg-app-muted dark:hover:bg-slate-800/70 transition-colors"
              >
                <span className="block text-[13.5px] text-app-fg dark:text-slate-100">
                  {shortDay(row.from)}–{shortDay(row.to)}
                </span>
                {preview && (
                  <span className="block mt-1 text-[12px] leading-relaxed text-app-fg-secondary line-clamp-2">
                    {preview}
                  </span>
                )}
                {row.createdAt && (
                  <span className="block mt-1 text-[11px] text-app-fg-tertiary">
                    {written(row.createdAt)}
                  </span>
                )}
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
