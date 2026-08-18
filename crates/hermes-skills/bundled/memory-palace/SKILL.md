---
name: memory-palace
description: Protocol for navigating lasting notes — one living rule per kind of work
always_active: true
version: 0.2.0
---

# Memory Palace Protocol

Lasting notes are **living rules**: one active rule per kind of work. They are notes that can be wrong. Do not invent a past you were not given.

## Zones (only these four)

- **preferences** — how they like to work; identity they confirmed
- **standards** — quality bar for a kind of work
- **work** — reusable work episodes (情境 / 做法 / 产出 / 反馈 / 可复用点)
- **general** — does not fit the three above

Old names you may still see on disk: `core` → preferences, `episode` → work, `project:<name>` → work. When you read or write, use the four names above.

## Navigation

1. Check the palace index if it is in the system prompt
2. `palace_read_zone` with one of the four names
3. `palace_recall` / `memory_search` by topic
4. Do not guess — load the zone before asserting a preference or standard
5. If nothing relevant is present, say you do not have a note. Never pretend you remember.

## Saving

Only when the **`memory_save` tool is in your tool list**:

- preferences / identity they confirmed → zone `preferences` + tag `preference`
- quality bar → `standards` + tag `standard`
- finished work pattern → `work` + tag `work-episode`
- otherwise → `general`

If `memory_save` is **not** in your tool list, you cannot keep anything lasting. Do not claim you saved. Tell them to do it in the desktop app.

Do not write session recaps, empty templates, or environment probes.
