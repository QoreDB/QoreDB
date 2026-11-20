// SPDX-License-Identifier: BUSL-1.1

//! Self-signed TLS certificate generation for the Instant Data API.
//!
//! Certificates live in RAM only — they are minted at server start, kept in
//! the `ApiServer` for as long as the HTTPS listener runs, and dropped when
//! the server stops. They are **never** written to disk and **never**
//! reused across restarts, so a compromised cert can't outlive one session.
//!
//! Because the cert is self-signed, every browser will require an explicit
//! user override on first visit. This is expected — the server binds only
//! to `127.0.0.1`, so the TLS layer here is purely about transport hygiene
//! (defence in depth against local-network observers, never identity).

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TlsError {
    #[error("failed to generate self-signed certificate: {0}")]
    Generate(String),
}

/// Self-signed PEM-encoded certificate and matching private key. Lives in
/// memory for the lifetime of the HTTPS server.
pub struct SelfSignedCert {
    pub cert_pem: String,
    pub key_pem: String,
}

/// Mints a fresh ephemeral self-signed certificate in memory for `127.0.0.1` + `localhost`.
///
/// The certificate is valid for [`VALIDITY_DAYS`] (from the day before, to
/// absorb clock skew) and is regenerated on every server start without ever
/// touching disk, limiting exposure to the running HTTPS server instance.
pub const VALIDITY_DAYS: i64 = 30;

fn certificate_params(today: chrono::NaiveDate) -> Result<CertificateParams, TlsError> {
    let mut params = CertificateParams::new(vec!["127.0.0.1".to_string(), "localhost".to_string()])
        .map_err(|e| TlsError::Generate(e.to_string()))?;

    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "QoreDB Instant API");
    dn.push(DnType::OrganizationName, "QoreDB local");
    params.distinguished_name = dn;
    let ymd = |date: chrono::NaiveDate| {
        use chrono::Datelike;
        rcgen::date_time_ymd(date.year(), date.month() as u8, date.day() as u8)
    };
    params.not_before = ymd(today - chrono::Duration::days(1));
    params.not_after = ymd(today + chrono::Duration::days(VALIDITY_DAYS));

    // Force the IP into the SAN even when rcgen's parser doesn't auto-detect it.
    params
        .subject_alt_names
        .push(SanType::IpAddress(std::net::IpAddr::from([127, 0, 0, 1])));
    Ok(params)
}

pub fn generate_self_signed() -> Result<SelfSignedCert, TlsError> {
    let params = certificate_params(chrono::Utc::now().date_naive())?;
    let key_pair = KeyPair::generate().map_err(|e| TlsError::Generate(e.to_string()))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| TlsError::Generate(e.to_string()))?;

    Ok(SelfSignedCert {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_pem_with_expected_headers() {
        let bundle = generate_self_signed().expect("generation should succeed");
        assert!(bundle.cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(bundle.cert_pem.contains("-----END CERTIFICATE-----"));
        assert!(bundle.key_pem.contains("PRIVATE KEY"));
    }

    #[test]
    fn each_call_returns_a_fresh_certificate() {
        let a = generate_self_signed().unwrap();
        let b = generate_self_signed().unwrap();
        // Two independent KeyPair::generate() calls — vanishingly small
        // chance of collision; if this ever flakes we're rebuilding the
        // sun out of dice.
        assert_ne!(a.cert_pem, b.cert_pem);
        assert_ne!(a.key_pem, b.key_pem);
    }

    #[test]
    fn certificate_is_short_lived() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 2, 27).unwrap();
        let params = certificate_params(today).unwrap();
        assert_eq!(
            (params.not_after - params.not_before).whole_days(),
            VALIDITY_DAYS + 1
        );
        assert_eq!(params.not_before.date().to_string(), "2026-02-26");
    }
}
