// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

/// License tier determines which features are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LicenseTier {
    Core,
    Pro,
    Team,
    Enterprise,
}

impl LicenseTier {
    /// Returns true if this tier includes the given tier's features.
    pub fn includes(&self, required: LicenseTier) -> bool {
        self.level() >= required.level()
    }

    fn level(&self) -> u8 {
        match self {
            LicenseTier::Core => 0,
            LicenseTier::Pro => 1,
            LicenseTier::Team => 2,
            LicenseTier::Enterprise => 3,
        }
    }
}

impl Default for LicenseTier {
    fn default() -> Self {
        LicenseTier::Core
    }
}

/// Current license status exposed to frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseStatus {
    pub tier: LicenseTier,
    pub email: Option<String>,
    pub payment_id: Option<String>,
    pub issued_at: Option<String>,
    pub expires_at: Option<String>,
    pub is_expired: bool,
}

impl Default for LicenseStatus {
    fn default() -> Self {
        Self {
            tier: LicenseTier::Core,
            email: None,
            payment_id: None,
            issued_at: None,
            expires_at: None,
            is_expired: false,
        }
    }
}

/// Feature identifiers for gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProFeature {
    Sandbox,
    VisualDiff,
    ErDiagram,
    AuditAdvanced,
    Profiling,
    Ai,
    ExportXlsx,
    ExportParquet,
    CustomSafetyRules,
    QueryLibraryAdvanced,
    VirtualRelationsAutoSuggest,
}

impl ProFeature {
    /// Minimum tier required for this feature.
    pub fn required_tier(&self) -> LicenseTier {
        match self {
            // All Pro features require at least Pro
            ProFeature::Sandbox
            | ProFeature::VisualDiff
            | ProFeature::ErDiagram
            | ProFeature::AuditAdvanced
            | ProFeature::Profiling
            | ProFeature::Ai
            | ProFeature::ExportXlsx
            | ProFeature::ExportParquet
            | ProFeature::CustomSafetyRules
            | ProFeature::QueryLibraryAdvanced
            | ProFeature::VirtualRelationsAutoSuggest => LicenseTier::Pro,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_hierarchy() {
        assert!(LicenseTier::Enterprise.includes(LicenseTier::Core));
        assert!(LicenseTier::Enterprise.includes(LicenseTier::Pro));
        assert!(LicenseTier::Enterprise.includes(LicenseTier::Team));
        assert!(LicenseTier::Pro.includes(LicenseTier::Core));
        assert!(LicenseTier::Pro.includes(LicenseTier::Pro));
        assert!(!LicenseTier::Pro.includes(LicenseTier::Team));
        assert!(!LicenseTier::Core.includes(LicenseTier::Pro));
    }

    #[test]
    fn default_is_core() {
        assert_eq!(LicenseTier::default(), LicenseTier::Core);
        assert_eq!(LicenseStatus::default().tier, LicenseTier::Core);
    }
}
