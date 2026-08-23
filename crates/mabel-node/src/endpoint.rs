//! Binding the Iroh endpoint a node speaks `mabel/ledger/0` over.
//!
//! This wraps [`mabel_net::bind_endpoint`], which takes no bind port: an
//! `--iroh-port` is what makes a node reachable at a fixed address in the
//! compose topology. Both runtimes call it, so the wallet and the witness bind
//! the same way.
//!
//! Addresses from `--peer` tickets are seeded into the lookup as address hints
//! and never as authorization (proposal 001 section 4).

use std::net::{Ipv4Addr, SocketAddr};

use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr, RelayMode as IrohRelayMode};
use iroh_base::SecretKey;

use crate::config::RelayMode;

/// Binds an endpoint with `secret` under `relay`, on `port` when one is asked
/// for.
///
/// # Errors
///
/// Returns an error when `port` is not bindable or the UDP socket cannot be
/// opened.
pub async fn bind_endpoint(
    relay: RelayMode,
    secret: SecretKey,
    port: Option<u16>,
    peers: &[EndpointAddr],
) -> anyhow::Result<Endpoint> {
    let lookup = MemoryLookup::new();
    for addr in peers {
        lookup.add_endpoint_info(addr.clone());
    }
    let mut builder = match relay {
        RelayMode::N0 => Endpoint::builder(presets::N0),
        RelayMode::Disabled => {
            Endpoint::builder(presets::Minimal).relay_mode(IrohRelayMode::Disabled)
        }
    };
    builder = builder.address_lookup(lookup).secret_key(secret);
    if let Some(port) = port {
        let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
        builder = builder
            .clear_ip_transports()
            .bind_addr(address)
            .map_err(|error| anyhow::anyhow!("{address} is not a bindable address: {error}"))?;
    }
    builder
        .bind()
        .await
        .map_err(|error| anyhow::anyhow!("the Iroh endpoint could not bind: {error}"))
}
