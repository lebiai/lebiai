import { Plus, MessageSquare, Trash2, Brain, Zap, Plug, Settings, Sparkles } from "lucide-react";
import { useChatStore } from "../../store/chatStore";
import { useNavStore, type Panel } from "../../store/navStore";
import { useUiStore } from "../../store/uiStore";
import type { TranslationKey } from "../../i18n";

const navItems: { panel: Panel; icon: typeof Brain; labelKey: TranslationKey }[] = [
  { panel: "chat", icon: MessageSquare, labelKey: "nav.chat" },
  { panel: "memory", icon: Brain, labelKey: "nav.memories" },
  { panel: "skills", icon: Zap, labelKey: "nav.skills" },
  { panel: "mcp", icon: Plug, labelKey: "nav.mcp" },
  { panel: "reflect", icon: Sparkles, labelKey: "nav.reflect" },
  { panel: "settings", icon: Settings, labelKey: "nav.settings" },
];

export function Sidebar() {
  const { sessions, activeSessionId, newSession, loadSession, deleteSession } =
    useChatStore();
  const { activePanel, setPanel } = useNavStore();
  const t = useUiStore((s) => s.t);

  return (
    <aside className="w-64 h-full flex flex-col border-r border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800">
      <div className="p-3">
        <button
          onClick={newSession}
          className="w-full flex items-center gap-2 px-3 py-2 rounded-lg border border-gray-300 dark:border-gray-600 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors text-sm"
        >
          <Plus size={16} />
          <span>{t("chat.new")}</span>
        </button>
      </div>

      {activePanel === "chat" && (
        <div className="flex-1 overflow-y-auto px-2 pb-2">
          {sessions.map((session) => (
            <div
              key={session.id}
              className={`group flex items-center gap-2 px-3 py-2 rounded-lg cursor-pointer text-sm mb-0.5 ${
                activeSessionId === session.id
                  ? "bg-gray-200 dark:bg-gray-700"
                  : "hover:bg-gray-100 dark:hover:bg-gray-700/50"
              }`}
              onClick={() => loadSession(session.path)}
            >
              <MessageSquare size={14} className="shrink-0 text-gray-400" />
              <span className="truncate flex-1">
                {session.title === "New Chat" ? t("chat.defaultTitle") : session.title}
              </span>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  deleteSession(session.path);
                }}
                className="opacity-0 group-hover:opacity-100 p-1 hover:text-red-500 transition-opacity"
                title={t("chat.deleteSession")}
              >
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </div>
      )}

      {activePanel !== "chat" && <div className="flex-1" />}

      <nav className="border-t border-gray-200 dark:border-gray-700 p-2 space-y-0.5">
        {navItems.map(({ panel, icon: Icon, labelKey }) => (
          <button
            key={panel}
            onClick={() => setPanel(panel)}
            className={`w-full flex items-center gap-2 px-3 py-2 rounded-lg text-sm transition-colors ${
              activePanel === panel
                ? "bg-gray-200 dark:bg-gray-700 font-medium"
                : "hover:bg-gray-100 dark:hover:bg-gray-700/50"
            }`}
          >
            <Icon size={16} />
            <span>{t(labelKey)}</span>
          </button>
        ))}
      </nav>
    </aside>
  );
}
