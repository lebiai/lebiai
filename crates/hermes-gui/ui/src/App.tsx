import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Sidebar } from "./components/layout/Sidebar";
import { ChatView } from "./components/chat/ChatView";
import { MemoryPanel } from "./components/memory/MemoryPanel";
import { SkillPanel } from "./components/skills/SkillPanel";
import { McpPanel } from "./components/mcp/McpPanel";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { ReflectPanel } from "./components/reflect/ReflectPanel";
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
import { bindSystemThemeWatcher, useUiStore } from "./store/uiStore";
import { applyTheme } from "./utils/theme";
import { isOnboardingDone } from "./utils/onboarding";
import { toast } from "./utils/toast";

export default function App() {
  const fetchSessions = useChatStore((s) => s.fetchSessions);
  const { activePanel } = useNavStore();
  const setLanguage = useUiStore((s) => s.setLanguage);
  const setTheme = useUiStore((s) => s.setTheme);
  const setHasApiKey = useUiStore((s) => s.setHasApiKey);
  const [showOnboarding, setShowOnboarding] = useState(() => !isOnboardingDone());
  const onboardingRequestId = useUiStore((s) => s.onboardingRequestId);

  useEffect(() => {
    if (onboardingRequestId > 0) {
      setShowOnboarding(true);
    }
  }, [onboardingRequestId]);

  useEffect(() => {
    applyTheme(useUiStore.getState().theme);
    fetchSessions();
    invoke<{
      uiLanguage: string;
      uiTheme: string;
      hasApiKey: boolean;
    }>("get_config")
      .then((config) => {
        setLanguage(config.uiLanguage);
        setTheme(config.uiTheme ?? "system");
        setHasApiKey(!!config.hasApiKey);
      })
      .catch(() => {
        setLanguage("en-US");
        setTheme("system");
        setHasApiKey(false);
      });
  }, [fetchSessions, setLanguage, setTheme, setHasApiKey]);

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
        // Proposed skill first
        if (chat.proposedSkills.length > 0) {
          e.preventDefault();
          chat.dismissProposedSkill(chat.proposedSkills[0].name);
          return;
        }
        // Micro-reflection in-chat review
        if (chat.microReviewOpen) {
          e.preventDefault();
          chat.dismissMicroReview();
          return;
        }
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
      case "memory":
        return <MemoryPanel />;
      case "skills":
        return <SkillPanel />;
      case "mcp":
        return <McpPanel />;
      case "reflect":
        return <ReflectPanel />;
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
        <OnboardingRitual onDone={() => setShowOnboarding(false)} />
      )}
    </div>
  );
}
