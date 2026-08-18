import { useEffect, useState } from "react";
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

export default function App() {
  const fetchSessions = useChatStore((s) => s.fetchSessions);
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
      await fetchSessions();
      const chat = useChatStore.getState();
      if (!chat.activeSessionId) {
        await chat.newSession();
      }
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
  }, [fetchSessions, setLanguage, setTheme, setHasApiKey, refreshLicense]);

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
      const mod = e.metaKey || e.ctrlKey;
      if (mod && (e.key === "n" || e.key === "N")) {
        e.preventDefault();
        const chat = useChatStore.getState();
        if (chat.isStreaming) {
          toast.info(useUiStore.getState().t("toast.streamingBusy"));
          return;
        }
        useNavStore.getState().setPanel("chat");
        void chat.newSession();
        return;
      }

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
            try {
              await invoke("mark_pending_leave", {
                sessionId: state.activeSessionId,
              });
            } catch {
              /* still close */
            }
            await win.destroy();
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
          onDone={() => {
            setShowOnboarding(false);
            const chat = useChatStore.getState();
            if (!chat.activeSessionId) {
              void chat.newSession();
            }
          }}
        />
      )}
      {/* License lock above onboarding so expired install cannot skip via onboarding */}
      <LicenseNudgeModal />
      <LicenseLockScreen />
    </div>
  );
}
