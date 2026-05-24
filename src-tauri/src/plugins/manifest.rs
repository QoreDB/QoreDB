// SPDX-License-Identifier: Apache-2.0

//! `plugin.json` parsing and validation.

use super::{PluginContributions, PluginManifest, RuntimeSpec};

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Parses raw `plugin.json` content into a validated manifest.
pub fn parse_manifest(raw: &str) -> Result<PluginManifest, String> {
    // Pre-pass: serde would silently drop unknown capability fields, so we
    // walk the raw JSON first to surface the ones we explicitly disallow.
    reject_retired_capabilities(raw)?;
    let manifest: PluginManifest =
        serde_json::from_str(raw).map_err(|e| format!("Invalid plugin.json: {e}"))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Rejects manifests that declare capabilities we used to accept but no
/// longer support. `queryExec` was wired through the type system but never
/// reached the host functions; rather than ship a footgun that looks active,
/// we refuse the manifest with an explicit message.
fn reject_retired_capabilities(raw: &str) -> Result<(), String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        // Malformed JSON — let the typed parse below produce the real error.
        return Ok(());
    };
    let caps = value
        .get("runtime")
        .and_then(|r| r.get("capabilities"));
    if let Some(caps) = caps {
        if caps.get("queryExec").is_some() {
            return Err(
                "Plugin capability 'queryExec' is not supported in this build. \
                 Remove it from runtime.capabilities."
                    .to_string(),
            );
        }
    }
    Ok(())
}

/// Validates a manifest's identifiers and contributions.
///
/// Declarative plugins must not be able to inject anything beyond QoreDB's own
/// design tokens, so theme variables are restricted to the `--q-*` namespace.
pub fn validate_manifest(m: &PluginManifest) -> Result<(), String> {
    if !is_valid_id(&m.id) {
        return Err(format!(
            "Invalid plugin id '{}': use lowercase letters, digits, '.', '-' or '_'",
            m.id
        ));
    }
    if m.name.trim().is_empty() {
        return Err("Plugin name must not be empty".into());
    }
    if m.version.trim().is_empty() {
        return Err("Plugin version must not be empty".into());
    }
    validate_contributions(&m.contributes)?;
    if let Some(runtime) = &m.runtime {
        validate_runtime(runtime)?;
    }
    // A `command` contribution needs the executable runtime to fire the
    // WASM `command` hook — without it the action would do nothing.
    if !m.contributes.commands.is_empty() && m.runtime.is_none() {
        return Err(
            "Command contributions require a 'runtime' block to receive the command hook".into(),
        );
    }
    Ok(())
}

/// ABI version this build of QoreDB speaks. A plugin built against a newer
/// ABI is rejected at validation time rather than misbehaving at run time.
pub const CURRENT_ABI_VERSION: u32 = 1;

/// Validates a plugin's executable-runtime descriptor.
fn validate_runtime(r: &RuntimeSpec) -> Result<(), String> {
    if r.abi_version != CURRENT_ABI_VERSION {
        return Err(format!(
            "Plugin runtime targets ABI version {} but this QoreDB speaks {}",
            r.abi_version, CURRENT_ABI_VERSION
        ));
    }
    let entry = r.entry.trim();
    if entry.is_empty() {
        return Err("Runtime entry must not be empty".into());
    }
    if !entry.ends_with(".wasm") {
        return Err("Runtime entry must be a '.wasm' file".into());
    }
    // The entry is joined to the plugin folder — forbid any path navigation.
    if entry.contains('/') || entry.contains('\\') || entry.contains("..") {
        return Err("Runtime entry must be a bare filename".into());
    }
    if let Some(http) = &r.capabilities.http {
        if http.allowed_hosts.is_empty() {
            return Err("The 'http' capability requires a non-empty allowedHosts list".into());
        }
    }
    if let Some(integrity) = &r.integrity {
        validate_integrity(integrity)?;
    }
    Ok(())
}

/// Accepts the subresource-integrity-style format `sha256-<64 lowercase hex>`.
/// Any other shape is rejected at manifest parse time so a typo turns into a
/// clear error rather than a silent "always fails to load".
fn validate_integrity(value: &str) -> Result<(), String> {
    let Some(hex) = value.strip_prefix("sha256-") else {
        return Err(
            "Runtime integrity must start with 'sha256-' followed by a 64-character hex digest"
                .into(),
        );
    };
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        return Err(
            "Runtime integrity must be 'sha256-' followed by exactly 64 lowercase hex characters"
                .into(),
        );
    }
    Ok(())
}

fn validate_contributions(c: &PluginContributions) -> Result<(), String> {
    for s in &c.snippets {
        if s.id.trim().is_empty() || s.label.trim().is_empty() {
            return Err("Snippet contributions require a non-empty id and label".into());
        }
        if s.template.trim().is_empty() {
            return Err(format!("Snippet '{}' has an empty template", s.id));
        }
    }
    for t in &c.connection_templates {
        if t.id.trim().is_empty() || t.name.trim().is_empty() {
            return Err("Connection templates require a non-empty id and name".into());
        }
        if t.driver.trim().is_empty() {
            return Err(format!("Connection template '{}' has no driver", t.id));
        }
    }
    for theme in &c.themes {
        if theme.id.trim().is_empty() || theme.name.trim().is_empty() {
            return Err("Theme contributions require a non-empty id and name".into());
        }
        for (key, value) in theme.light.iter().chain(theme.dark.iter()) {
            if !key.starts_with("--q-") {
                return Err(format!(
                    "Theme '{}' variable '{}' is not allowed: only '--q-*' design tokens",
                    theme.id, key
                ));
            }
            validate_css_value(&theme.id, key, value)?;
        }
    }
    for viewer in &c.result_viewers {
        if viewer.id.trim().is_empty() {
            return Err("Result viewer contributions require a non-empty id".into());
        }
        // At least one match criterion — a viewer that matches nothing would
        // never fire; one that matches everything would override every cell
        // and is almost certainly a mistake.
        let has_match = viewer.match_on.column_type.as_deref().is_some_and(|s| !s.trim().is_empty())
            || viewer
                .match_on
                .name_pattern
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty());
        if !has_match {
            return Err(format!(
                "Result viewer '{}' must declare a 'columnType' or 'namePattern' match",
                viewer.id
            ));
        }
        // The frontend treats `*` as the only wildcard; reject anything that
        // smells like a regex so we don't accidentally honour an unsafe pattern
        // later.
        if let Some(pat) = viewer.match_on.name_pattern.as_deref() {
            if pat.contains(['/', '\\', '^', '$', '(', ')', '[', ']', '{', '}', '|']) {
                return Err(format!(
                    "Result viewer '{}' name pattern may only contain '*' wildcards",
                    viewer.id
                ));
            }
        }
    }
    for command in &c.commands {
        if command.id.trim().is_empty() {
            return Err("Command contributions require a non-empty id".into());
        }
        if command.label.trim().is_empty() {
            return Err(format!("Command '{}' has an empty label", command.id));
        }
    }
    Ok(())
}

/// CSS fragments a declarative theme value must never contain. A theme only
/// needs colours and sizes; these would let it fetch remote resources
/// (`url(...)`) or pull in external stylesheets, leaking that the app ran.
/// Themes are applied via `setProperty`, so this is defence in depth.
const FORBIDDEN_CSS: [&str; 4] = ["url(", "expression(", "javascript:", "@import"];

/// Maximum length of a theme variable value (a colour or size literal).
const MAX_CSS_VALUE_LEN: usize = 256;

/// Rejects theme values that are over-long or carry active CSS.
fn validate_css_value(theme_id: &str, key: &str, value: &str) -> Result<(), String> {
    if value.len() > MAX_CSS_VALUE_LEN {
        return Err(format!(
            "Theme '{theme_id}' variable '{key}' value is too long (max {MAX_CSS_VALUE_LEN})"
        ));
    }
    let lower = value.to_ascii_lowercase();
    for fragment in FORBIDDEN_CSS {
        if lower.contains(fragment) {
            return Err(format!(
                "Theme '{theme_id}' variable '{key}' contains disallowed CSS ('{fragment}')"
            ));
        }
    }
    Ok(())
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'))
}

/// Best-effort check of a plugin's `qoredb` version requirement against this
/// build. Only `>=X.Y.Z` (or a bare `X.Y.Z`) is understood; anything else is
/// treated as compatible so an unknown syntax never silently disables a plugin.
pub fn is_compatible(requirement: Option<&str>) -> bool {
    let Some(req) = requirement else {
        return true;
    };
    let req = req.trim().strip_prefix(">=").unwrap_or(req.trim()).trim();
    let (Some(required), Some(current)) = (parse_semver(req), parse_semver(CURRENT_VERSION)) else {
        return true;
    };
    current >= required
}

fn parse_semver(v: &str) -> Option<(u32, u32, u32)> {
    let mut parts = v.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(json: &str) -> Result<PluginManifest, String> {
        parse_manifest(json)
    }

    #[test]
    fn parses_a_minimal_valid_manifest() {
        let m = manifest(r#"{"id":"acme.pack","name":"Pack","version":"1.0.0"}"#).unwrap();
        assert_eq!(m.id, "acme.pack");
        assert!(m.contributes.snippets.is_empty());
    }

    #[test]
    fn rejects_invalid_id() {
        let err = manifest(r#"{"id":"Acme Pack","name":"Pack","version":"1.0.0"}"#).unwrap_err();
        assert!(err.contains("Invalid plugin id"));
    }

    #[test]
    fn rejects_empty_name() {
        let err = manifest(r#"{"id":"acme.pack","name":"  ","version":"1.0.0"}"#).unwrap_err();
        assert!(err.contains("name"));
    }

    #[test]
    fn rejects_non_design_token_theme_variable() {
        let json = r##"{
            "id":"acme.theme","name":"Theme","version":"1.0.0",
            "contributes":{"themes":[
                {"id":"midnight","name":"Midnight","light":{"background":"#000"}}
            ]}
        }"##;
        let err = manifest(json).unwrap_err();
        assert!(err.contains("--q-*"));
    }

    #[test]
    fn accepts_design_token_theme_variable() {
        let json = r##"{
            "id":"acme.theme","name":"Theme","version":"1.0.0",
            "contributes":{"themes":[
                {"id":"midnight","name":"Midnight","light":{"--q-accent":"#3b5bdb"}}
            ]}
        }"##;
        assert!(manifest(json).is_ok());
    }

    #[test]
    fn rejects_theme_value_with_remote_url() {
        let json = r##"{
            "id":"acme.theme","name":"Theme","version":"1.0.0",
            "contributes":{"themes":[
                {"id":"x","name":"X","light":{"--q-bg":"url(https://evil.test/x)"}}
            ]}
        }"##;
        let err = manifest(json).unwrap_err();
        assert!(err.contains("disallowed CSS"));
    }

    #[test]
    fn rejects_snippet_without_template() {
        let json = r#"{
            "id":"acme.pack","name":"Pack","version":"1.0.0",
            "contributes":{"snippets":[{"id":"s1","label":"S1","template":""}]}
        }"#;
        assert!(manifest(json).unwrap_err().contains("template"));
    }

    #[test]
    fn version_requirement_is_best_effort() {
        assert!(is_compatible(None));
        assert!(is_compatible(Some(">=0.0.1")));
        assert!(is_compatible(Some("not-a-version")));
        assert!(!is_compatible(Some(">=999.0.0")));
    }

    #[test]
    fn parses_a_runtime_plugin() {
        let json = r#"{
            "id":"acme.linter","name":"Linter","version":"1.0.0",
            "runtime":{"abiVersion":1,"entry":"plugin.wasm","hooks":["preExecute"]}
        }"#;
        let m = manifest(json).unwrap();
        let runtime = m.runtime.expect("runtime block");
        assert_eq!(runtime.entry, "plugin.wasm");
        assert_eq!(runtime.hooks.len(), 1);
    }

    #[test]
    fn declarative_plugin_has_no_runtime() {
        let m = manifest(r#"{"id":"acme.pack","name":"Pack","version":"1.0.0"}"#).unwrap();
        assert!(m.runtime.is_none());
    }

    #[test]
    fn rejects_runtime_entry_with_path() {
        let json = r#"{
            "id":"acme.linter","name":"Linter","version":"1.0.0",
            "runtime":{"abiVersion":1,"entry":"../evil.wasm"}
        }"#;
        assert!(manifest(json).unwrap_err().contains("bare filename"));
    }

    #[test]
    fn rejects_unknown_abi_version() {
        let json = r#"{
            "id":"acme.linter","name":"Linter","version":"1.0.0",
            "runtime":{"abiVersion":99,"entry":"plugin.wasm"}
        }"#;
        assert!(manifest(json).unwrap_err().contains("ABI version"));
    }

    #[test]
    fn accepts_a_result_viewer() {
        let json = r#"{
            "id":"acme.geo","name":"Geo","version":"1.0.0",
            "contributes":{"resultViewers":[
                {"id":"jsonb","match":{"columnType":"jsonb"},"renderer":"json-tree"}
            ]}
        }"#;
        let m = manifest(json).unwrap();
        assert_eq!(m.contributes.result_viewers.len(), 1);
    }

    #[test]
    fn rejects_viewer_with_no_match() {
        let json = r#"{
            "id":"acme.geo","name":"Geo","version":"1.0.0",
            "contributes":{"resultViewers":[
                {"id":"x","match":{},"renderer":"image"}
            ]}
        }"#;
        assert!(manifest(json).unwrap_err().contains("match"));
    }

    #[test]
    fn rejects_viewer_with_regex_pattern() {
        let json = r#"{
            "id":"acme.geo","name":"Geo","version":"1.0.0",
            "contributes":{"resultViewers":[
                {"id":"x","match":{"namePattern":"^geom_.*$"},"renderer":"map"}
            ]}
        }"#;
        assert!(manifest(json).unwrap_err().contains("wildcards"));
    }

    #[test]
    fn rejects_commands_without_a_runtime() {
        let json = r#"{
            "id":"acme.x","name":"X","version":"1.0.0",
            "contributes":{"commands":[{"id":"go","label":"Go"}]}
        }"#;
        assert!(manifest(json)
            .unwrap_err()
            .contains("Command contributions require"));
    }

    #[test]
    fn accepts_commands_with_a_runtime() {
        let json = r#"{
            "id":"acme.x","name":"X","version":"1.0.0",
            "runtime":{"abiVersion":1,"entry":"plugin.wasm"},
            "contributes":{"commands":[{"id":"go","label":"Go"}]}
        }"#;
        let m = manifest(json).unwrap();
        assert_eq!(m.contributes.commands.len(), 1);
    }

    #[test]
    fn rejects_http_with_empty_allow_list() {
        let json = r#"{
            "id":"acme.x","name":"X","version":"1.0.0",
            "runtime":{"abiVersion":1,"entry":"plugin.wasm",
                       "capabilities":{"http":{"allowedHosts":[]}}}
        }"#;
        assert!(manifest(json).unwrap_err().contains("allowedHosts"));
    }

    #[test]
    fn accepts_http_with_an_allow_list() {
        let json = r#"{
            "id":"acme.x","name":"X","version":"1.0.0",
            "runtime":{"abiVersion":1,"entry":"plugin.wasm",
                       "capabilities":{"http":{"allowedHosts":["api.example.com"]}}}
        }"#;
        let m = manifest(json).unwrap();
        let http = m.runtime.unwrap().capabilities.http.unwrap();
        assert_eq!(http.allowed_hosts, vec!["api.example.com".to_string()]);
    }

    #[test]
    fn accepts_secrets_list() {
        let json = r#"{
            "id":"acme.x","name":"X","version":"1.0.0",
            "runtime":{"abiVersion":1,"entry":"plugin.wasm",
                       "capabilities":{"secrets":["api-token","webhook-url"]}}
        }"#;
        let m = manifest(json).unwrap();
        assert_eq!(m.runtime.unwrap().capabilities.secrets.len(), 2);
    }

    #[test]
    fn rejects_manifest_declaring_query_exec() {
        let json = r#"{
            "id":"acme.x","name":"X","version":"1.0.0",
            "runtime":{"abiVersion":1,"entry":"plugin.wasm",
                       "capabilities":{"queryExec":true}}
        }"#;
        let err = manifest(json).unwrap_err();
        assert!(err.contains("queryExec"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_command_with_empty_label() {
        let json = r#"{
            "id":"acme.x","name":"X","version":"1.0.0",
            "runtime":{"abiVersion":1,"entry":"plugin.wasm"},
            "contributes":{"commands":[{"id":"go","label":"  "}]}
        }"#;
        assert!(manifest(json).unwrap_err().contains("label"));
    }
}
