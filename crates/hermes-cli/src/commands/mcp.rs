//! `hermes mcp ...` — inspect MCP configuration and connectivity.

use anyhow::Result;
use hermes_mcp::{McpConfig, McpToolHost, ServerSpec};

pub async fn list() -> Result<()> {
    let cfg = McpConfig::load_default()?;
    if cfg.servers.is_empty() {
        println!("(no MCP servers configured in ~/.small-rust-hermes/mcp.json)");
        return Ok(());
    }
    let mut names: Vec<&String> = cfg.servers.keys().collect();
    names.sort();
    for name in names {
        let spec = &cfg.servers[name];
        match spec {
            ServerSpec::Stdio { command, args, .. } => {
                println!("{name}  stdio: {} {}", command, args.join(" "));
            }
            ServerSpec::Http { url, .. } => {
                println!("{name}  http:  {url}");
            }
        }
    }
    Ok(())
}

pub async fn test(target: Option<String>) -> Result<()> {
    let cfg = McpConfig::load_default()?;
    let filtered = match &target {
        Some(name) => {
            let mut sub = McpConfig::default();
            let spec = cfg
                .servers
                .get(name)
                .ok_or_else(|| anyhow::anyhow!("no server named {name:?}"))?;
            sub.servers.insert(name.clone(), spec.clone());
            sub
        }
        None => cfg,
    };
    if filtered.servers.is_empty() {
        println!("(no servers to test)");
        return Ok(());
    }

    let host = McpToolHost::connect_all(&filtered)
        .await
        .map_err(|e| anyhow::anyhow!("connect: {e}"))?;
    let tools = host
        .list_tools()
        .await
        .map_err(|e| anyhow::anyhow!("list_tools: {e}"))?;

    if tools.is_empty() {
        println!(
            "(connected but no tools were advertised — {} server(s) tried)",
            host.server_count()
        );
        return Ok(());
    }
    for tool in tools {
        let desc_line = tool.description.lines().next().unwrap_or("");
        println!("{}  — {desc_line}", tool.name);
    }
    Ok(())
}

// ToolHost trait usage, pull it into scope for the list_tools() call.
use hermes_core::ToolHost;
