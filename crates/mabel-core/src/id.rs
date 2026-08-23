//! 32-byte ids and their text form (proposal 001 section 3.1).
//!
//! An id renders as lowercase RFC 4648 base32 without padding, 52 characters,
//! with no type prefix. Parsing also accepts uppercase, which decodes to the
//! same bytes; ids are never hashed, so only the bytes are authoritative.

use std::fmt;
use std::str::FromStr;

use data_encoding::BASE32_NOPAD;

use crate::ID_BYTES;

/// Length of an id in its base32 text form.
pub const ID_STR_LEN: usize = 52;

/// Why a string or byte slice is not an id.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseIdError {
    /// The text form was not 52 characters.
    #[error("id must be 52 base32 characters, got {0}")]
    TextLength(usize),
    /// The text form held characters outside the base32 alphabet, or trailing
    /// bits that are not zero.
    #[error("id is not lowercase RFC 4648 base32")]
    Alphabet,
    /// The byte form was not 32 bytes.
    #[error("id must be 32 bytes, got {0}")]
    ByteLength(usize),
}

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; ID_BYTES]);

        impl $name {
            /// Wraps 32 raw bytes.
            pub const fn from_bytes(bytes: [u8; ID_BYTES]) -> Self {
                Self(bytes)
            }

            /// The 32 raw bytes.
            pub const fn as_bytes(&self) -> &[u8; ID_BYTES] {
                &self.0
            }

            /// Copies the id out for a protobuf `bytes` field.
            pub fn to_vec(&self) -> Vec<u8> {
                self.0.to_vec()
            }

            /// Reads an id from a decoded `bytes` field.
            pub fn from_slice(bytes: &[u8]) -> Result<Self, ParseIdError> {
                let bytes: [u8; ID_BYTES] = bytes
                    .try_into()
                    .map_err(|_| ParseIdError::ByteLength(bytes.len()))?;
                Ok(Self(bytes))
            }
        }

        impl From<[u8; ID_BYTES]> for $name {
            fn from(bytes: [u8; ID_BYTES]) -> Self {
                Self(bytes)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&BASE32_NOPAD.encode(&self.0).to_ascii_lowercase())
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self)
            }
        }

        impl FromStr for $name {
            type Err = ParseIdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                if s.len() != ID_STR_LEN {
                    return Err(ParseIdError::TextLength(s.len()));
                }
                let bytes = BASE32_NOPAD
                    .decode(s.to_ascii_uppercase().as_bytes())
                    .map_err(|_| ParseIdError::Alphabet)?;
                Self::from_slice(&bytes)
            }
        }

        #[cfg(feature = "serde")]
        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <String as serde::Deserialize>::deserialize(deserializer)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

define_id! {
    /// `BLAKE3("mabel/event/v0\n" || event_body_bytes)`.
    EventId
}

define_id! {
    /// The `event_id` of a ledger's seq-0 event, which names both the ledger
    /// and the identity it belongs to (proposal 001 section 3.3).
    IdentityId
}

/// A ledger is named by the identity it belongs to (proposal 001 section 3.3).
pub type LedgerId = IdentityId;

impl From<EventId> for IdentityId {
    /// A ledger's id is the `event_id` of its seq-0 event, so an inception's
    /// event id is the identity it creates (proposal 001 section 3.3).
    fn from(event: EventId) -> Self {
        Self(*event.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::{EventId, ID_STR_LEN, IdentityId, ParseIdError};

    #[test]
    fn renders_as_52_lowercase_base32_characters() {
        let text = EventId::from_bytes([0xab; 32]).to_string();
        assert_eq!(text.len(), ID_STR_LEN);
        assert!(
            text.chars()
                .all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c)),
            "unexpected characters in {text}"
        );
    }

    #[test]
    fn round_trips_through_base32() {
        for seed in [0u8, 1, 0x7f, 0xff] {
            let id = IdentityId::from_bytes([seed; 32]);
            let parsed: IdentityId = id.to_string().parse().expect("parses");
            assert_eq!(parsed, id);
        }
        let zero = EventId::from_bytes([0u8; 32]);
        assert_eq!(zero.to_string(), "a".repeat(ID_STR_LEN));
        assert_eq!(zero.to_string().parse::<EventId>().unwrap(), zero);
    }

    #[test]
    fn parsing_accepts_uppercase_and_rejects_the_rest() {
        let id = EventId::from_bytes([9u8; 32]);
        let upper = id.to_string().to_ascii_uppercase();
        assert_eq!(upper.parse::<EventId>().unwrap(), id);

        assert_eq!("abc".parse::<EventId>(), Err(ParseIdError::TextLength(3)));
        assert_eq!(
            "1".repeat(ID_STR_LEN).parse::<EventId>(),
            Err(ParseIdError::Alphabet)
        );
        // 52 base32 characters carry 260 bits; the 4 trailing bits must be
        // zero, so only 32 bytes can round-trip.
        let mut trailing = id.to_string();
        trailing.pop();
        trailing.push('b');
        assert_eq!(trailing.parse::<EventId>(), Err(ParseIdError::Alphabet));
    }

    #[test]
    fn reads_bytes_from_a_slice() {
        assert_eq!(
            EventId::from_slice(&[1u8; 31]),
            Err(ParseIdError::ByteLength(31))
        );
        assert_eq!(
            EventId::from_slice(&[1u8; 32]).unwrap(),
            EventId::from_bytes([1u8; 32])
        );
    }

    #[test]
    fn debug_names_the_type() {
        let id = EventId::from_bytes([0u8; 32]);
        assert_eq!(format!("{id:?}"), format!("EventId({id})"));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_uses_the_text_form() {
        let id = EventId::from_bytes([2u8; 32]);
        let json = serde_json::to_string(&id).expect("serializes");
        assert_eq!(json, format!("\"{id}\""));
        assert_eq!(serde_json::from_str::<EventId>(&json).unwrap(), id);
    }
}
