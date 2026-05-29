# Audit — Ordre des checks de capability dans les host functions plugins

**Date** : 2026-05-24
**Périmètre** : `src-tauri/src/plugins/runtime/host_fns.rs`
**Critère** : la vérification de capability doit être la **toute première instruction** de chaque host function exposée au runtime WASM. Aucun side-effect (lecture mémoire guest, allocation, accès au système de fichiers, requête réseau, lecture keyring) ne doit précéder le check.

## Pourquoi

Un plugin malveillant qui n'a pas obtenu une capability ne doit jamais déclencher la moindre opération coûteuse ou observable côté host. Si la lecture des arguments précédait le check, un plugin pourrait :

- Asker le host à allouer de la mémoire dans le guest avant d'être refusé (DoS).
- Exposer la latence du backing store (timing oracle sur la présence d'un secret).
- Toucher le disque ou le réseau « pour rien » avant d'être recalé.

Le check capability d'abord est la **garantie minimale** que `ERR_DENIED` se prend en O(1) et sans effet observable autre que la consignation tracing.

## Résultat de l'audit

| Host fn | Capability | Première instruction ? | Note |
| --- | --- | --- | --- |
| `qoredb_log` | `Log` | ✅ `has_capability(&caller, CapabilityKind::Log)` | OK |
| `qoredb_notify` | `Notify` | ✅ `has_capability(&caller, CapabilityKind::Notify)` | OK |
| `qoredb_kv_get` | `Storage` | ✅ `has_capability(&caller, CapabilityKind::Storage)` | OK |
| `qoredb_kv_set` | `Storage` | ✅ `has_capability(&caller, CapabilityKind::Storage)` | OK |
| `qoredb_kv_del` | `Storage` | ✅ `has_capability(&caller, CapabilityKind::Storage)` | OK |
| `qoredb_http_request` | `Http` | ✅ `has_capability(&caller, CapabilityKind::Http)` | OK |
| `qoredb_fs_read` | `Fs` | ✅ `has_capability(&caller, CapabilityKind::Fs)` | OK |
| `qoredb_fs_write` | `Fs` | ✅ `has_capability(&caller, CapabilityKind::Fs)` | OK |
| `qoredb_fs_delete` | `Fs` | ✅ `has_capability(&caller, CapabilityKind::Fs)` | OK |
| `qoredb_secret_get` | `Secrets` | ✅ `has_capability(&caller, CapabilityKind::Secrets)` | OK |
| `qoredb_query_read` | `QueryRead` | ✅ `has_capability(&caller, CapabilityKind::QueryRead)` | OK |

**Conclusion** : toutes les host functions respectent l'invariant.

## Helper centralisé

Depuis Phase 4 (S4), un helper unique `has_capability(&Caller, CapabilityKind) -> bool` consigne tout refus via `tracing::warn!` :

```rust
fn has_capability(caller: &Caller<'_, StoreData>, kind: CapabilityKind) -> bool {
    if caller.data().services.consent.contains(&kind) {
        return true;
    }
    tracing::warn!(
        target: "plugins",
        plugin = %caller.data().services.plugin_id,
        capability = ?kind,
        "plugin attempted to use a capability it was not granted"
    );
    false
}
```

Chaque host fn appelle `if !has_capability(&caller, KIND) { return DENIAL; }` comme **première** ligne de son closure body. Cette uniformité rend toute violation future visible au diff review : une instruction supplémentaire avant ce `if` doit déclencher la question « pourquoi avant le check ? ».

## Refus secondaires (defence in depth)

Plusieurs host fns ont un **second** filtre après le check de capability, lui aussi gardé : URL → allowlist d'hôtes (`qoredb_http_request`), nom de secret → liste déclarée (`qoredb_secret_get`), chemin → scope plugin-data (`scoped_fs_path`). Ces filtres ne sont **pas** des checks de capability : ils valident les arguments d'un appel autorisé. Ils peuvent légitimement faire un peu de travail (parse URL, allocation locale) avant de refuser — leur ordre est dicté par la sémantique, pas par la sécurité.

## Tests

L'invariant « refus en O(1) sans side-effect » est couvert indirectement par la suite E2E (`src-tauri/tests/plugins_e2e.rs`) :

- `storage_capability_denied_drops_the_write` : un plugin qui appelle `qoredb_kv_set` sans la capability ne touche pas le disque.
- `http_request_to_unallowed_host_is_rejected_before_the_network` : l'allowlist d'hôtes refuse avant tout fetch reqwest.
- `fs_read_outside_the_scoped_root_is_rejected` : le filtre de scope tient même quand la capability `Fs` est accordée.

Toute régression sur l'ordre des checks ferait sauter ces tests (le fichier de stockage apparaîtrait à tort, ou un appel réseau hangerait).
