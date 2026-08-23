//! The canonical encoding of proposal 001 section 3.1.
//!
//! The canonical form is what prost emits: fields in ascending field-number
//! order, minimal varints, no proto3 default value serialized, each
//! non-repeated field once, and no packed repeated fields (every repeated
//! field in the signed messages is `bytes` or a message, which protobuf
//! length-delimits per entry). mabel does not reimplement the encoder; the
//! tests in this module assert each of those properties and the golden
//! vectors in `test-vectors/` pin the resulting bytes for other
//! implementations.

use mabel_proto::prost::Message;

/// Encodes a message in canonical form.
///
/// Only the signing path calls this for an `EventBody` or an `Acceptance`:
/// the bytes it returns are hashed, signed, stored and shipped as-is, and no
/// other code path may re-encode them (proposal 001 section 3.1, pitfall 1).
pub(crate) fn encode<M: Message>(msg: &M) -> Vec<u8> {
    let mut buf = Vec::with_capacity(msg.encoded_len());
    msg.encode(&mut buf)
        .expect("encoding into a Vec cannot fail");
    buf
}

#[cfg(test)]
mod tests {
    use super::encode;
    use mabel_proto::prost::Message;
    use mabel_proto::v0::{EventBody, WitnessConfig};

    /// The tag and wire type of each record in an encoded message, in the
    /// order they appear. Handles the two wire types the mabel schemas use:
    /// varint (0) and length-delimited (2).
    fn scan_records(mut buf: &[u8]) -> Vec<(u32, u8)> {
        let mut out = Vec::new();
        while !buf.is_empty() {
            let (key, rest) = read_varint(buf);
            let (tag, wire) = ((key >> 3) as u32, (key & 7) as u8);
            buf = match wire {
                0 => read_varint(rest).1,
                2 => {
                    let (len, rest) = read_varint(rest);
                    &rest[len as usize..]
                }
                other => panic!("unexpected wire type {other}"),
            };
            out.push((tag, wire));
        }
        out
    }

    fn read_varint(buf: &[u8]) -> (u64, &[u8]) {
        let mut value = 0u64;
        for (i, byte) in buf.iter().enumerate() {
            value |= u64::from(byte & 0x7f) << (7 * i);
            if byte & 0x80 == 0 {
                return (value, &buf[i + 1..]);
            }
        }
        panic!("truncated varint");
    }

    fn body_with_all_envelope_fields() -> EventBody {
        EventBody {
            version: 0,
            ledger: vec![1u8; 32],
            seq: 3,
            prev: vec![2u8; 32],
            timestamp_ms: 1_700_000_000_000,
            author_key: vec![3u8; 32],
            payload: Some(mabel_proto::v0::event_body::Payload::TrustRevocation(
                mabel_proto::v0::TrustRevocation {
                    target: vec![4u8; 32],
                },
            )),
        }
    }

    #[test]
    fn defaults_are_absent() {
        let explicit = EventBody {
            version: 0,
            ledger: Vec::new(),
            seq: 0,
            prev: Vec::new(),
            timestamp_ms: 1_700_000_000_000,
            author_key: vec![3u8; 32],
            payload: None,
        };
        let omitted = EventBody {
            timestamp_ms: 1_700_000_000_000,
            author_key: vec![3u8; 32],
            ..EventBody::default()
        };

        assert_eq!(encode(&explicit), encode(&omitted));
        let tags: Vec<u32> = scan_records(&encode(&explicit))
            .into_iter()
            .map(|(tag, _)| tag)
            .collect();
        assert_eq!(tags, vec![5, 6]);
    }

    #[test]
    fn fields_are_in_ascending_order() {
        let tags: Vec<u32> = scan_records(&encode(&body_with_all_envelope_fields()))
            .into_iter()
            .map(|(tag, _)| tag)
            .collect();
        assert_eq!(tags, vec![2, 3, 4, 5, 6, 13]);
        assert!(tags.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn encoding_matches_a_hand_built_canonical_encoding() {
        let body = EventBody {
            seq: 1,
            timestamp_ms: 2,
            author_key: vec![9u8; 32],
            ..EventBody::default()
        };
        let mut expected = vec![0x18, 0x01, 0x28, 0x02, 0x32, 0x20];
        expected.extend_from_slice(&[9u8; 32]);
        assert_eq!(encode(&body), expected);
    }

    #[test]
    fn non_minimal_varints_differ_from_the_canonical_bytes() {
        let body = EventBody {
            seq: 1,
            timestamp_ms: 2,
            author_key: vec![9u8; 32],
            ..EventBody::default()
        };
        let canonical = encode(&body);

        let mut non_minimal = vec![0x18, 0x81, 0x00, 0x28, 0x02, 0x32, 0x20];
        non_minimal.extend_from_slice(&[9u8; 32]);

        assert_ne!(canonical, non_minimal);
        // prost accepts the padded varint and yields the same message, which
        // is why the strict gate is a byte scanner over the received bytes
        // rather than a re-encode comparison (proposal 001 section 3.1).
        assert_eq!(EventBody::decode(&non_minimal[..]).unwrap(), body);
    }

    #[test]
    fn repeated_bytes_are_not_packed() {
        let config = WitnessConfig {
            witnesses: vec![vec![1u8; 32], vec![2u8; 32]],
        };
        assert_eq!(scan_records(&encode(&config)), vec![(1, 2), (1, 2)]);
    }
}
