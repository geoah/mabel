//! Binding an Iroh endpoint for mabel.
//!
//! Two relay settings, matching `node.json` (proposal 001, clarifications):
//! [`RelayChoice::N0`] uses n0's relays and DNS, so a peer can be dialled by
//! `EndpointId` alone, and [`RelayChoice::Disabled`] uses no relays and no
//! published discovery, so peers must be reachable directly or through a
//! seeded `EndpointTicket`. The compose topology sets the second, which is
//! what makes the container suite runnable with no internet.
//!
//! Tickets from `--peer` are parsed with [`parse_peer_ticket`] and seeded into
//! a [`MemoryLookup`], iroh 1.x's replacement for `Endpoint::add_node_addr`.
//! A ticket is an address hint and never authorization (proposal 001
//! section 4).

use iroh::Endpoint;
use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::{BindError, presets};
use iroh_base::{EndpointAddr, SecretKey};
use iroh_tickets::Ticket;
use iroh_tickets::endpoint::EndpointTicket;

/// Whether the endpoint uses the n0 relays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelayChoice {
    /// n0's default relays, DNS lookup and pkarr publishing, which is what
    /// lets a witness be reached by `EndpointId` alone.
    #[default]
    N0,
    /// No relays and no published discovery.
    Disabled,
}

/// How to bind an endpoint.
#[derive(Debug, Default)]
pub struct EndpointConfig {
    /// The relay setting from `node.json`.
    pub relay: RelayChoice,
    /// The node's identity key. A fresh key means a new `EndpointId`, so
    /// callers persist it.
    pub secret_key: Option<SecretKey>,
    /// Addresses to seed into the lookup, usually from `--peer` tickets.
    pub peers: Vec<EndpointAddr>,
}

impl EndpointConfig {
    /// A config for one relay setting.
    pub fn new(relay: RelayChoice) -> Self {
        Self {
            relay,
            ..Self::default()
        }
    }

    /// Sets the identity key.
    #[must_use]
    pub fn with_secret_key(mut self, secret_key: SecretKey) -> Self {
        self.secret_key = Some(secret_key);
        self
    }

    /// Adds an address hint.
    #[must_use]
    pub fn with_peer(mut self, addr: EndpointAddr) -> Self {
        self.peers.push(addr);
        self
    }
}

/// A bound endpoint and the lookup that holds its seeded addresses.
#[derive(Debug, Clone)]
pub struct BoundEndpoint {
    /// The endpoint.
    pub endpoint: Endpoint,
    /// The seeded addresses. More can be added at runtime with
    /// `MemoryLookup::add_endpoint_info`.
    pub lookup: MemoryLookup,
}

/// Reads an `EndpointTicket` string, as `--peer` takes.
///
/// # Errors
///
/// Returns the parse error if the string is not an `endpoint` ticket.
pub fn parse_peer_ticket(ticket: &str) -> Result<EndpointAddr, iroh_tickets::ParseError> {
    Ok(ticket.parse::<EndpointTicket>()?.into())
}

/// Writes an `EndpointTicket` string, the sibling of [`parse_peer_ticket`].
///
/// This is what `mabel node ticket` prints and what a peer passes back as
/// `--peer`. An address in the ticket is a hint and never authorization
/// (proposal 001 section 4), so a ticket carrying no address at all is valid:
/// it names the endpoint and leaves the relays to find it.
#[must_use]
pub fn build_peer_ticket(addr: EndpointAddr) -> String {
    EndpointTicket::new(addr).encode_string()
}

/// Binds an endpoint per `config`.
///
/// # Errors
///
/// Returns the iroh bind error, for example when the UDP socket cannot be
/// opened.
pub async fn bind_endpoint(config: EndpointConfig) -> Result<BoundEndpoint, BindError> {
    let lookup = MemoryLookup::new();
    for addr in &config.peers {
        lookup.add_endpoint_info(addr.clone());
    }

    let mut builder = match config.relay {
        RelayChoice::N0 => Endpoint::builder(presets::N0),
        RelayChoice::Disabled => {
            Endpoint::builder(presets::Minimal).relay_mode(iroh::RelayMode::Disabled)
        }
    };
    builder = builder.address_lookup(lookup.clone());
    if let Some(secret_key) = config.secret_key {
        builder = builder.secret_key(secret_key);
    }
    let endpoint = builder.bind().await?;
    Ok(BoundEndpoint { endpoint, lookup })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh_base::{EndpointId, TransportAddr};

    fn addr() -> EndpointAddr {
        let id: EndpointId = SecretKey::from_bytes(&[5u8; 32]).public();
        EndpointAddr {
            id,
            addrs: [TransportAddr::Ip("127.0.0.1:4433".parse().unwrap())]
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn a_peer_ticket_round_trips() {
        let ticket = EndpointTicket::new(addr()).encode_string();
        assert_eq!(
            parse_peer_ticket(&ticket).expect("the ticket parses"),
            addr()
        );
    }

    #[test]
    fn a_built_ticket_parses_back_to_the_address_it_names() {
        let ticket = build_peer_ticket(addr());
        assert!(ticket.starts_with("endpoint"), "{ticket}");
        assert_eq!(parse_peer_ticket(&ticket).expect("it parses"), addr());
    }

    #[test]
    fn a_ticket_with_no_address_names_the_endpoint_alone() {
        let id: EndpointId = SecretKey::from_bytes(&[5u8; 32]).public();
        let ticket = build_peer_ticket(EndpointAddr::new(id));
        let parsed = parse_peer_ticket(&ticket).expect("it parses");
        assert_eq!(parsed.id, id);
        assert!(parsed.addrs.is_empty(), "{parsed:?}");
    }

    #[test]
    fn a_string_that_is_not_a_ticket_is_an_error() {
        assert!(parse_peer_ticket("endpointnope").is_err());
        assert!(parse_peer_ticket("").is_err());
    }

    #[tokio::test]
    async fn a_disabled_relay_endpoint_binds_with_a_seeded_peer() {
        let bound = bind_endpoint(
            EndpointConfig::new(RelayChoice::Disabled)
                .with_secret_key(SecretKey::from_bytes(&[11u8; 32]))
                .with_peer(addr()),
        )
        .await
        .expect("the endpoint binds");
        assert_eq!(
            bound.endpoint.id(),
            SecretKey::from_bytes(&[11u8; 32]).public()
        );
        bound.endpoint.close().await;
    }
}
