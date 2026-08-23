//! The four domain-separated digests and signing inputs of proposal 001
//! section 3.1.
//!
//! ```text
//! event_id       = BLAKE3(b"mabel/event/v0\n"   || event_body_bytes)
//! sign_input     =        b"mabel/sig/v0\n"     || event_body_bytes
//! accept_input   =        b"mabel/accept/v0\n"  || acceptance_bytes
//! reserve_commit = BLAKE3(b"mabel/reserve/v0\n" || reserve_public_key)
//! ```
//!
//! Every one of these takes the received or emitted bytes. A caller that
//! re-encodes a decoded message before hashing breaks the signature
//! (pitfall 1).

use iroh_base::PublicKey;

use crate::id::EventId;
use crate::{ACCEPT_DOMAIN, EVENT_ID_DOMAIN, ID_BYTES, RESERVE_DOMAIN, SIGN_DOMAIN};

/// The id of the event whose `EventBody` encodes to `body_bytes`.
pub fn event_id(body_bytes: &[u8]) -> EventId {
    EventId::from_bytes(hash(EVENT_ID_DOMAIN, body_bytes))
}

/// The bytes an event's author signs.
pub fn sign_input(body_bytes: &[u8]) -> Vec<u8> {
    prefixed(SIGN_DOMAIN, body_bytes)
}

/// The bytes an invitee signs for a detached acceptance (section 3.5).
pub fn accept_input(acceptance_bytes: &[u8]) -> Vec<u8> {
    prefixed(ACCEPT_DOMAIN, acceptance_bytes)
}

/// The commitment a `RawRoot` records for its reserve key.
pub fn reserve_commit(reserve_key: &PublicKey) -> [u8; ID_BYTES] {
    hash(RESERVE_DOMAIN, reserve_key.as_bytes())
}

fn hash(domain: &[u8], bytes: &[u8]) -> [u8; ID_BYTES] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn prefixed(domain: &[u8], bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(domain.len() + bytes.len());
    out.extend_from_slice(domain);
    out.extend_from_slice(bytes);
    out
}

#[cfg(test)]
mod tests {
    use super::{accept_input, event_id, reserve_commit, sign_input};

    #[test]
    fn digests_hash_the_domain_then_the_bytes() {
        let body = b"body bytes";
        let mut expected = b"mabel/event/v0\n".to_vec();
        expected.extend_from_slice(body);
        assert_eq!(
            event_id(body).as_bytes(),
            blake3::hash(&expected).as_bytes()
        );

        let key = iroh_base::SecretKey::from_bytes(&[5u8; 32]).public();
        let mut expected = b"mabel/reserve/v0\n".to_vec();
        expected.extend_from_slice(key.as_bytes());
        assert_eq!(&reserve_commit(&key), blake3::hash(&expected).as_bytes());
    }

    #[test]
    fn signing_inputs_prefix_the_domain() {
        assert_eq!(sign_input(b"xy"), b"mabel/sig/v0\nxy".to_vec());
        assert_eq!(accept_input(b"xy"), b"mabel/accept/v0\nxy".to_vec());
    }

    #[test]
    fn domains_separate_the_same_bytes() {
        let bytes = b"same";
        assert_ne!(sign_input(bytes), accept_input(bytes));
        let key = iroh_base::SecretKey::from_bytes(&[6u8; 32]).public();
        assert_ne!(event_id(key.as_bytes()).as_bytes(), &reserve_commit(&key));
    }
}
