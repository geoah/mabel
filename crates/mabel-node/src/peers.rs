//! `peers.json`: where to look for a ledger (proposal 001 section 8).
//!
//! Address hints and tickets, never authorization: a peer that hands over a
//! ledger is still checked against the chain (proposal 001 section 4).

use std::collections::BTreeMap;

use iroh_base::EndpointId;
use mabel_core::LedgerId;
use serde::{Deserialize, Serialize};

/// The contents of `peers.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peers {
    /// Endpoints known to hold a given ledger.
    #[serde(default)]
    pub ledgers: BTreeMap<LedgerId, Vec<EndpointId>>,
    /// `EndpointTicket` strings seeded by `--peer` or by the compose
    /// topology, which carry addresses for endpoints the relays cannot find.
    #[serde(default)]
    pub tickets: Vec<String>,
}

impl Peers {
    /// Adds an endpoint hint for a ledger, keeping the list unique.
    pub fn add_hint(&mut self, ledger: LedgerId, endpoint: EndpointId) {
        let hints = self.ledgers.entry(ledger).or_default();
        if !hints.contains(&endpoint) {
            hints.push(endpoint);
        }
    }

    /// Endpoints known to hold `ledger`.
    #[must_use]
    pub fn hints(&self, ledger: LedgerId) -> &[EndpointId] {
        self.ledgers.get(&ledger).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use mabel_core::IdentityId;

    use super::Peers;

    #[test]
    fn hints_are_unique_per_ledger() {
        let ledger = IdentityId::from_bytes([4u8; 32]);
        let endpoint = iroh_base::SecretKey::from_bytes(&[6u8; 32]).public();
        let mut peers = Peers::default();
        peers.add_hint(ledger, endpoint);
        peers.add_hint(ledger, endpoint);
        assert_eq!(peers.hints(ledger), [endpoint]);
        assert!(peers.hints(IdentityId::from_bytes([5u8; 32])).is_empty());
    }

    #[test]
    fn round_trips_through_json_with_ledger_ids_as_keys() {
        let ledger = IdentityId::from_bytes([4u8; 32]);
        let mut peers = Peers::default();
        peers.add_hint(
            ledger,
            iroh_base::SecretKey::from_bytes(&[6u8; 32]).public(),
        );
        peers.tickets.push("endpointticket-placeholder".to_string());

        let json = serde_json::to_string(&peers).unwrap();
        assert!(json.contains(&ledger.to_string()), "{json}");
        assert_eq!(serde_json::from_str::<Peers>(&json).unwrap(), peers);
    }

    #[test]
    fn an_unknown_field_is_a_load_error() {
        assert!(serde_json::from_str::<Peers>(r#"{"peers": {}}"#).is_err());
        assert_eq!(
            serde_json::from_str::<Peers>("{}").unwrap(),
            Peers::default()
        );
    }
}
