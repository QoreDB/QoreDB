# Frontend

- Bindings are split under `src/lib/tauri/`; `src/lib/tauri.ts` is their public
  barrel. Follow the relevant binding's desktop/web routing before adding IPC.
  Types shared with Rust are maintained manually; update both sides together.
- UI uses `src/components/ui/` and [design tokens](../doc/rules/DESIGN.md).
  Hooks live in `src/hooks/` and use the `use` prefix.
- Translation setup is `src/i18n.ts`. Update all supported JSON locales for
  new keys; English fallback is not a substitute for translating new UI.
- Driver descriptors are in `src/lib/connection/drivers.ts`; static schema
  capabilities are in `driverCapabilities.ts`. Coordinate with runtime backend
  capabilities using the [driver guide](../doc/development/DRIVERS.md).
- Frontend visibility is not authorization. Keep safety and license enforcement
  in the backend too; see [command changes](../doc/development/COMMANDS.md).
- Vitest currently discovers `src/**/*.test.ts` in a Node environment. It does
  not provide DOM component coverage. For visual changes, exercise the affected
  flow in the app and report any unavailable runtime check.
- Use `pnpm test:ts path/to/affected.test.ts`, `pnpm typecheck`, and targeted
  `pnpm exec biome check <files>`. Avoid autofixing unrelated files.
