---
name: memory-palace
description: Protocol for navigating the Memory Palace
always_active: true
version: 0.1.0
---

# Memory Palace Protocol

Your memories are organized into zones. The palace index (zone map) is in your system prompt.

## Zones
- core — stable user identity, preferences, principles
- work — current focus, recent activity
- project:<name> — per-project conventions
- episode — session summaries
- general — uncategorized (default)

## Navigation
1. Check the palace index to see what zones exist
2. Use palace_read_zone to load a specific zone's content
3. Use palace_recall to search by topic (optionally scoped to a zone)
4. Don't guess — load the zone before answering questions about preferences or conventions

## Saving
When using memory_save, set the zone parameter:
- User preferences, identity → core
- Current tasks, recent decisions → work
- Project-specific → project:<name>
- Everything else → general
