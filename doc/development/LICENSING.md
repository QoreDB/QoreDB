# Source licensing

QoreDB uses [Apache-2.0](../../LICENSE) for Core and
[BUSL-1.1](../../LICENSE-BSL) for Premium. This guide describes repository
conventions; the license files contain the terms.

Every first-party `.ts`, `.tsx`, and `.rs` file starts with exactly one of:

```text
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: BUSL-1.1
```

Preserve an existing file's license. A new file is Core unless it belongs to a
Premium module or an explicit licensing decision says otherwise. Helpers and
tests extracted from Premium code retain that license. A move between Core and
Premium requires an explicit decision and a matching header update. Do not infer
a file's license from its Cargo package metadata or whether `pro` is enabled.
Vendored dependencies retain upstream licensing.

## Locate the Premium boundary

This module map is a navigation aid. The current per-file inventory comes from
headers, avoiding a second exhaustive file list that drifts with every rename:

```bash
rg -l '^// SPDX-License-Identifier: BUSL-1.1' src src-tauri --glob '*.ts' --glob '*.tsx' --glob '*.rs' --glob '!vendor/**'
```

| Module | Premium locations / exceptions to inspect |
| --- | --- |
| AI and chat | `src/components/AI/`, `Chat/`, `Settings/sections/AiSection.tsx`, `Brand/QoreAiMark.tsx`, `Notebook/cells/AiCell.tsx`; AI/chat hooks, bindings, preferences, `query/inlineEditDiff*`; `src-tauri/src/ai/` and AI/agent/chat commands |
| Contracts | `src/components/Contracts/`, `src/lib/contracts/`, `src-tauri/src/contracts/`, contracts command |
| Data diff | `src/components/Diff/`, `src/lib/diffUtils.ts` |
| Federation | `src/components/Federation/`, `src/lib/connection/federation.ts`, `qore-service/src/federation/`, federation command |
| Query replay | `src/components/Replay/`, `src/hooks/useReplay.ts`, `src/lib/replay*.ts`, `src-tauri/src/replay/`, replay command and `tests/replay_e2e.rs` |
| Time travel | `src/components/TimeTravel/`, `src-tauri/src/time_travel/`, time-travel command |
| Advanced notebook | `ChartCell.tsx`, `ContractCell.tsx`, `CellResultSummary.tsx`, `notebookInterCellRef.ts` |
| Advanced schema | `src/components/Schema/ERDiagram.tsx` |
| Advanced export | `src-tauri/src/export/writers/parquet_writer.rs`, `xlsx.rs` |
| Profiling and alerts | `qore-service/src/interceptor/{profiling,regression,n_plus_one,alerts}.rs`, `src/providers/InterceptorAlertsProvider.tsx` |
| Index suggestions | `src/lib/query/indexSuggestions.ts`, `src/components/Results/IndexSuggestions.tsx` |
| Schema diff | `src/lib/migrations/{schemaDiff,schemaCompare,baselineStore}.ts`, schemaDiff tests, `SchemaDeltaView.tsx`, `SchemaDiffViewer.tsx`, workspace-baselines command |
| Column masking | `qore-core/src/masking.rs`, `qore-service/src/masking_guard.rs`, `src/lib/masking*`, `MaskingSection.tsx`, `useColumnMasking.tsx` |
| Data generation / AI filter | `src/components/Grid/{DataGeneratorDialog,NaturalLanguageFilterBar}.tsx`, `src/lib/dataGenerator.ts`, data-generator command |
| Instant API | `src/components/InstantApi/`, `src/lib/instantApi/`, `src-tauri/src/api/`, instant-api command |
| Server | Rust sources under `src-tauri/crates/qore-server/src/` |

Component filenames abbreviated above are located under `src/components/`;
`qore-*` paths are under `src-tauri/crates/`. Mixed modules also contain Core
files: check the actual header before assuming a whole parent directory is Premium.
If the header, this map, and intended scope conflict, resolve that decision before
relicensing code. `pnpm docs:check` checks header presence and identifier spelling;
it does not decide whether a licensing choice is correct.
