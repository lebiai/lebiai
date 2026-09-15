---
name: memory-palace
description: Protocol for navigating lasting notes — organised by subject, not by slot
always_active: true
version: 0.3.0
---

# Memory Palace Protocol

Lasting notes are **living notes**: several complementary sides of one subject can live together. Only a note that truly replaces another supersedes it. They are notes that can be wrong. Do not invent a past you were not given.

## Zones (only these four)

- **preferences** — how they like to work; identity they confirmed
- **standards** — quality bar for a kind of work
- **work** — reusable work episodes (情境 / 做法 / 产出 / 反馈 / 可复用点)
- **general** — does not fit the three above

Old names you may still see on disk: `core` → preferences, `episode` → work, `project:<name>` → work. When you read or write, use the four names above.

## Navigation

1. If topic cards (主题卡) are in the system prompt, they say **which subjects** you have notes on — open the notes themselves before quoting a rule; the wording of the note is what binds
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
