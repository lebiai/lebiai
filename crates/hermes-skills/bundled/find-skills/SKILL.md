---
name: find-skills
description: Find and install an existing skill from the open agent ecosystem (skills.sh). Use this skill whenever the user asks "is there a skill for X", "find a skill for X", "install a skill that does Y", or mentions wishing they had help with a domain (design / testing / deployment / etc.). Walks through searching skills.sh, vetting candidates by quality, presenting the SKILL.md for review, and calling skill_install on confirmation — which fetches the full directory (SKILL.md + scripts/ + references/ + assets/) not just one file.
always_active: false
---

# Finding and Installing Skills

_Adapted from [vercel-labs/skills](https://github.com/vercel-labs/skills) — see upstream for the npx-CLI version._

This skill helps you discover and install skills from the open Agent Skills ecosystem at [skills.sh](https://skills.sh). The local install path is `skill_install` (multi-file, transactional). The remote catalogue is GitHub-hosted; most popular repos are `vercel-labs/skills`, `anthropics/skills`, and `ComposioHQ/awesome-claude-skills`.

## When to use

Trigger this skill when the user:
- Asks "is there a skill for X" / "find a skill for Y"
- Asks "can you do X" where X is a specialized capability (PR review, React optimization, deployment checks, design feedback)
- Mentions they wish they had help with a specific domain
- Wants to extend the agent with a packaged workflow

## Workflow

### Step 1: Understand the need

Identify (briefly):
1. **Domain** — React, testing, deployment, design, docs, ...
2. **Specific task** — "review PRs for security", "optimize useEffect", "generate changelogs"
3. **Whether it's a common task** — if so, an existing skill probably exists; if not, suggest creating one (see the `skill-creator` skill).

### Step 2: Search skills.sh

Use the `web_fetch` tool to load the skills.sh leaderboard or search page. Good starting URLs:

- `https://skills.sh/` — the leaderboard, ranked by install count
- `https://skills.sh/?q=<query>` — keyword search (URL-encode the query)

Examples:
- User asks "make my React app faster" → `web_fetch("https://skills.sh/?q=react+performance", "list candidate skills with name, source repo, and install count")`
- User asks "PR review skill" → `web_fetch("https://skills.sh/?q=pr+review", ...)`

### Step 3: Vet candidates

**Do not install based on the search blurb alone.** For each promising candidate:

1. **Install count** — Prefer ≥ 1 K installs. Treat anything under 100 with extra scrutiny.
2. **Source reputation** — Official sources (`vercel-labs`, `anthropics`, `microsoft`) carry more weight than unknown authors.
3. **GitHub stars** — If the source repo has < 100 stars and the author is unknown, slow down.
4. **Read the SKILL.md** — `web_fetch("https://raw.githubusercontent.com/<owner>/<repo>/main/skills/<slug>/SKILL.md", "show me the full body and frontmatter")` and present it to the user verbatim. Skills are essentially prompts; the user should see what's about to be added to their agent.

### Step 4: Present to the user

After vetting, summarize for the user:

```
I found a candidate for "<their query>":

  vercel-labs/skills@react-best-practices   (185K installs)
  "Diagnose and fix React rendering issues; recommends memoization patterns."

Here's the full SKILL.md:

  ---
  name: react-best-practices
  description: ...
  ---
  # React Best Practices
  ...

Install it? (yes / no / show me the next candidate)
```

Wait for explicit confirmation. Skills run as part of every future system prompt's discovery index, so the user owns this decision.

### Step 5: Install

Once confirmed, call `skill_install`:

```json
{
  "source": "vercel-labs/skills@react-best-practices"
}
```

This fetches the **entire** skill directory — `SKILL.md` plus any sibling `scripts/`, `references/`, and `assets/` — not just the single SKILL.md. Tell the user what landed (the tool returns a `files_written` list).

For a one-off SKILL.md without siblings, a raw URL works as a degraded fallback:

```json
{
  "source": "https://raw.githubusercontent.com/owner/repo/main/skills/x/SKILL.md"
}
```

But warn the user: scripts/references/assets won't be fetched in URL mode.

Optional flags:
- `overwrite: true` — replace an existing skill with the same name
- `git_ref: "<branch|tag|sha>"` — pin to a specific commit (defaults to `main`)

### Step 6: Confirm and offer next step

After install succeeds, surface the install summary and tell the user how to use the skill (it'll appear in the discovery index on the next turn). If the search returned multiple good candidates, ask whether they want to look at the next one.

## Common categories

| Category | Example queries |
| --- | --- |
| Web frontend | react, nextjs, tailwind, css, typescript |
| Testing | jest, playwright, e2e, unit testing |
| DevOps | docker, kubernetes, ci-cd, deployment |
| Documentation | readme, changelog, api-docs, technical writing |
| Code quality | review, lint, refactor, security |
| Design | ui, ux, accessibility, design-system |

## When nothing matches

If the search comes up empty (or all candidates are low quality):

1. Acknowledge the gap.
2. Offer to help with the task directly using general capabilities.
3. Suggest using the `skill-creator` skill to author one if it's a recurring need.

Example:
> No good skill found for "<query>" on skills.sh. I can still help directly — or, if this is something you do often, the `skill-creator` skill walks through making one in a few steps.
