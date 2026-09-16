# Documentation

- Use [README.md](README.md) as the shared entry point. Add new public documents
  to it, or to the audits index it links. Do not create a second agent-only index.
- `development/` describes current contributor workflows; `rules/` and
  `security/` describe invariants; `tests/` records constraints and evidence.
  `todo/` holds proposals, `archive/` holds completed specs and past plans.
- Keep operational instructions grounded in existing files and commands.
  Link to the source of truth instead of copying catalogs or version lists.
- For audits, record the reviewed date, code baseline, and limits of verification.
  A dated test report is not proof that today's implementation passes.
- Archive a plan only after its delivery is established; update incoming links
  in the same change. Mixed or partially implemented plans stay explicitly plans.
- Use relative Markdown links for navigation. `doc/private/` is ignored and must
  not be required by public docs. Keep local task notes out of tracked docs.
- Write sober prose: no emoji or bold headings, marketing superlatives, export
  artifacts, or text addressed to an agent. Explain the non-obvious details.
- Run `pnpm docs:check` for documentation changes. It checks local link targets
  in maintained contributor docs, index coverage, instruction imports/size,
  and first-party SPDX headers; it does not validate URLs or Markdown anchors.
