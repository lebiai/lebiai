import { useEffect, useMemo, useRef, useState } from "react";
import {
  Plus,
  MessageSquare,
  Trash2,
  Brain,
  Settings,
  Search,
  X,
  RefreshCw,
} from "lucide-react";
import brandLogo from "../../assets/logo.png";
import { invoke } from "@tauri-apps/api/core";
import { useChatStore } from "../../store/chatStore";
import { useNavStore, type Panel } from "../../store/navStore";
import { useUiStore } from "../../store/uiStore";
import { refreshProviderLabel } from "../../store/uiStore";
import type { TranslationKey } from "../../i18n";
import type { SessionSummary } from "../../types";
import { Button, ui } from "../common/ui";
import { ConfirmPopover } from "../common/ConfirmPopover";
import { toast } from "../../utils/toast";
import { isDefaultTitle } from "../../utils/sessionTitle";
import {
  formatSessionActivity,
  groupSessionsByDay,
  type SessionGroupId,
} from "../../utils/sessionTime";
import { LicenseSidebarHint } from "../license/LicenseSidebarHint";

/** Dialogue first; Continuity/Evolve in one place; settings last. */
const primaryNav: { panel: Panel; icon: typeof Brain; labelKey: TranslationKey }[] = [
  { panel: "chat", icon: MessageSquare, labelKey: "nav.chat" },
  { panel: "know", icon: Brain, labelKey: "nav.know" },
  { panel: "settings", icon: Settings, labelKey: "nav.settings" },
];

const groupLabelKey: Record<SessionGroupId, TranslationKey> = {
  today: "chat.groupToday",
  yesterday: "chat.groupYesterday",
  wechat: "chat.groupWechat",
  earlier: "chat.groupEarlier",
};

function sessionTitleOf(
  session: SessionSummary,
  t: (key: TranslationKey) => string
): string {
  return isDefaultTitle(session.title) ? t("chat.defaultTitle") : session.title;
}

export function Sidebar() {
  const {
    sessions,
    draftSession,
    sessionsLoading,
    sessionsError,
    fetchSessions,
    clearSessionsError,
    activeSessionId,
    newSession,
    loadSession,
    deleteSession,
    isStreaming,
    sessionEnd,
  } = useChatStore();
  const { activePanel, setPanel, openPendingReview } = useNavStore();
  const t = useUiStore((s) => s.t);
  const language = useUiStore((s) => s.language);
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

  const [query, setQuery] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const [confirmDeletePath, setConfirmDeletePath] = useState<string | null>(null);

  useEffect(() => {
    if (searchOpen) {
      searchInputRef.current?.focus();
    }
  }, [searchOpen]);

  useEffect(() => {
    if (!searchOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setSearchOpen(false);
        setQuery("");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [searchOpen]);

  const q = query.trim().toLowerCase();

  const draftMatches = useMemo(() => {
    if (!draftSession) return false;
    if (!q) return true;
    return sessionTitleOf(draftSession, t).toLowerCase().includes(q);
  }, [draftSession, q, t]);

  const filtered = useMemo(() => {
    if (!q) return sessions;
    return sessions.filter((s) => sessionTitleOf(s, t).toLowerCase().includes(q));
  }, [sessions, q, t]);

  const groups = useMemo(() => groupSessionsByDay(filtered), [filtered]);

  const locale = language === "zh-CN" ? "zh-CN" : "en-US";

  const hasListContent =
    (draftSession && draftMatches) || filtered.length > 0;

  const openSession = (path: string) => {
    if (busy) return;
    setPanel("chat");
    void loadSession(path);
  };

  const handleNew = () => {
    if (busy) {
      toast.info(t("toast.streamingBusy"));
      return;
    }
    setPanel("chat");
    void newSession();
  };

  const openDraft = () => {
    if (busy || !draftSession) return;
    // Draft stays memory-resident (activeSessionId already points here when set).
    setPanel("chat");
  };

  const closeSearch = () => {
    setSearchOpen(false);
    setQuery("");
  };

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
            className="shrink-0 text-[10px] font-semibold min-w-[1.15rem] h-4 px-1 rounded-full bg-app-primary text-white flex items-center justify-center"
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
        <div className="flex items-center gap-2 px-1 mb-3">
          <img
            src={brandLogo}
            alt={t("app.brand")}
            className="h-8 w-8 shrink-0 rounded-xl object-cover shadow-sm"
          />
          <div className="min-w-0 flex-1">
            <div className="text-sm font-semibold tracking-tight text-app-fg dark:text-slate-100">
              {t("app.brand")}
            </div>
            <div className="text-[11px] text-app-fg-tertiary dark:text-slate-500 truncate">
              {t("app.tagline")}
            </div>
          </div>
          <button
            type="button"
            title={t("chat.searchToggle")}
            aria-label={t("chat.searchToggle")}
            aria-expanded={searchOpen}
            onClick={() => {
              if (searchOpen) closeSearch();
              else setSearchOpen(true);
            }}
            className={`shrink-0 p-1.5 rounded-lg transition-colors ${
              searchOpen || q
                ? "text-app-primary bg-app-primary-soft dark:bg-blue-950/40"
                : "text-app-fg-tertiary hover:text-app-fg-secondary hover:bg-app-muted dark:hover:bg-slate-800"
            }`}
          >
            <Search size={15} />
          </button>
        </div>
        <Button variant="primary" className="w-full" onClick={handleNew} disabled={busy}>
          <Plus size={16} />
          <span>{t("chat.new")}</span>
          <span className="ml-auto text-[10px] opacity-70 font-normal hidden sm:inline">
            ⌘N
          </span>
        </Button>
      </div>

      {searchOpen && (
        <div className="px-3 pb-2 shrink-0">
          <div className="relative">
            <Search
              size={13}
              className="absolute left-2.5 top-1/2 -translate-y-1/2 text-app-fg-tertiary pointer-events-none"
            />
            <input
              ref={searchInputRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("chat.searchSessions")}
              className="w-full pl-8 pr-8 py-1.5 text-xs rounded-lg border border-app-border dark:border-slate-700 bg-app-surface dark:bg-slate-800/80 text-app-fg dark:text-slate-200 placeholder:text-app-fg-tertiary focus:outline-none focus:ring-2 focus:ring-app-primary/30 select-text"
            />
            <button
              type="button"
              aria-label={t("common.dismiss")}
              className="absolute right-1.5 top-1/2 -translate-y-1/2 p-1 rounded-md text-app-fg-tertiary hover:text-app-fg-secondary"
              onClick={closeSearch}
            >
              <X size={12} />
            </button>
          </div>
        </div>
      )}

      <div className="flex-1 min-h-0 overflow-y-auto px-2 pb-2">
        <div className={`px-2 pt-0.5 pb-1.5 ${ui.sectionLabel}`}>
          {t("chat.recentSessions")}
        </div>

        {sessionsError && (
          <div className="mx-1 mb-2 rounded-lg border border-red-200 dark:border-red-800 bg-red-50 dark:bg-red-950/30 px-2 py-2 space-y-1.5">
            <p className="text-[11px] text-red-700 dark:text-red-300 leading-snug break-all">
              {t("chat.sessionsError")}
            </p>
            <button
              type="button"
              className="inline-flex items-center gap-1 text-[11px] text-red-800 dark:text-red-200 font-medium"
              onClick={() => {
                clearSessionsError();
                void fetchSessions();
              }}
            >
              <RefreshCw size={11} />
              {t("chat.retrySessions")}
            </button>
          </div>
        )}

        {sessionsLoading && !hasListContent && (
          <p className="px-2.5 py-3 text-xs text-app-fg-tertiary">{t("chat.loadingSessions")}</p>
        )}

        {!sessionsLoading && !sessionsError && !hasListContent && (
          <p className="px-2.5 py-3 text-xs text-app-fg-tertiary leading-relaxed">
            {q ? t("chat.noSearchResults") : t("chat.noSessions")}
          </p>
        )}

        {/* Empty draft — history zone, marked "current", not a second New Chat CTA */}
        {draftSession && draftMatches && (
          <div
            className={`group relative flex items-center gap-2 pl-2.5 pr-1 py-1.5 rounded-lg cursor-pointer text-sm mb-1 ${
              activePanel === "chat" && activeSessionId === draftSession.id
                ? ui.sessionActive
                : ui.sessionIdle
            } ${busy ? "pointer-events-none opacity-55" : ""}`}
            onClick={openDraft}
          >
            <MessageSquare
              size={14}
              className={`shrink-0 ${
                activePanel === "chat" && activeSessionId === draftSession.id
                  ? "text-app-primary dark:text-blue-400"
                  : "text-app-fg-tertiary"
              }`}
            />
            <div className="flex-1 min-w-0">
              <div className="truncate leading-snug text-app-fg-secondary dark:text-slate-300">
                {sessionTitleOf(draftSession, t)}
              </div>
            </div>
            <span className="shrink-0 text-[10px] font-medium px-1.5 py-0.5 rounded bg-app-muted dark:bg-slate-800 text-app-fg-tertiary">
              {t("chat.currentBadge")}
            </span>
          </div>
        )}

        {groups.map((group) => (
          <div key={group.id} className="mb-2">
            <div className={`px-2 py-1 ${ui.sectionLabel}`}>
              {t(groupLabelKey[group.id])}
            </div>
            {group.sessions.map((session) => {
              const selected =
                activePanel === "chat" && activeSessionId === session.id;
              const title = sessionTitleOf(session, t);
              const timeLabel = formatSessionActivity(session, locale);
              return (
                <div
                  key={session.id}
                  className={`group relative flex items-center gap-2 pl-2.5 pr-1 py-1.5 rounded-lg cursor-pointer text-sm mb-0.5 ${
                    selected ? ui.sessionActive : ui.sessionIdle
                  } ${busy ? "pointer-events-none opacity-55" : ""}`}
                  onClick={() => openSession(session.path)}
                >
                  <MessageSquare
                    size={14}
                    className={`shrink-0 ${
                      selected
                        ? "text-app-primary dark:text-blue-400"
                        : "text-app-fg-tertiary"
                    }`}
                  />
                  <div className="flex-1 min-w-0">
                    <div className="truncate leading-snug">{title}</div>
                    {timeLabel && (
                      <div className="text-[10px] text-app-fg-tertiary truncate leading-tight">
                        {timeLabel}
                      </div>
                    )}
                  </div>
                  <button
                    type="button"
                    title={t("chat.deleteSession")}
                    aria-label={t("chat.deleteSession")}
                    className="p-1 rounded-md text-app-fg-tertiary hover:text-app-danger hover:bg-red-50 dark:hover:bg-red-950/40"
                    onClick={(e) => {
                      e.stopPropagation();
                      if (!busy) setConfirmDeletePath(session.path);
                    }}
                  >
                    <Trash2 size={12} />
                  </button>
                  <ConfirmPopover
                    open={confirmDeletePath === session.path}
                    message={t("chat.deleteSessionConfirm")}
                    onCancel={() => setConfirmDeletePath(null)}
                    onConfirm={() => {
                      setConfirmDeletePath(null);
                      void deleteSession(session.path)
                        .then(() => toast.success(t("toast.sessionDeleted")))
                        .catch((err) => toast.error(String(err)));
                    }}
                  />
                </div>
              );
            })}
          </div>
        ))}
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
            <div className="flex items-center gap-1.5 min-w-0 mt-0.5">
              <span className="text-[10px] text-app-fg-tertiary dark:text-slate-500 truncate shrink min-w-0">
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
