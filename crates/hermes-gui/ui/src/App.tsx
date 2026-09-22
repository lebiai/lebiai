import { useEffect, useState } from "react";
import { exit } from "@tauri-apps/plugin-process";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Sidebar } from "./components/layout/Sidebar";
import { ChatView } from "./components/chat/ChatView";
import { KnowPanel } from "./components/know/KnowPanel";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { SessionEndModal } from "./components/reflect/SessionEndModal";

import { ToastHost } from "./components/common/ToastHost";
import { ErrorBoundary } from "./components/common/ErrorBoundary";
import { OnboardingRitual } from "./components/ritual/OnboardingRitual";
import { RitualSealHost } from "./components/ritual/RitualSealHost";
import {
  bindMicroReflectionListener,
  useChatStore,
} from "./store/chatStore";
import { useNavStore } from "./store/navStore";
import {
  bindSystemThemeWatcher,
  refreshProviderLabel,
  useUiStore,
} from "./store/uiStore";
import { applyTheme } from "./utils/theme";
import { isOnboardingDone } from "./utils/onboarding";
import { toast } from "./utils/toast";
import { useLicenseStore } from "./store/licenseStore";
import { bindZaibanListener, useZaibanStore } from "./store/zaibanStore";
import { useWorkDrawerStore } from "./store/workDrawerStore";
import { LicenseLockScreen } from "./components/license/LicenseLockScreen";
import { LicenseNudgeModal } from "./components/license/LicenseNudgeModal";

/** 关闭前留给「下次启动补齐」的写盘窗口；超时也照关，不让记录拦住宿主。 */
const LEAVE_MARK_TIMEOUT_MS = 1500;

/**
 * 关闭键必须关得掉：`destroy()` 是受权命令，一旦被拒（权限漏配、桥接异常）
 * 就会留下一个「点了没反应」的窗口。这里退到进程退出兜底，绝不静默卡住。
 * 授权与根因见 `docs/records/20260913-gui-close-button.md`。
 */
async function forceClose() {
  try {
    await getCurrentWindow().destroy();
  } catch (e) {
    console.error("window.destroy failed; falling back to app exit", e);
    await exit(0);
  }
}

/** 先记一笔「下次启动补齐这次会话」，再关闭；写不上记录也照样关。 */
async function markLeaveThenClose(sessionId: string) {
  try {
    await Promise.race([
      invoke("mark_pending_leave", { sessionId }),
      new Promise((resolve) => setTimeout(resolve, LEAVE_MARK_TIMEOUT_MS)),
    ]);
  } catch {
    /* 记录失败不拦关闭 */
  }
  await forceClose();
}

export default function App() {
  const fetchSessions = useChatStore((s) => s.fetchSessions);
  const fetchPersonas = useChatStore((s) => s.fetchPersonas);
  const fetchTeams = useChatStore((s) => s.fetchTeams);
  const { activePanel } = useNavStore();
  const setLanguage = useUiStore((s) => s.setLanguage);
  const setTheme = useUiStore((s) => s.setTheme);
  const setHasApiKey = useUiStore((s) => s.setHasApiKey);
  const refreshLicense = useLicenseStore((s) => s.refresh);
  const [showOnboarding, setShowOnboarding] = useState(() => !isOnboardingDone());
  const onboardingRequestId = useUiStore((s) => s.onboardingRequestId);

  useEffect(() => {
    if (onboardingRequestId > 0) {
      setShowOnboarding(true);
    }
  }, [onboardingRequestId]);

  useEffect(() => {
    applyTheme(useUiStore.getState().theme);
    void (async () => {
      await Promise.all([fetchSessions(), fetchPersonas(), fetchTeams()]);
      // 冷启动不自动开会话：每个会话都绑一个人物，谁开由用户点侧栏决定。
      // 没有会话时对话区是一句问候（`ChatView` 的空态），不是「正在开启…」。
    })();
    bindZaibanListener();
    void useZaibanStore.getState().refresh();
    void useWorkDrawerStore.getState().refreshPrefs();
    void refreshLicense();
    void invoke("drain_pending_leave").catch(() => undefined);
    invoke<{
      uiLanguage: string;
      uiTheme: string;
      hasApiKey: boolean;
    }>("get_config")
      .then((config) => {
        setLanguage(config.uiLanguage);
        setTheme(config.uiTheme ?? "system");
        setHasApiKey(!!config.hasApiKey);
        void refreshProviderLabel();
      })
      .catch(() => {
        setLanguage("zh-CN");
        setTheme("system");
        setHasApiKey(false);
      });
  }, [
    fetchSessions,
    fetchPersonas,
    fetchTeams,
    setLanguage,
    setTheme,
    setHasApiKey,
    refreshLicense,
  ]);

  // Re-check license when returning to the app (cross-day nudge / expiry).
  useEffect(() => {
    const onVis = () => {
      if (document.visibilityState === "visible") {
        void refreshLicense();
        void useWorkDrawerStore.getState().refreshPrefs();
      }
    };
    document.addEventListener("visibilitychange", onVis);
    return () => document.removeEventListener("visibilitychange", onVis);
  }, [refreshLicense]);

  // Single frontend source for the display name: read the onboarding seed
  // once at boot into uiStore; onboarding/settings writes sync it live.
  useEffect(() => {
    const setDisplayName = useUiStore.getState().setDisplayName;
    invoke<{ displayName: string; scenarios: string[] } | null>(
      "onboarding_seed_get",
    )
      .then((seed) => setDisplayName(seed?.displayName?.trim() || null))
      .catch(() => setDisplayName(null));
  }, []);

  useEffect(() => bindSystemThemeWatcher(), []);

  // Micro-reflection is a global app event (not the turn stream Channel).
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void bindMicroReflectionListener().then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  // ⌘/Ctrl+N new chat; Esc dismiss overlays (not tool confirm — safety).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // 不再有「⌘N 新对话」：用户不自己建会话，会话由侧栏选工位产生。
      if (e.key === "Escape") {
        if (showOnboarding) {
          e.preventDefault();
          // Skip is explicit via button; Esc does not skip onboarding (avoid accidental dismiss of first-run contract).
          return;
        }
        const chat = useChatStore.getState();
        // Session-end review (not background banner)
        if (chat.sessionEnd?.status === "review") {
          e.preventDefault();
          void chat.completeSessionEnd();
          return;
        }
        if (chat.sessionEnd?.status === "background") {
          e.preventDefault();
          chat.dismissSessionEnd();
          return;
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [showOnboarding]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    (async () => {
      try {
        const win = getCurrentWindow();
        unlisten = await win.onCloseRequested(async (event) => {
          const state = useChatStore.getState();
          if (state.isStreaming) {
            event.preventDefault();
            toast.info(useUiStore.getState().t("toast.streamingCloseBlocked"));
            return;
          }
          if (state.activeSessionId && state.messages.length > 0) {
            event.preventDefault();
            await markLeaveThenClose(state.activeSessionId);
          }
        });
        if (cancelled) {
          unlisten?.();
        }
      } catch (e) {
        console.debug("window close hook unavailable", e);
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const renderPanel = () => {
    switch (activePanel) {
      case "know":
        return <KnowPanel />;
      case "settings":
        return <SettingsPanel />;
      default:
        return <ChatView />;
    }
  };

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-app-bg dark:bg-slate-950 text-app-fg dark:text-slate-100 font-sans">
      <ErrorBoundary region="sidebar">
        <Sidebar />
      </ErrorBoundary>
      <main className="flex-1 flex flex-col min-w-0 bg-app-bg dark:bg-slate-950">
        <div
          key={activePanel}
          className="flex-1 flex flex-col min-w-0 min-h-0 panel-enter"
        >
          <ErrorBoundary region="main">{renderPanel()}</ErrorBoundary>
        </div>
      </main>
      <SessionEndModal />
      <RitualSealHost />
      <ToastHost />
      {showOnboarding && (
        <OnboardingRitual
          onDone={() => setShowOnboarding(false)}
        />
      )}
      {/* License lock above onboarding so expired install cannot skip via onboarding */}
      <LicenseNudgeModal />
      <LicenseLockScreen />
    </div>
  );
}
