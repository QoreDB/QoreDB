# Dev License Keypair (Ed25519)

**DO NOT commit this file or share the private key.**

## Seed (used to derive the keypair deterministically)

```
qoredb-dev-license-key-seed!1234
```

Bytes: `[113, 111, 114, 101, 100, 98, 45, 100, 101, 118, 45, 108, 105, 99, 101, 110, 115, 101, 45, 107, 101, 121, 45, 115, 101, 101, 100, 33, 49, 50, 51, 52]`

## Public Key (embedded in binary)

```rust
const PUBLIC_KEY_BYTES: [u8; 32] = [
    1, 113, 141, 7, 16, 243, 72, 191, 94, 203, 142, 178, 11, 110, 99, 138,
    1, 104, 110, 132, 222, 221, 231, 246, 206, 72, 216, 110, 19, 248, 61, 112,
];
```

## Regenerating

The keypair is deterministic from the seed. To regenerate or use in tests:

```rust
use ed25519_dalek::SigningKey;
let seed: [u8; 32] = *b"qoredb-dev-license-key-seed!1234";
let signing_key = SigningKey::from_bytes(&seed);
let public_key = signing_key.verifying_key();
```

Or run: `cargo test -p qoredb generate_dev_keypair -- --nocapture`

## Production

For production, generate a new random keypair and:
1. Replace `PUBLIC_KEY_BYTES` in `src-tauri/src/license/key.rs`
2. Use the private key on the license server to sign keys
3. **Never** commit the production private key
