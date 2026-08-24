//! `peers.json`: where to look for a ledger (proposal 001 section 8, proposal
//! 006 section 5.3).
//!
//! Address hints, never authorization: a peer that hands over a ledger is
//! still checked against the chain (proposal 001 section 4). An
//! `EndpointTicket` reaches a node on the command line as `--peer`, so it is
//! never stored here, and neither is any other `CallerHint` endpoint: an
//! endpoint that arrived in a link or on a command line served the operation it
//! came with and nothing more.
//!
//! This is a cache with a cap, an age-out and an eviction rule, not a
//! register. Each ledger holds at most [`MAX_HINTS`] hints; over the cap the
//! hint with the oldest success is dropped, a hint with no success in
//! [`HINT_MAX_AGE_MS`] is dropped, and [`MAX_FAILURES`] consecutive failures
//! evict a hint while one success resets the count.

use std::collections::BTreeMap;

use iroh_base::EndpointId;
use mabel_core::LedgerId;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

/// Hints one ledger may hold (proposal 006 section 5.3).
pub const MAX_HINTS: usize = 8;

/// How long a hint survives without a success: 30 days.
pub const HINT_MAX_AGE_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// Consecutive failures that evict a hint.
pub const MAX_FAILURES: u32 = 3;

/// One endpoint that once served a ledger, and what happened since.
///
/// `first_seen_ms` and `last_success_ms` are zero on a hint read from the bare
/// string an older `peers.json` holds: that file recorded no time, and a hint
/// with no timestamp has no age rather than an infinite one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PeerHint {
    /// The endpoint to dial.
    pub endpoint: EndpointId,
    /// When this home first wrote the hint.
    pub first_seen_ms: u64,
    /// When the endpoint last served this ledger.
    pub last_success_ms: u64,
    /// Failures since the last success.
    pub failures: u32,
}

impl PeerHint {
    /// A hint recorded now, with one success behind it.
    #[must_use]
    pub const fn served(endpoint: EndpointId, now_ms: u64) -> Self {
        Self {
            endpoint,
            first_seen_ms: now_ms,
            last_success_ms: now_ms,
            failures: 0,
        }
    }

    /// A hint an older `peers.json` recorded as a bare string: an endpoint and
    /// no history at all.
    #[must_use]
    pub const fn undated(endpoint: EndpointId) -> Self {
        Self {
            endpoint,
            first_seen_ms: 0,
            last_success_ms: 0,
            failures: 0,
        }
    }

    /// Whether this hint has gone [`HINT_MAX_AGE_MS`] without a success.
    ///
    /// A hint with no timestamps is never aged out: it came from a file that
    /// recorded no time, so there is nothing to measure. Its first write in
    /// the new shape stamps it and the clock starts then.
    #[must_use]
    pub const fn aged_out(&self, now_ms: u64) -> bool {
        let stamp = if self.last_success_ms > self.first_seen_ms {
            self.last_success_ms
        } else {
            self.first_seen_ms
        };
        if stamp == 0 {
            return false;
        }
        now_ms.saturating_sub(stamp) > HINT_MAX_AGE_MS
    }

    /// Whether this hint is still worth dialling at `now_ms`.
    #[must_use]
    pub const fn live(&self, now_ms: u64) -> bool {
        self.failures < MAX_FAILURES && !self.aged_out(now_ms)
    }
}

/// Reads a hint from either shape: the object of proposal 006 section 5.3, or
/// the bare endpoint id an older `peers.json` holds.
impl<'de> Deserialize<'de> for PeerHint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(HintVisitor)
    }
}

struct HintVisitor;

impl<'de> Visitor<'de> for HintVisitor {
    type Value = PeerHint;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an endpoint id, or an object holding one")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<PeerHint, E> {
        let endpoint = value.parse::<EndpointId>().map_err(E::custom)?;
        Ok(PeerHint::undated(endpoint))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<PeerHint, A::Error> {
        let mut endpoint: Option<EndpointId> = None;
        let mut first_seen_ms = 0u64;
        let mut last_success_ms = 0u64;
        let mut failures = 0u32;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "endpoint" => endpoint = Some(map.next_value()?),
                "first_seen_ms" => first_seen_ms = map.next_value()?,
                "last_success_ms" => last_success_ms = map.next_value()?,
                "failures" => failures = map.next_value()?,
                other => {
                    return Err(de::Error::unknown_field(
                        other,
                        &["endpoint", "first_seen_ms", "last_success_ms", "failures"],
                    ));
                }
            }
        }
        let endpoint = endpoint.ok_or_else(|| de::Error::missing_field("endpoint"))?;
        Ok(PeerHint {
            endpoint,
            first_seen_ms,
            last_success_ms,
            failures,
        })
    }
}

/// The contents of `peers.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peers {
    /// Endpoints known to hold a given ledger, at most [`MAX_HINTS`] each.
    #[serde(default)]
    pub ledgers: BTreeMap<LedgerId, Vec<PeerHint>>,
}

impl Peers {
    /// Records that `endpoint` served `ledger` at `now_ms`.
    ///
    /// A hint already there has its `last_success_ms` refreshed and its
    /// `failures` cleared; a new one is appended and the cap is enforced by
    /// dropping the hint with the oldest success.
    pub fn record_success(&mut self, ledger: LedgerId, endpoint: EndpointId, now_ms: u64) {
        let hints = self.ledgers.entry(ledger).or_default();
        if let Some(hint) = hints.iter_mut().find(|hint| hint.endpoint == endpoint) {
            hint.last_success_ms = now_ms;
            hint.failures = 0;
            if hint.first_seen_ms == 0 {
                hint.first_seen_ms = now_ms;
            }
            return;
        }
        hints.push(PeerHint::served(endpoint, now_ms));
        while hints.len() > MAX_HINTS {
            let oldest = hints
                .iter()
                .enumerate()
                .min_by_key(|(index, hint)| (hint.last_success_ms, hint.first_seen_ms, *index))
                .map(|(index, _)| index);
            match oldest {
                Some(index) => {
                    hints.remove(index);
                }
                None => break,
            }
        }
    }

    /// Records that `endpoint` did not serve `ledger`, evicting the hint on the
    /// [`MAX_FAILURES`]th consecutive failure.
    ///
    /// Returns whether the hint is gone.
    pub fn record_failure(&mut self, ledger: LedgerId, endpoint: EndpointId) -> bool {
        let Some(hints) = self.ledgers.get_mut(&ledger) else {
            return false;
        };
        let Some(index) = hints.iter().position(|hint| hint.endpoint == endpoint) else {
            return false;
        };
        hints[index].failures = hints[index].failures.saturating_add(1);
        if hints[index].failures < MAX_FAILURES {
            return false;
        }
        hints.remove(index);
        if hints.is_empty() {
            self.ledgers.remove(&ledger);
        }
        true
    }

    /// Adds an endpoint hint for a ledger, stamped with the current clock.
    pub fn add_hint(&mut self, ledger: LedgerId, endpoint: EndpointId) {
        self.record_success(ledger, endpoint, crate::now_ms());
    }

    /// Endpoints known to hold `ledger` at `now_ms`, freshest success first.
    ///
    /// Aged-out hints are left out rather than removed: a read never rewrites
    /// the file. [`Peers::prune`] is what drops them, before a write.
    #[must_use]
    pub fn hints_at(&self, ledger: LedgerId, now_ms: u64) -> Vec<EndpointId> {
        let Some(hints) = self.ledgers.get(&ledger) else {
            return Vec::new();
        };
        let mut live: Vec<(usize, &PeerHint)> = hints
            .iter()
            .enumerate()
            .filter(|(_, hint)| hint.live(now_ms))
            .collect();
        // The freshest success is dialled first, because the dial budget of
        // proposal 006 section 5.2 asks 4 of the 8 a ledger may hold.
        live.sort_by(|(left_index, left), (right_index, right)| {
            right
                .last_success_ms
                .cmp(&left.last_success_ms)
                .then_with(|| left_index.cmp(right_index))
        });
        live.into_iter().map(|(_, hint)| hint.endpoint).collect()
    }

    /// Endpoints known to hold `ledger` now.
    #[must_use]
    pub fn hints(&self, ledger: LedgerId) -> Vec<EndpointId> {
        self.hints_at(ledger, crate::now_ms())
    }

    /// Whether a hint for `endpoint` is recorded against `ledger`, live or not.
    #[must_use]
    pub fn holds(&self, ledger: LedgerId, endpoint: EndpointId) -> bool {
        self.ledgers
            .get(&ledger)
            .is_some_and(|hints| hints.iter().any(|hint| hint.endpoint == endpoint))
    }

    /// Drops every hint that has gone [`HINT_MAX_AGE_MS`] without a success,
    /// and every ledger left with none.
    ///
    /// Returns whether anything was dropped.
    pub fn prune(&mut self, now_ms: u64) -> bool {
        let mut dropped = false;
        for hints in self.ledgers.values_mut() {
            let before = hints.len();
            hints.retain(|hint| !hint.aged_out(now_ms));
            dropped |= hints.len() != before;
        }
        let before = self.ledgers.len();
        self.ledgers.retain(|_, hints| !hints.is_empty());
        dropped || self.ledgers.len() != before
    }
}

#[cfg(test)]
mod tests {
    use mabel_core::IdentityId;

    use super::{HINT_MAX_AGE_MS, MAX_FAILURES, MAX_HINTS, PeerHint, Peers};

    fn endpoint(seed: u8) -> iroh_base::EndpointId {
        iroh_base::SecretKey::from_bytes(&[seed; 32]).public()
    }

    const NOW: u64 = 1_700_000_000_000;

    #[test]
    fn hints_are_unique_per_ledger() {
        let ledger = IdentityId::from_bytes([4u8; 32]);
        let mut peers = Peers::default();
        peers.record_success(ledger, endpoint(6), NOW);
        peers.record_success(ledger, endpoint(6), NOW + 1);
        assert_eq!(peers.hints_at(ledger, NOW + 1), [endpoint(6)]);
        assert!(
            peers
                .hints_at(IdentityId::from_bytes([5u8; 32]), NOW)
                .is_empty()
        );
    }

    #[test]
    fn round_trips_through_json_with_ledger_ids_as_keys() {
        let ledger = IdentityId::from_bytes([4u8; 32]);
        let mut peers = Peers::default();
        peers.record_success(ledger, endpoint(6), NOW);

        let json = serde_json::to_string(&peers).unwrap();
        assert!(json.contains(&ledger.to_string()), "{json}");
        assert!(json.contains("\"last_success_ms\":1700000000000"), "{json}");
        assert_eq!(serde_json::from_str::<Peers>(&json).unwrap(), peers);
    }

    /// A `peers.json` written before proposal 006 holds bare endpoint ids. It
    /// loads as hints with no history, so an existing home keeps its addresses.
    #[test]
    fn a_bare_string_loads_as_a_hint_with_no_timestamps() {
        let ledger = IdentityId::from_bytes([4u8; 32]);
        let json = format!(r#"{{"ledgers": {{"{ledger}": ["{}"]}}}}"#, endpoint(6));
        let peers = serde_json::from_str::<Peers>(&json).expect("the old shape loads");
        assert_eq!(peers.ledgers[&ledger], [PeerHint::undated(endpoint(6))]);
        assert_eq!(peers.hints_at(ledger, NOW), [endpoint(6)]);
        // No timestamp means no age, so an old file is not emptied by the
        // age-out the moment it is read.
        assert!(!peers.ledgers[&ledger][0].aged_out(NOW + HINT_MAX_AGE_MS * 4));

        // The first write puts it in the new shape.
        let mut peers = peers;
        peers.record_success(ledger, endpoint(6), NOW);
        let written = serde_json::to_string(&peers).unwrap();
        assert!(
            written.contains("\"first_seen_ms\":1700000000000"),
            "{written}"
        );
    }

    #[test]
    fn the_cap_evicts_the_oldest_success() {
        let ledger = IdentityId::from_bytes([4u8; 32]);
        let mut peers = Peers::default();
        for seed in 0..MAX_HINTS {
            peers.record_success(ledger, endpoint(seed as u8), NOW + seed as u64);
        }
        assert_eq!(peers.ledgers[&ledger].len(), MAX_HINTS);
        peers.record_success(ledger, endpoint(60), NOW + 100);
        assert_eq!(peers.ledgers[&ledger].len(), MAX_HINTS);
        assert!(!peers.holds(ledger, endpoint(0)), "the oldest is gone");
        assert!(peers.holds(ledger, endpoint(60)));
    }

    #[test]
    fn a_hint_with_no_success_in_thirty_days_is_dropped() {
        let ledger = IdentityId::from_bytes([4u8; 32]);
        let mut peers = Peers::default();
        peers.record_success(ledger, endpoint(6), NOW);
        let later = NOW + HINT_MAX_AGE_MS + 1;
        assert!(peers.hints_at(ledger, later).is_empty());
        assert!(peers.prune(later));
        assert!(peers.ledgers.is_empty(), "{peers:?}");
    }

    #[test]
    fn three_failures_evict_and_one_success_resets_the_count() {
        let ledger = IdentityId::from_bytes([4u8; 32]);
        let mut peers = Peers::default();
        peers.record_success(ledger, endpoint(6), NOW);
        for _ in 0..MAX_FAILURES - 1 {
            assert!(!peers.record_failure(ledger, endpoint(6)));
        }
        peers.record_success(ledger, endpoint(6), NOW + 1);
        assert_eq!(peers.ledgers[&ledger][0].failures, 0);
        for _ in 0..MAX_FAILURES - 1 {
            assert!(!peers.record_failure(ledger, endpoint(6)));
        }
        assert!(
            peers.record_failure(ledger, endpoint(6)),
            "the third evicts"
        );
        assert!(!peers.holds(ledger, endpoint(6)));
    }

    #[test]
    fn an_unknown_field_is_a_load_error() {
        assert!(serde_json::from_str::<Peers>(r#"{"peers": {}}"#).is_err());
        assert_eq!(
            serde_json::from_str::<Peers>("{}").unwrap(),
            Peers::default()
        );
        let ledger = IdentityId::from_bytes([4u8; 32]);
        let json = format!(
            r#"{{"ledgers": {{"{ledger}": [{{"endpoint": "{}", "seen": 1}}]}}}}"#,
            endpoint(6)
        );
        let error = serde_json::from_str::<Peers>(&json).expect_err("unknown field");
        assert!(error.to_string().contains("seen"), "{error}");
    }
}
