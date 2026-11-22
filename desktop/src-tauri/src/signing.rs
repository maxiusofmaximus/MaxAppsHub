//! Checks the Ed25519 signature that the Hub's publisher puts on each release. The
//! public key is built in: a release signed with any other key is refused, even when
//! GitHub's own digest matches, so a hijacked release page cannot push a Hub update.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// The public half of the update key. The private half is kept outside the repository.
const UPDATE_PUBLIC_KEY: &str = "ecdc16d703c2732e4268e0ccb4cbc804809208a899f5b893d489a464c7a7f162";

/// True when `signature_hex` (64 bytes, hex) is the update key's signature of `bytes`.
pub fn verified(signature_hex: &str, bytes: &[u8]) -> bool {
    verify_with(UPDATE_PUBLIC_KEY, signature_hex, bytes)
}

fn verify_with(public_hex: &str, signature_hex: &str, bytes: &[u8]) -> bool {
    let Ok(key) = hex::decode(public_hex) else { return false };
    let Ok(key) = <[u8; 32]>::try_from(key.as_slice()) else { return false };
    let Ok(key) = VerifyingKey::from_bytes(&key) else { return false };
    let Ok(signature) = hex::decode(signature_hex.trim()) else { return false };
    let Ok(signature) = Signature::from_slice(&signature) else { return false };
    key.verify(bytes, &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signature_is_accepted_only_for_the_exact_bytes_and_key() {
        // RFC 8032, section 7.1, test 1 (the empty message).
        let key = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
        let signature = "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b";
        assert!(verify_with(key, signature, b""));
        assert!(!verify_with(key, signature, b"otro"));
        assert!(!verify_with(UPDATE_PUBLIC_KEY, signature, b""));
        assert!(!verify_with(key, "no es hex", b""));
    }

    #[test]
    fn the_built_in_key_accepts_a_release_signed_with_the_private_key() {
        // Made with the update key: openssl pkeyutl -sign -rawin on this exact text.
        let signature = "cf410a21eb9ad13ffe242f8a52c3852d282132da3c2fe5228c2075067a495a93cef8750e3d2ce5f0e94b7ee800b5c991b6fe76347428a545816c1499dfda5206";
        assert!(verified(signature, b"maxapps-hub-update-test"));
        assert!(!verified(signature, b"maxapps-hub-update-test!"));
    }
}
