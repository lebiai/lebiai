import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Server, Wrench } from "lucide-react";

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
  const [servers, setServers] = useState<McpServerInfo[]>([]);
  const [tools, setTools] = useState<McpToolItem[]>([]);
  const [tab, setTab] = useState<"servers" | "tools">("servers");

  useEffect(() => {
    invoke<McpServerInfo[]>("list_mcp_servers").then(setServers);
    invoke<McpToolItem[]>("list_mcp_tools").then(setTools);
  }, []);

  return (
    <div className="flex-1 flex flex-col h-full">
      <header className="flex items-center gap-4 px-4 py-3 border-b border-gray-200 dark:border-gray-700">
        <h2 className="text-lg font-semibold">MCP</h2>
        <div className="flex gap-1">
          <button
            onClick={() => setTab("servers")}
            className={`px-3 py-1 text-sm rounded-lg transition-colors ${
              tab === "servers"
                ? "bg-gray-200 dark:bg-gray-700 font-medium"
                : "hover:bg-gray-100 dark:hover:bg-gray-700/50"
            }`}
          >
            Servers ({servers.length})
          </button>
          <button
            onClick={() => setTab("tools")}
            className={`px-3 py-1 text-sm rounded-lg transition-colors ${
              tab === "tools"
                ? "bg-gray-200 dark:bg-gray-700 font-medium"
                : "hover:bg-gray-100 dark:hover:bg-gray-700/50"
            }`}
          >
            Tools ({tools.length})
          </button>
        </div>
      </header>

      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {tab === "servers" && (
          <>
            {servers.length === 0 && (
              <p className="text-sm text-gray-500 text-center mt-8">
                No MCP servers configured. Edit ~/.small-rust-hermes/mcp.json to add servers.
              </p>
            )}
            {servers.map((srv) => (
              <div
                key={srv.name}
                className="p-3 rounded-lg border border-gray-200 dark:border-gray-700 space-y-1"
              >
                <div className="flex items-center gap-2">
                  <Server size={14} className="text-gray-400" />
                  <span className="font-medium text-sm">{srv.name}</span>
                  <span className="text-xs px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-700">
                    {srv.kind}
                  </span>
                </div>
                <p className="text-xs text-gray-500 font-mono truncate">{srv.detail}</p>
              </div>
            ))}
          </>
        )}

        {tab === "tools" && (
          <>
            {tools.length === 0 && (
              <p className="text-sm text-gray-500 text-center mt-8">No tools available.</p>
            )}
            {tools.map((tool) => (
              <div
                key={tool.name}
                className="p-3 rounded-lg border border-gray-200 dark:border-gray-700 space-y-1"
              >
                <div className="flex items-center gap-2">
                  <Wrench size={14} className="text-gray-400" />
                  <span className="font-medium text-sm font-mono">{tool.name}</span>
                </div>
                <p className="text-xs text-gray-500">{tool.description}</p>
              </div>
            ))}
          </>
        )}
      </div>
    </div>
  );
}
