# Agent and contributor readiness

- Reviewed: 2026-09-16.
- Code baseline: `99f2222`, plus the documentation/tooling changes accompanying
  this audit. Pre-existing deletions under `aur/` were outside this review.
- Scope: repository instructions, contributor navigation, extension guides,
  lightweight validation, and official guidance for Opus 5, Fable 5, and Astra.
- Limit: no comparative agent benchmark, full product audit, or live database run.

## Findings addressed

| Finding at the baseline | Change |
| --- | --- |
| `AGENTS.md` and `CLAUDE.md` duplicated almost all content but disagreed on Premium scope | One shared root plus scoped instructions; Claude files only import their sibling |
| Startup instructions contained stale versions, four databases, old paths, and an extensive file inventory | Short task routing; versions in manifests, architecture in guides, licensing inventory from headers |
| The documentation index was a nine-line directory list, including nonexistent `internals/` | A task-oriented index, public document coverage check, explicit guide/plan/evidence/archive roles |
| Contribution instructions pointed driver work at `src-tauri/src/engine/drivers/` | A driver guide grounded in workspace crates, including feature forwarding and all entry points |
| No complete command/IPC extension path | A command guide covering registration, types, web transport, policy and feature gates |
| `pnpm test` was documented as Rust-only; workspace test coverage was easy to overestimate | Correct script description and package/feature testing matrix |
| Contributor i18n instructions allowed English-only additions, contradicting repository rules | All supported locales consistently required; correct `src/i18n.ts` path |
| Frontend CI ran Biome only | Typecheck, Vitest, repository checker, and checker regression tests added to the same job |
| Two first-party code files lacked the required SPDX first line | Headers added to Vite config and service build script; automated presence/spelling check |
| Private documents were referenced as essential project context | Public workflows stand alone; local/private notes are explicitly non-portable |

## Instruction size

Measured as UTF-8 bytes, not tokenizer output:

| File loaded at root | Before | After |
| --- | ---: | ---: |
| `AGENTS.md` | 11,064 | 4,972 |
| `CLAUDE.md` plus its import | 11,860 | 4,983 |

Root shared instructions shrink by about 55%. Scoped instructions add 1.3–1.6 KiB
when relevant. This is a reduction in instruction text; total task tokens, latency,
and accuracy have not been benchmarked. Both old root files were not necessarily
loaded by the same client, so their sizes should not be summed as a per-run cost.
The [agent workflow](../development/AGENT_WORKFLOW.md) specifies a comparison
protocol and the official model/client sources behind these choices.

## Verification performed

- `pnpm typecheck`: passed before and after the changes.
- `pnpm test:ts`: 27 files / 187 tests passed before and after the changes.
- `pnpm test:ts src/lib/connection/drivers.test.ts`: one file / two tests passed;
  the extra `--` form was observed to run the entire suite and removed from guides.
- `pnpm test:repo`: seven checker regression tests passed.
- `pnpm docs:check`: passed for 19 maintained documents and first-party headers.
- `pnpm exec biome check package.json vite.config.ts` and `git diff --check`: passed.
- Cargo metadata confirmed package targets, default workspace selection, and
  feature defaults. Rust tests, native app builds, and live database tests were
  not run for these documentation/tooling changes.

## Verification boundaries

The accompanying checks validate imports and size budgets, navigation targets in
maintained contributor documents, public documentation index coverage, and
first-party TypeScript/Rust header presence. They use Git-visible files, including
untracked non-ignored additions, so an ignored local file cannot hide a broken
public link. Vendor sources are outside header enforcement.

The checker does not validate external URLs, Markdown fragments, all links inside
historical audits/specifications, translation quality, or the legal correctness
of a chosen license. Guides still require technical review when their code changes.

## Remaining work, by observed limitation

| Priority | Limitation | Next bounded action |
| --- | --- | --- |
| High | Backend CI selects the desktop package; it does not run every workspace member's own unit tests | Add package/feature jobs for shared crates with known native requirements and an explicit Core/Pro matrix; establish their baseline first |
| Medium | `todo/` contains partially delivered and overlapping plans | Reconcile each plan with code and acceptance evidence, then archive completed material with incoming links updated |
| Medium | Vitest currently covers Node utilities, not component rendering or desktop interactions | Choose a representative UI regression and add the smallest runnable interaction test setup for it |
| Medium | Agent efficiency is inferred from reduced context and fewer ambiguous paths | Run the fixed-task comparison described in the agent workflow before making token/latency claims |
| Low | Rust stable floats in CI; no supported minimum Rust version is declared/tested | Decide a toolchain policy and validate it across release platforms before pinning or claiming an MSRV |

No generic skill catalog, extra MCP server, automatic subagent review loop, or
model-specific configuration is introduced. Each would add maintenance and context
cost; adopt one only for a demonstrated workflow with measurable benefit.
