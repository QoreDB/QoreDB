// SPDX-License-Identifier: Apache-2.0

//! `plugin.json` parsing and validation.

use super::{PluginContributions, PluginManifest};

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Parses raw `plugin.json` content into a validated manifest.
pub fn parse_manifest(raw: &str) -> Result<PluginManifest, String> {
    let manifest: PluginManifest =
        serde_json::from_str(raw).map_err(|e| format!("Invalid plugin.json: {e}"))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
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
    validate_contributions(&m.contributes)
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
}
