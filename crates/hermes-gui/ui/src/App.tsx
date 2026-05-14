import { useEffect } from "react";
import { Sidebar } from "./components/layout/Sidebar";
import { ChatView } from "./components/chat/ChatView";
import { MemoryPanel } from "./components/memory/MemoryPanel";
import { SkillPanel } from "./components/skills/SkillPanel";
import { McpPanel } from "./components/mcp/McpPanel";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { ReflectPanel } from "./components/reflect/ReflectPanel";
import { useChatStore } from "./store/chatStore";
import { useNavStore } from "./store/navStore";

export default function App() {
  const { fetchSessions } = useChatStore();
  const { activePanel } = useNavStore();

  useEffect(() => {
    fetchSessions();
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
    <div className="flex h-screen w-screen overflow-hidden bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100">
      <Sidebar />
      <main className="flex-1 flex flex-col min-w-0">
        {renderPanel()}
      </main>
    </div>
  );
}
