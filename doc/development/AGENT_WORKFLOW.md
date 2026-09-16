# Working efficiently with coding agents

Reviewed against official documentation on 2026-09-16 for Claude Opus 5,
Claude Fable 5, and GPT-6 Astra. Model behavior and client features change;
re-check the linked sources when upgrading. These are repository workflow
choices, not measured claims that one model performs best on QoreDB.

## One instruction source, contextual guides

[AGENTS.md](../../AGENTS.md) contains the common agreement and task routing.
Scoped files cover frontend, Rust, and documentation. Each `CLAUDE.md` contains
only `@AGENTS.md`, an import supported by
[Claude Code](https://code.claude.com/docs/en/memory). Imports still consume
context: importing the whole documentation tree would defeat the purpose.
Guides stay ordinary links, opened for a specific task.

[Codex instruction discovery](https://learn.chatgpt.com/docs/agent-configuration/agents-md)
uses the project path and working directory, with nested instructions taking
precedence. Claude Code has its own on-demand loading behavior. The root guide
therefore explicitly routes agents to scoped files rather than assuming every
client loads them identically. No machine-wide configuration is required.

`pnpm docs:check` keeps the shared imports exact and enforces repository budgets
of 8 KiB for root instructions and 4 KiB for each scoped file. These are maintenance
limits chosen for this project, not model context limits or token measurements.
New instructions should prevent a recurring mistake; move reference material
into a linked guide rather than increasing startup context indefinitely.

## Give a task enough shape

A useful request states the problem, a concrete example, constraints, and the
observable result. For example:

```text
Fix duplicate rows when browsing a PostgreSQL table with non-unique sort values.
Keep the current grid UX and existing offset fallback. Reproduce the duplicate
in a regression test, preserve stable ordering across pages, and report which
checks ran against a real database versus mocks. Stop when the relevant checks
pass and the behavior is covered.
```

The agent should find nearby implementations and tests, follow dependencies when
needed, and make routine decisions inside that scope. Material product or safety
ambiguities deserve clarification; ordinary implementation choices do not.
Keep unrelated discoveries as findings rather than silently widening the patch.

## Model-specific considerations

| Model | Guidance supported by its current documentation | QoreDB application |
| --- | --- | --- |
| [Opus 5](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-opus-5) | Extra self-check instructions can cause redundant verification; delegation and narration benefit from explicit scope. Lower effort can reduce cost where quality holds. | Use the testing matrix and stop condition; avoid a mandatory second review agent. Request concise evidence, not a transcript of every action. |
| [Fable 5](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/prompting-claude-fable-5) | Strong literal instruction following, long runs, and memory benefit from clear boundaries and progress grounded in tool results. Routine work at high effort may over-explore. | Keep one active scope; save confirmed decisions and actual test outcomes for handoff. Calibrate effort on representative tasks. |
| [GPT-6 Astra](https://developers.openai.com/api/docs/guides/latest-model) | Instruction files strongly influence behavior; clarification and verification can exceed the needs of small tasks. | Remove conflicting rules, distinguish material ambiguities from reversible decisions, and choose checks by affected package. |

Effort and permissions belong in the client/session configuration. There is no
portable `AGENTS.md` setting that controls a model's reasoning budget. Do not
hardcode a model ID, price, or context-window assumption into project invariants.
Measure outcomes before adopting a model-specific setting as a team default.

## Context, tools, and parallel work

- Search filenames and local symbols before reading large files. Read source,
  callers, and tests for the changed boundary rather than copying entire trees.
- Keep tool output bounded. Preserve the useful failure excerpt and full logs
  locally when necessary; avoid repeatedly loading successful build output.
- Use the [testing matrix](TESTING.md) to avoid desktop builds for a pure engine
  change. Keep caches and dependencies between runs where the environment allows.
- Enable only integrations needed for the task. QoreDB's MCP server is a product
  database interface, not a prerequisite for editing this repository. Do not
  connect production databases merely to inspect source code.
- When parallel agents are authorized and useful, give each an independent scope,
  files it may edit, and a concrete deliverable. Keep concurrency bounded; do not
  create an automatic writer/reviewer loop for every patch. Prefer separate
  worktrees for concurrent writes, and integrate before claiming tests passed.

## Handoffs and memory

For a long task, keep a short local note such as `dev/agent-notes/<task>.md` (`dev/`
is ignored). A useful handoff contains:

```text
Goal and acceptance criteria:
Current branch/commit and relevant working-tree changes:
Confirmed decisions and source paths:
Changes completed:
Checks run, exact outcome, environment and skipped coverage:
Next action / unresolved decision:
```

Record conclusions that would be expensive to rediscover, not a transcript or
copied code. Reconcile a handoff with `git status` and the diff before resuming.
Keep credentials, customer rows, and private prompts out of notes. Durable
architecture decisions belong in the relevant tracked guide, reviewed with the
change. Local memory is not a second source of project rules.

## Measure whether changes help

Use a fixed repository revision and a small repeatable task set: a utility bug,
a UI/i18n change, a driver capability change, and a service safety regression.
Run comparable before/after sessions on isolated checkouts with the same model,
client version, effort, permissions, cache state, and available services.

Record success against acceptance tests, regressions, out-of-scope edits,
human corrections, elapsed time, tool calls, and input/output/cached token usage
when exposed by the client. Repeat runs before drawing conclusions from latency
or token counts. Shorter instruction files are directly measurable; lower total
cost and better correctness require these experiments.

## Maintenance

Keep project facts in code/manifests, operational paths in the guides, and dated
findings in [the readiness audit](../audits/AGENT_READINESS.md). Review the relevant
links and commands when moving code or changing tools. Add a skill or hook only
for a demonstrated repeatable workflow that existing guides and scripts cannot
cover; avoid maintaining parallel instruction systems for each model.
