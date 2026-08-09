import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Server, Wrench, Plug } from "lucide-react";
import { useUiStore } from "../../store/uiStore";
import { EmptyState, PanelShell, ui } from "../common/ui";

interface McpServerInfo {
  name: string;
  kind: string;
  detail: string;
}

interface McpToolItem {
  name: string;
  description: string;
}

export function McpPanel() {
  const t = useUiStore((state) => state.t);
  const [servers, setServers] = useState<McpServerInfo[]>([]);
  const [tools, setTools] = useState<McpToolItem[]>([]);
  const [tab, setTab] = useState<"servers" | "tools">("servers");

  useEffect(() => {
    invoke<McpServerInfo[]>("list_mcp_servers").then(setServers);
    invoke<McpToolItem[]>("list_mcp_tools").then(setTools);
  }, []);

  const tabBar = (
    <div className="flex gap-1 p-0.5 rounded-lg bg-app-muted dark:bg-slate-800">
      <button
        type="button"
        onClick={() => setTab("servers")}
        className={`px-3 py-1 text-sm rounded-md transition-colors duration-[var(--motion-fast)] ${
          tab === "servers"
            ? "bg-app-surface dark:bg-slate-700 font-medium shadow-sm text-app-fg"
            : "text-app-fg-secondary hover:text-app-fg"
        }`}
      >
        {t("mcp.servers")} ({servers.length})
      </button>
      <button
        type="button"
        onClick={() => setTab("tools")}
        className={`px-3 py-1 text-sm rounded-md transition-colors duration-[var(--motion-fast)] ${
          tab === "tools"
            ? "bg-app-surface dark:bg-slate-700 font-medium shadow-sm text-app-fg"
            : "text-app-fg-secondary hover:text-app-fg"
        }`}
      >
        {t("mcp.tools")} ({tools.length})
      </button>
    </div>
  );

  return (
    <PanelShell
      title={t("mcp.title")}
      subtitle={t("mcp.subtitle")}
      actions={tabBar}
      bodyClassName="p-4 space-y-3 max-w-3xl mx-auto w-full"
    >
      {tab === "servers" && (
        <>
          {servers.length === 0 && (
            <EmptyState
              icon={<Plug size={22} strokeWidth={1.75} />}
              tone="neutral"
              title={t("mcp.noServersTitle")}
              description={t("mcp.noServers")}
            />
          )}
          {servers.map((srv) => (
            <div key={srv.name} className={`${ui.card} p-3.5 space-y-1 fade-up-in`}>
              <div className="flex items-center gap-2">
                <Server size={14} className="text-app-fg-tertiary" />
                <span className="font-medium text-sm text-app-fg dark:text-slate-100">
                  {srv.name}
                </span>
                <span className="text-xs px-1.5 py-0.5 rounded-md bg-app-muted dark:bg-slate-800 text-app-fg-secondary">
                  {srv.kind}
                </span>
              </div>
              <p className="text-xs text-app-fg-tertiary font-mono truncate">
                {srv.detail}
              </p>
            </div>
          ))}
        </>
      )}

      {tab === "tools" && (
        <>
          {tools.length === 0 && (
            <EmptyState
              icon={<Wrench size={22} strokeWidth={1.75} />}
              tone="neutral"
              title={t("mcp.noToolsTitle")}
              description={t("mcp.noTools")}
            />
          )}
          {tools.map((tool) => (
            <div key={tool.name} className={`${ui.card} p-3.5 space-y-1 fade-up-in`}>
              <div className="flex items-center gap-2">
                <Wrench size={14} className="text-app-fg-tertiary" />
                <span className="font-medium text-sm font-mono text-app-fg dark:text-slate-100">
                  {tool.name}
                </span>
              </div>
              <p className="text-xs text-app-fg-secondary leading-relaxed">
                {tool.description}
              </p>
            </div>
          ))}
        </>
      )}
    </PanelShell>
  );
}
