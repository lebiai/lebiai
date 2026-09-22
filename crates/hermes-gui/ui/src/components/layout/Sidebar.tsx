import { useEffect, useMemo, useState } from "react";
import { Brain, Settings } from "lucide-react";
import brandLogo from "../../assets/logo.png";
import { invoke } from "@tauri-apps/api/core";
import { useChatStore } from "../../store/chatStore";
import { useNavStore, type Panel } from "../../store/navStore";
import { useUiStore } from "../../store/uiStore";
import { refreshProviderLabel } from "../../store/uiStore";
import type { SessionSummary } from "../../types";
import type { TranslationKey } from "../../i18n";
import { ui } from "../common/ui";
import { toast } from "../../utils/toast";
import { LicenseSidebarHint } from "../license/LicenseSidebarHint";

/** Workstations are the dialogue entry. Bottom rail is secondary only. */
const primaryNav: { panel: Panel; icon: typeof Brain; labelKey: TranslationKey }[] = [
  { panel: "know", icon: Brain, labelKey: "nav.know" },
  { panel: "settings", icon: Settings, labelKey: "nav.settings" },
];

export function Sidebar() {
  const {
    sessions,
    fetchSessions,
    newSession,
    loadSession,
    isStreaming,
    sessionEnd,
    personas,
    personaId,
    teams,
    teamId,
  } = useChatStore();
  const { activePanel, setPanel, openPendingReview } = useNavStore();
  const t = useUiStore((s) => s.t);
  const displayName = useUiStore((s) => s.displayName);
  const providerLabel = useUiStore((s) => s.providerLabel);
  const busy = isStreaming || sessionEnd?.status === "review";
  const [inboxCount, setInboxCount] = useState(0);

  useEffect(() => {
    void refreshProviderLabel();
  }, []);

  useEffect(() => {
    const load = () => {
      void invoke<number>("count_pending_review")
        .then(setInboxCount)
        .catch(() => setInboxCount(0));
    };
    load();
    window.addEventListener("hermes:inbox-changed", load);
    return () => window.removeEventListener("hermes:inbox-changed", load);
  }, []);

  /**
   * 工位要按 `sessions` 找「这个人物最近的一段对话」。别的入口（微信/飞书）
   * 会在后台往里加会话，所以回到窗口时刷一次，免得点进去开重复的新对话。
   */
  useEffect(() => {
    const onVis = () => {
      if (document.visibilityState !== "visible") return;
      void fetchSessions();
      // 今天动过没取自会话文件本身，回到窗口时顺手刷一次。
      void useChatStore.getState().fetchTeams();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => document.removeEventListener("visibilitychange", onVis);
  }, [fetchSessions]);

  /** 授权人物在前、自带人物在后；两组内部保持后端原顺序。 */
  const enabledPersonas = useMemo(
    () => [
      ...personas.filter((p) => p.enabled && !p.builtin),
      ...personas.filter((p) => p.enabled && p.builtin),
    ],
    [personas]
  );

  /**
   * 点一个工作位：优先回到它最近的那段对话，没有就开一段新的。
   * 工位（人）与项目组（事）走**同一套**——差别只在「它认哪个字段」。
   */
  const openWorkplace = ({
    current,
    find,
    open,
  }: {
    current: boolean;
    find: (s: SessionSummary) => boolean;
    open: () => Promise<void>;
  }) => {
    if (busy) {
      toast.info(t("toast.streamingBusy"));
      return;
    }
    setPanel("chat");
    if (current) return;
    const latest = sessions
      .filter(find)
      .sort((a, b) =>
        (b.updatedAt ?? b.createdAt).localeCompare(a.updatedAt ?? a.createdAt)
      )[0];
    if (latest) {
      void loadSession(latest.path);
    } else {
      void open();
    }
  };

  const openStation = (id: string) =>
    openWorkplace({
      current: personaId === id,
      find: (s) => s.persona === id,
      open: () => newSession(id),
    });

  /** 项目组：一条会话日复一日，所以同样是「回到它那一条」。 */
  const openTeam = (id: string) =>
    openWorkplace({
      current: teamId === id,
      find: (s) => s.team === id,
      open: () => newSession(undefined, id),
    });

  const navButton = (panel: Panel, Icon: typeof Brain, labelKey: TranslationKey) => {
    const active = activePanel === panel;
    const shell = `${ui.navItem} ${active ? ui.navItemActive : ui.navItemIdle}`;
    if (panel === "know" && inboxCount > 0) {
      return (
        <div key={panel} className={`${shell} !py-0 !pr-1`}>
          <button
            type="button"
            onClick={() => setPanel("know")}
            className="flex-1 flex items-center gap-2 min-w-0 py-2 text-left"
          >
            <Icon size={16} className="shrink-0 opacity-90" strokeWidth={1.75} />
            <span className="truncate">{t(labelKey)}</span>
          </button>
          <button
            type="button"
            title={t("memory.pendingZone")}
            aria-label={t("memory.pendingZone")}
            onClick={openPendingReview}
            className="shrink-0 text-xs font-semibold min-w-[1.15rem] h-4 px-1 rounded-full bg-app-primary text-white flex items-center justify-center"
          >
            {inboxCount > 99 ? "99+" : inboxCount}
          </button>
        </div>
      );
    }
    return (
      <button
        key={panel}
        type="button"
        onClick={() => setPanel(panel)}
        className={shell}
      >
        <Icon size={16} className="shrink-0 opacity-90" strokeWidth={1.75} />
        <span className="flex-1 text-left">{t(labelKey)}</span>
      </button>
    );
  };

  return (
    <aside className={`w-[17rem] h-full flex flex-col shrink-0 ${ui.sidebar}`}>
      <div className="px-3 pt-3.5 pb-2 shrink-0">
        <div className="flex items-center gap-2 px-1">
          <img
            src={brandLogo}
            alt={t("app.brand")}
            className="h-8 w-8 shrink-0 rounded-xl object-cover shadow-sm"
          />
          <div className="min-w-0 flex-1">
            <div className="text-sm font-semibold tracking-tight text-app-fg dark:text-slate-100">
              {t("app.brand")}
            </div>
            <div className="text-app-sub text-app-fg-secondary dark:text-slate-400 truncate">
              {t("sidebar.tagline")}
            </div>
          </div>
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto px-2 pb-2">
        <div className={`px-2 pt-1 pb-1.5 ${ui.sectionLabel}`}>
          {t("persona.section")}
        </div>
        {enabledPersonas.map((p) => {
          const current = personaId === p.id;
          return (
            <button
              key={p.id}
              type="button"
              onClick={() => openStation(p.id)}
              title={p.role}
              className={`w-full flex items-center gap-2 pl-2.5 pr-2 py-1.5 rounded-lg text-sm mb-0.5 text-left ${
                current ? ui.sessionActive : ui.sessionIdle
              } ${busy ? "pointer-events-none opacity-55" : ""}`}
            >
              <span className="shrink-0 w-6 h-6 rounded-full bg-app-primary-soft dark:bg-blue-950/60 text-app-primary dark:text-blue-300 flex items-center justify-center text-xs font-semibold">
                {p.name[0]}
              </span>
              <span className="flex-1 min-w-0">
                <span className="block truncate leading-snug">{p.name}</span>
                <span className="block truncate text-app-sub text-app-fg-secondary leading-tight">
                  {p.role}
                </span>
              </span>
            </button>
          );
        })}

        {teams.length > 0 && (
          <>
            <div className={`px-2 pt-3 pb-1.5 ${ui.sectionLabel}`}>
              {t("team.section")}
            </div>
            {teams.map((team) => {
              const current = teamId === team.id;
              // 缺席 / 缺接口人都是信息，不是故障：灰掉、写明缺谁，不做假按钮。
              const short = team.missing > 0 || !team.canRun;
              return (
                <button
                  key={team.id}
                  type="button"
                  onClick={() => openTeam(team.id)}
                  disabled={!team.canRun}
                  title={team.canRun ? team.role : team.hint}
                  className={`w-full flex items-center gap-2 pl-2.5 pr-2 py-1.5 rounded-lg text-sm mb-0.5 text-left ${
                    current ? ui.sessionActive : ui.sessionIdle
                  } ${
                    !team.canRun
                      ? "opacity-55 cursor-not-allowed"
                      : busy
                        ? "pointer-events-none opacity-55"
                        : ""
                  }`}
                >
                  <span className="shrink-0 w-6 h-6 rounded-md bg-app-primary-soft dark:bg-blue-950/60 text-app-primary dark:text-blue-300 flex items-center justify-center text-xs font-semibold">
                    {team.name[0]}
                  </span>
                  <span className="flex-1 min-w-0">
                    <span className="block truncate leading-snug">
                      {team.name}
                    </span>
                    {/*
                      缺人是**要处理的事**，不是装饰：挤成一行会被裁在字中间，
                      所以警示态给两行，平静态才截断。
                    */}
                    <span
                      className={`block text-app-sub leading-tight ${
                        short
                          ? "text-amber-600 dark:text-amber-400"
                          : "truncate text-app-fg-tertiary"
                      }`}
                    >
                      {team.hint}
                    </span>
                  </span>
                </button>
              );
            })}
          </>
        )}
      </div>

      <nav className="border-t border-app-border dark:border-slate-800 p-2 space-y-0.5 shrink-0 max-h-[40vh] overflow-y-auto">
        {primaryNav.map(({ panel, icon, labelKey }) => navButton(panel, icon, labelKey))}
      </nav>

      <div className="shrink-0 border-t border-app-border dark:border-slate-800 px-3 py-2.5">
        <div className="flex items-center gap-2.5 min-w-0">
          <div className="w-8 h-8 rounded-full bg-app-primary-soft dark:bg-blue-950/60 text-app-primary dark:text-blue-300 flex items-center justify-center text-sm font-semibold shrink-0">
            {displayName ? displayName[0].toUpperCase() : "乐"}
          </div>
          <div className="min-w-0 flex-1">
            <div className="text-xs font-medium text-app-fg dark:text-slate-100 truncate">
              {displayName || t("sidebar.userGuest")}
            </div>
            {/* Provider when calm; license chip/date when it matters — not a separate battery block */}
            {/*
              两段信息挤一行时会被裁成「不足 1 …」——授权提示是**要看的**，
              所以允许换行，宁可占两行也不截断。
            */}
            <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 min-w-0 mt-0.5">
              <span className="text-app-sub text-app-fg-secondary dark:text-slate-400 truncate min-w-0">
                {providerLabel}
              </span>
              <LicenseSidebarHint />
            </div>
          </div>
        </div>
      </div>
    </aside>
  );
}
