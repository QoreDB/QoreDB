// SPDX-License-Identifier: Apache-2.0

//! Encrypted-file credential provider.
//!
//! Drop-in `CredentialProvider` for headless/containerised deployments where the
//! OS keyring is unavailable. Credentials are encrypted at rest with
//! XChaCha20Poly1305 under a key derived from a passphrase via Argon2id. The
//! passphrase comes from the environment; without it, the file cannot be read.

use std::collections::HashMap;
use std::path::PathBuf;

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use parking_lot::Mutex;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};

use crate::vault::backend::{CredentialError, CredentialProvider};
use qore_core::error::{EngineError, EngineResult};

const SALT_LEN: usize = 16;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;

#[derive(Serialize, Deserialize)]
struct EncEntry {
    nonce: String,
    ct: String,
}

#[derive(Serialize, Deserialize, Default)]
struct VaultFile {
    version: u8,
    salt: String,
    entries: HashMap<String, EncEntry>,
}

pub struct EncryptedFileProvider {
    path: PathBuf,
    cipher: XChaCha20Poly1305,
    lock: Mutex<()>,
}

impl EncryptedFileProvider {
    pub fn new(path: PathBuf, passphrase: &str) -> EngineResult<Self> {
        if passphrase.is_empty() {
            return Err(EngineError::validation("Vault passphrase must not be empty"));
        }

        let salt = match read_file(&path)? {
            Some(file) => BASE64
                .decode(&file.salt)
                .map_err(|e| EngineError::internal(format!("Invalid vault salt: {e}")))?,
            None => {
                let mut salt = vec![0u8; SALT_LEN];
                OsRng.fill_bytes(&mut salt);
                write_file(
                    &path,
                    &VaultFile {
                        version: 1,
                        salt: BASE64.encode(&salt),
                        entries: HashMap::new(),
                    },
                )?;
                salt
            }
        };

        let key = derive_key(passphrase, &salt)?;
        let cipher = XChaCha20Poly1305::new_from_slice(&key)
            .map_err(|e| EngineError::internal(format!("Cipher init failed: {e}")))?;

        Ok(Self {
            path,
            cipher,
            lock: Mutex::new(()),
        })
    }

    fn entry_key(service: &str, username: &str) -> String {
        format!("{service}\u{1f}{username}")
    }

    fn encrypt(&self, aad: &str, plaintext: &str) -> EngineResult<EncEntry> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = XNonce::from_slice(&nonce_bytes);
        let ct = self
            .cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| EngineError::internal("Credential encryption failed"))?;
        Ok(EncEntry {
            nonce: BASE64.encode(nonce_bytes),
            ct: BASE64.encode(ct),
        })
    }

    fn decrypt(&self, aad: &str, entry: &EncEntry) -> EngineResult<String> {
        let nonce_bytes = BASE64
            .decode(&entry.nonce)
            .map_err(|e| EngineError::internal(format!("Invalid nonce: {e}")))?;
        let ct = BASE64
            .decode(&entry.ct)
            .map_err(|e| EngineError::internal(format!("Invalid ciphertext: {e}")))?;
        let nonce = XNonce::from_slice(&nonce_bytes);
        let pt = self
            .cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &ct,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| EngineError::internal("Credential decryption failed"))?;
        String::from_utf8(pt)
            .map_err(|_| EngineError::internal("Decrypted credential is not valid UTF-8"))
    }

    fn load(&self) -> EngineResult<VaultFile> {
        Ok(read_file(&self.path)?.unwrap_or_default())
    }
}

impl CredentialProvider for EncryptedFileProvider {
    fn set_password(&self, service: &str, username: &str, password: &str) -> EngineResult<()> {
        let _guard = self.lock.lock();
        let key = Self::entry_key(service, username);
        let entry = self.encrypt(&key, password)?;
        let mut file = self.load()?;
        file.entries.insert(key, entry);
        write_file(&self.path, &file)
    }

    fn get_password(&self, service: &str, username: &str) -> EngineResult<String> {
        let key = Self::entry_key(service, username);
        let file = self.load()?;
        match file.entries.get(&key) {
            Some(entry) => self.decrypt(&key, entry),
            None => Err(EngineError::internal("Credentials not found")),
        }
    }

    fn delete_password(&self, service: &str, username: &str) -> EngineResult<()> {
        match self.delete_credential(service, username) {
            Ok(()) | Err(CredentialError::NotFound) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn has_credential(&self, service: &str, username: &str) -> Result<bool, CredentialError> {
        let key = Self::entry_key(service, username);
        let file = self
            .load()
            .map_err(|e| CredentialError::Other(e.to_string()))?;
        Ok(file.entries.contains_key(&key))
    }

    fn delete_credential(&self, service: &str, username: &str) -> Result<(), CredentialError> {
        let _guard = self.lock.lock();
        let key = Self::entry_key(service, username);
        let mut file = self
            .load()
            .map_err(|e| CredentialError::Other(e.to_string()))?;
        if file.entries.remove(&key).is_none() {
            return Err(CredentialError::NotFound);
        }
        write_file(&self.path, &file).map_err(|e| CredentialError::Other(e.to_string()))
    }
}

fn derive_key(passphrase: &str, salt: &[u8]) -> EngineResult<[u8; KEY_LEN]> {
    let params = Params::new(64 * 1024, 3, 1, Some(KEY_LEN))
        .map_err(|e| EngineError::internal(format!("Argon2 params: {e}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| EngineError::internal(format!("Key derivation failed: {e}")))?;
    Ok(key)
}

fn read_file(path: &PathBuf) -> EngineResult<Option<VaultFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| EngineError::internal(format!("Failed to read vault file: {e}")))?;
    let file = serde_json::from_str(&content)
        .map_err(|e| EngineError::internal(format!("Failed to parse vault file: {e}")))?;
    Ok(Some(file))
}

fn write_file(path: &PathBuf, file: &VaultFile) -> EngineResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| EngineError::internal(format!("Failed to create vault dir: {e}")))?;
    }
    let content = serde_json::to_string(file)
        .map_err(|e| EngineError::internal(format!("Failed to serialize vault: {e}")))?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)
        .map_err(|e| EngineError::internal(format!("Failed to write vault file: {e}")))?;
    restrict_permissions(&tmp);
    std::fs::rename(&tmp, path)
        .map_err(|e| EngineError::internal(format!("Failed to commit vault file: {e}")))?;
    Ok(())
}

#[cfg(unix)]
fn restrict_permissions(path: &PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &PathBuf) {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn roundtrip_persists_across_instances() -> EngineResult<()> {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("vault.enc");

        let p1 = EncryptedFileProvider::new(path.clone(), "correct horse battery")?;
        p1.set_password("svc", "user", "s3cret")?;
        assert!(p1.has_credential("svc", "user").unwrap());

        let p2 = EncryptedFileProvider::new(path.clone(), "correct horse battery")?;
        assert_eq!(p2.get_password("svc", "user")?, "s3cret");

        p2.delete_password("svc", "user")?;
        assert!(!p2.has_credential("svc", "user").unwrap());
        assert!(p2.get_password("svc", "user").is_err());
        Ok(())
    }

    #[test]
    fn wrong_passphrase_cannot_decrypt() -> EngineResult<()> {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("vault.enc");

        let p1 = EncryptedFileProvider::new(path.clone(), "passphrase-one")?;
        p1.set_password("svc", "user", "s3cret")?;

        let p2 = EncryptedFileProvider::new(path.clone(), "passphrase-two")?;
        assert!(p2.get_password("svc", "user").is_err());
        Ok(())
    }

    #[test]
    fn empty_passphrase_rejected() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("vault.enc");
        assert!(EncryptedFileProvider::new(path, "").is_err());
    }
}
