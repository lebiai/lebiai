---
name: skill-creator
description: Create new skills, including multi-file skills with scripts/ references/ assets/ bundles, modify and improve existing skills, and measure skill performance via real subagent evals. Use whenever the user wants to author or draft a new skill, turn a recurring workflow into a reusable skill, asks "how do I make a skill", or wants to optimize an existing skill's description or triggering. Walks intent capture, drafting, multi-file create via `skill_create(extra_files=...)`, spawning `subagent` evals, grading with the bundled grader, blind A/B comparison, and iterating to convergence.
always_active: false
---

# Skill Creator

_Adapted from [anthropics/skills/skill-creator](https://github.com/anthropics/skills/tree/main/skills/skill-creator) (Apache 2.0 — see `LICENSE.txt`). The runtime primitives are rewired for Hermes:_

- _`subagent` tool replaces upstream's `claude -p` for spawning fresh child contexts (eval runs, graders, comparators)._
- _`skill_create(extra_files=...)` replaces upstream's directory-write packaging — one tool call writes the whole multi-file bundle atomically._
- _`skill_read_file` replaces direct disk reads — bundled references load on demand (Progressive Disclosure level 3)._
- _A markdown `review.md` in the workspace replaces upstream's eval-viewer HTML browser — the user reads it inline and tells you their notes._

This skill teaches you to author, evaluate, and iterate skills end-to-end. A "skill" is a chunk of expertise that future-you (or any other agent) loads on demand when a similar task shows up. It lives at `~/.small-rust-hermes/skills/<name>/SKILL.md`, optionally with sibling `scripts/` / `references/` / `assets/` / `agents/` directories. Discovery indexes only `name` + `description`; the body is read only when triggered, and bundled files are read only when the body asks for them.

Your job in one sentence: turn the user's recurring workflow into a SKILL.md (and supporting files, when useful), then verify with real evals that the next agent does the right thing when shown this skill.

---

## Creating a skill

The high-level loop:

1. **Capture intent** — four questions, terse, before drafting anything.
2. **Interview and research** — pull facts you need; spawn `subagent` for parallel web research when scope is wide.
3. **Draft SKILL.md** — frontmatter + body, imperative voice, pushy description.
4. **Decide multi-file vs single-file** — bundle scripts/references/assets only when content can't sensibly live inline.
5. **Call `skill_create`** — once, with everything (single tool call writes the whole bundle).
6. **Write test cases** — `<skill-name>-workspace/evals.json`.
7. **Run evals** — spawn `subagent` with-skill AND baseline in the same turn.
8. **Grade and review** — produce `review.md`; ask the user for notes.
9. **Improve** — iterate description/body based on failures; loop until convergence.

### Capture intent

Before writing anything, ask exactly these four questions (in this order, terse):

1. **What capability does this skill enable?** (Concrete: "review PRs for SQL-injection patterns" — not "be helpful with PRs".)
2. **When should it activate?** What user phrasing, code pattern, file type, or task shape triggers it? This is the source for the `description` field.
3. **What's the output / artifact?** A reviewed diff, a written test, a generated config, a checklist with results.
4. **Are there supporting files?** Long style guide? Bash helper? Template? Fixture? If everything fits in ~100–200 lines of body, stay single-file.

If any answer is fuzzy, push back before drafting. Skipping this step is the #1 cause of skills that never trigger.

### Interview and research

If the domain has stable best practices you don't already know (a framework's API, a tool's flags, an industry rubric), gather them. Two paths:

- **Cheap** — `web_fetch` one or two canonical URLs the user names ("here's the style guide", "here's the upstream rubric").
- **Parallel** — spawn a `subagent` with `goal: "Research <topic>. Write findings to <skill>-workspace/research.md."` Then read that file. This is the right move when you need 3+ sources, because each `subagent` runs in a fresh context and won't pollute yours with raw HTML.

Don't research what the user has already told you. Don't research things the model already knows from training (basic syntax, well-known APIs).

### Write the SKILL.md

#### Frontmatter

```yaml
---
name: kebab-case-name
description: One- or two-sentence pitch with concrete triggers. See pushy-description rules below.
always_active: false
---
```

Defaults:
- `always_active: false` — almost always. `true` forces the body into every system prompt; reserve that for protocols every turn needs (the bundled `memory-palace` skill is the only example so far).
- No `version` / `license` needed at create time. They're optional fields you can add later.

#### Pushy description

The `description` is the *only* text discovery sees. Vague → never triggers, even when it should. Write it like a recommendation pitch:

**Weak:** _"Helps with React performance."_

**Strong:** _"Diagnose and fix React rendering bottlenecks: identify unnecessary re-renders, recommend memoization (useMemo / useCallback / React.memo), and suggest profiler-driven optimizations. Use whenever the user mentions a slow React app, lag on interaction, large list rendering, or asks 'why does this component re-render'."_

Rules of thumb:
- Mention **concrete trigger phrases** ("when the user says X", "for tasks involving Y", "when reviewing diffs that touch Z").
- Don't be modest. If the skill would help, say so.
- Two sentences is fine when the trigger surface is broad.
- Avoid pure adjectives ("Comprehensive", "Powerful") — discovery weights nouns and verbs.

#### Skill writing guide

**Anatomy** — three sections, in order:

```markdown
# Skill Title

Brief framing of when this skill applies (1–2 sentences).

## Step 1: <imperative action>
Concrete instructions. Explain *why* a non-obvious step matters.

## Step 2: <next action>
...

## Common pitfalls
- "X looks like Y but isn't" — call out things future-you will get wrong.

## Optional deep-dives
- For the full style rules, read `references/style.md` via `skill_read_file`.
- To run the formatter, execute `scripts/format.sh` via the `bash` tool.
```

**Progressive Disclosure** is a three-stage model. Internalize it:

1. **Discovery** — Discovery index holds only `name` + `description`. Cheap. Always loaded.
2. **Activation** — When triggered, the full SKILL.md body is loaded (via `skill_read`). Medium cost.
3. **Execution** — Bundled files (`scripts/`, `references/`, `agents/`, `assets/`) are loaded only when the body explicitly asks the agent to read them, via `skill_read_file`. Cheap per-call, but only what's needed lands in context.

**Principle of lack of surprise**: a skill should do exactly what its description promised. If the description says "diagnose React perf" and the body also tries to write CSS, fix that — either narrow the description or split the skill.

**Writing patterns**:
- Imperative voice ("Run `cargo check` after edits.") — not passive, not subjunctive.
- One rule per bullet. Add a short *why* if it isn't obvious. Don't carpet-bomb readers with ALL-CAPS `ALWAYS` / `NEVER` — those become noise.
- Examples beat abstractions. A 3-line example of the right shape teaches faster than a paragraph describing it.
- Keep section depth ≤ 3 (`#` / `##` / `###`). Deep nesting hides content.

**Writing style**:
- Short sentences. The reader is an agent skimming under time pressure.
- Don't repeat instructions across sections; link with "see §X".
- Code blocks fenced with language tags so editors render them right.
- No emojis unless they're part of the artifact being produced.

### Multi-file skills

Bundle additional files when at least one of these is true:

- The full text would balloon SKILL.md past ~300 lines.
- The content is a **verbatim artifact** (template, fixture, prompt) the agent should reproduce or pass through unchanged.
- The content is **procedural code** the agent should *execute*, not paraphrase.
- The content is a **subagent prompt** (grader, comparator) that should run in a fresh child context.

Directory roles:

| Folder | What goes there | How agent uses it |
|---|---|---|
| `scripts/` | Shell / Python helpers | Invoked via the `bash` tool when the body says so |
| `references/` | Long-form docs | `skill_read_file` when the body links to them |
| `assets/` | Templates, fixtures, examples | `skill_read_file`; sometimes copied verbatim into output |
| `agents/` | Subagent prompts (grader, comparator) | `skill_read_file` to load, then passed as `subagent.goal` |

**Don't bundle when:**
- The whole skill fits in 100 lines. Keep it single-file.
- The "support file" is just another section of the same workflow. Inline it.

When you bundle, make the SKILL.md body explicitly point to each file ("Need the full ruleset? Read `references/rules.md`."). Otherwise future-you won't know it exists.

### Call `skill_create` — once

Single-file:

```json
{
  "name": "diff-reviewer",
  "description": "Review unified diffs for security regressions...",
  "body": "# Diff Reviewer\n\n## Step 1\n...",
  "always_active": false
}
```

Multi-file (one call, transactional — either the whole bundle lands or none of it does):

```json
{
  "name": "shell-style-review",
  "description": "Review shell scripts for portability and style violations. Triggers on 'review this script', 'check my bash', or any shell-script-related PR review.",
  "body": "# Shell Style Review\n\n## Step 1\nRun `scripts/format.sh` against the file via the `bash` tool.\n\n## Step 2\nFor each rule violation, consult `references/style-guide.md` via `skill_read_file`.\n",
  "extra_files": [
    { "rel_path": "references/style-guide.md", "content": "# Shell Style Guide\n..." },
    { "rel_path": "scripts/format.sh",         "content": "#!/bin/sh\nset -eu\nshfmt -d \"$1\"\n" }
  ],
  "always_active": false
}
```

Constraints enforced by the tool (rejected with a clear error if exceeded):
- Up to 49 extra files (50 total including SKILL.md).
- 100 KB per file, 5 MB total.
- Paths must be relative; no `..`, no absolute, no hidden segments, depth ≤ 6.
- `SKILL.md` / `Skill.md` / `skill.md` are not allowed as `rel_path` — that's what `body` is for.

If `skill_create` reports the skill already exists, confirm with the user before passing `overwrite: true`.

---

## Test cases

Skills are easy to write, hard to write *well*. Test cases are the difference. Save them to `<skill-name>-workspace/evals.json` (relative to the workspace root) before running anything:

```json
{
  "skill_name": "diff-reviewer",
  "evals": [
    {
      "name": "sql-injection-in-php",
      "prompt": "Review this diff:\n```diff\n+ $sql = \"SELECT * FROM users WHERE id = $id\";\n```",
      "expected": [
        "Identifies the unsanitized $id interpolation",
        "Recommends parameterized query / prepared statement",
        "Cites the SQL injection class explicitly"
      ]
    },
    { "name": "...", "prompt": "...", "expected": ["..."] }
  ]
}
```

Aim for 3–6 cases:
- 1–2 **golden path** cases — the obvious trigger.
- 1–2 **near-miss** cases — looks similar but the skill should NOT activate, or should activate differently.
- 1–2 **edge** cases — adversarial / ambiguous inputs the skill must handle gracefully.

The `expected` field is bullet-form rubric. The grader (`agents/grader.md`) compares model output against this list and flags any unmet bullet as a failure.

See `references/schemas.md` for the full schema (eval entry shape, grader output shape, comparator output shape, workspace layout).

---

## Running and evaluating test cases

Four steps, parallelized where possible. The whole loop should take one wall-clock turn for ≤6 cases.

### Step 1: Spawn all runs in the same turn

For each eval, spawn **two** `subagent` calls in parallel:

- **With-skill** — agent gets the SKILL.md you just drafted, runs the eval prompt.
- **Baseline** — agent does NOT get the SKILL.md, runs the same eval prompt. (This is your control; lets you tell skill-driven wins from model-baseline wins.)

Call shape:

```json
{
  "tool": "subagent",
  "args": {
    "goal": "<eval prompt>\n\nSave your final answer to <skill>-workspace/iteration-1/<eval-name>/with-skill.md (or .../baseline.md for the baseline run).",
    "extra_skills": ["<skill-name>"],
    "deny_skill_discovery": true
  }
}
```

For the baseline run, omit `extra_skills`. Keep `deny_skill_discovery: true` on both — that keeps the comparison fair and stops baseline from accidentally triggering some other installed skill.

Issue every `subagent` call in the **same turn**. Hermes runs them concurrently up to the runtime cap; sequential calls waste wall time and double your tokens.

`subagent` returns `tokens_in`, `tokens_out`, `duration_ms` in its result — capture these for the timing table in step 3.

### Step 2: Draft assertions while runs are in progress

While `subagent` calls execute (you'll see their results stream back), draft per-eval assertion sketches from the `expected` bullets. You can paste these into the grader prompt in step 4 — it saves you one round-trip per eval.

### Step 3: Capture timing data

For each eval, record `{ with_skill: { tokens_in, tokens_out, duration_ms }, baseline: { ... } }`. The grader's "ROI" judgment uses this — if the skill costs 4× tokens for marginal quality lift, that's a signal the body is bloated.

### Step 4: Grade, aggregate, present

For each eval, spawn one more `subagent` as the grader. Load `agents/grader.md` via `skill_read_file("skill-creator", "agents/grader.md")` and pass its body as the grader's system prompt, with this user input:

```text
Skill name: <name>
Eval: <eval-name>
Eval prompt: <verbatim>
Expected (rubric):
- <bullet 1>
- <bullet 2>

With-skill output:
<verbatim contents of with-skill.md>

Baseline output:
<verbatim contents of baseline.md>

Timing:
  with-skill: <tokens_in> in / <tokens_out> out / <duration_ms> ms
  baseline:   <tokens_in> in / <tokens_out> out / <duration_ms> ms

Grade per the rubric in your system prompt. Save your verdict to <skill>-workspace/iteration-1/<eval-name>/grade.md.
```

Run all graders in parallel — they're independent.

When all grades are back, aggregate. Write `<skill>-workspace/iteration-1/review.md`:

```markdown
# Iteration 1 — <skill-name>

## Summary
- Passed: X/Y
- Token overhead (skill vs baseline): +Z%
- Common failure modes: ...

## Per-eval

### sql-injection-in-php — PASS / FAIL
- Met: bullet 1, bullet 2
- Missed: bullet 3 (grader notes: ...)
- With-skill: 1200 in / 340 out / 4500 ms
- Baseline:   800 in / 290 out / 3200 ms
- Verdict: ...
```

This `review.md` **replaces upstream's eval-viewer HTML** — it's plain markdown the user reads in their editor. In chat surfaces, you can also paste the contents inline.

### Step 5: Read the feedback

Read `review.md` yourself. Then ask the user: "Here's iteration 1. Any notes before I revise?" Capture their notes verbatim into your context — they often catch failure modes the grader missed.

---

## Improving the skill

Four principles (in priority order):

1. **Fix description failures first.** If a near-miss eval triggered the skill when it shouldn't, or a golden-path eval didn't trigger, the description is wrong — that's a discovery-stage bug, and no body changes can fix it. Tighten or broaden trigger phrases.
2. **Make the body answer the actual failure.** Don't add general advice; add the specific instruction that would have changed the failing output. ("Always cite the CWE class" beats "be thorough".)
3. **Don't bloat to fix a corner case.** If one rare eval keeps failing and fixing it doubles the body, consider whether that case belongs in the skill at all, or in a separate one.
4. **Re-run after every change.** A change that fixed eval A often regresses eval B. Don't trust intuition — run the loop again.

Iteration loop:

```
draft v1 → eval → review.md → user notes → revise → eval again → ... → converged
```

"Converged" means: all golden-path evals pass, all near-miss evals correctly decline or differentiate, edge cases handled or explicitly declared out of scope.

Re-running uses the same `skill_create` call with `overwrite: true`, then the same eval spawn pattern with workspace path bumped to `iteration-2/`, etc.

---

## Advanced: Blind comparison

When iteration N+1 is supposed to be better than N but the grader says they look similar, run a **blind A/B comparator**.

Steps:

1. Read `agents/comparator.md` via `skill_read_file("skill-creator", "agents/comparator.md")`. This is the comparator's system prompt.
2. For each eval, gather `iteration-N/<eval>/with-skill.md` and `iteration-N+1/<eval>/with-skill.md`. **Randomize which is A and which is B per-eval** — record the mapping yourself, don't tell the comparator.
3. Spawn a `subagent` per eval with the comparator system prompt and a user message:
   ```
   Eval: <name>
   Eval prompt: <verbatim>
   Expected (rubric): <bullets>

   Output A:
   <verbatim>

   Output B:
   <verbatim>

   Which output better satisfies the rubric? Save your verdict (A / B / tie + reason) to <skill>-workspace/iteration-N+1/<eval>/compare.md.
   ```
4. When all verdicts are in, un-blind using your mapping. Tally wins.
5. Read `agents/analyzer.md` via `skill_read_file("skill-creator", "agents/analyzer.md")`. Spawn one final `subagent` with the analyzer system prompt and pass it all the compare.md verdicts + the per-eval rubrics. It outputs a synthesis: what consistently improved, what regressed, where the new version is genuinely better.
6. Append the synthesis to `review.md`.

Comparator + analyzer are heavyweight — only invoke them when grader scores are too close to tell improvements apart by eyeball.

---

## Description optimization

The body can be perfect and the skill still fail if `description` mis-triggers. To optimize the description specifically:

### Step 1: Split your eval set

- **Train set** (~60% of evals) — you may inspect, iterate on, regrade.
- **Held-out set** (~40%) — you DO NOT look at during iteration. Used once at the end to validate the final description doesn't overfit.

### Step 2: Build a trigger corpus

Collect 20–40 short user prompts (one line each). Mix:
- ~50% prompts that **should** trigger this skill.
- ~30% prompts that should trigger a *different* skill or no skill.
- ~20% adversarial near-misses (similar vocabulary, different intent).

Label each with the expected trigger decision. Save to `<skill>-workspace/trigger-corpus.json`.

### Step 3: Run the trigger eval via `subagent`

For each prompt, spawn a `subagent` with `goal: "<user prompt>"` and `extra_skills: ["<skill-name>"]` (plus all other installed skills, so discovery has a real surface to choose from). Capture whether the skill activated (the subagent's first turn will either call `skill_read` for your skill or not).

Score: precision (of activations, % correct) and recall (of should-activate prompts, % that activated).

### Step 4: Revise the description and re-run

Common fixes:
- **Low recall** → description is too narrow. Add trigger phrases, broader nouns.
- **Low precision** → description is too eager. Add "don't trigger on X" guidance, narrow nouns.

Iterate on the train set only. Run the held-out set once at the end. If held-out scores are >10% worse than train, you've overfit — back off and broaden.

---

## Reference files

Bundled with this skill (read via `skill_read_file("skill-creator", "<path>")`):

- `agents/grader.md` — system prompt for grading a single eval against its rubric.
- `agents/comparator.md` — system prompt for blind A/B comparison of two outputs.
- `agents/analyzer.md` — system prompt for synthesizing a multi-eval comparator run into a "what's actually better" summary.
- `references/schemas.md` — JSON schemas for `evals.json`, grader output, comparator output, and the workspace layout.
- `LICENSE.txt` — Apache 2.0 license from upstream anthropics/skills (governs the verbatim files above).
