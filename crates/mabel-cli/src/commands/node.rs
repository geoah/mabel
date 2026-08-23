//! `mabel node id` and `mabel node ticket`.

use std::net::{IpAddr, SocketAddr, UdpSocket};

use iroh_base::{EndpointAddr, TransportAddr};
use mabel_net::build_peer_ticket;

use crate::context::Context;
use crate::documents::{NodeId, NodeTicket};
use crate::error::{CliError, Result};
use crate::render::Outcome;

/// `mabel node id`: this node's Iroh endpoint id, base32 as every document
/// spells it.
pub fn id(ctx: &Context) -> Result<Outcome> {
    let endpoint_id = ctx.source()?;
    let text = endpoint_id.to_string();
    Outcome::new(&NodeId { endpoint_id }, text)
}

/// `mabel node ticket [--addr <ip:port>] [--port <port>]`.
///
/// The text rendering is the ticket and nothing else, so a script can pass
/// `--peer "$(mabel node ticket --addr 10.0.0.2:9070)"` straight through. The
/// addresses given by `--addr` come first, then the detected address `--port`
/// asks for; a repeated address is dropped.
///
/// # Errors
///
/// Returns code 60 for a group- or world-accessible `node.key`, and code 2
/// with reason `no_local_address` when `--port` was given and this host has no
/// routable non-loopback address to pair with it.
pub fn ticket(ctx: &Context, addrs: &[SocketAddr], port: Option<u16>) -> Result<Outcome> {
    let endpoint_id = ctx.endpoint_id()?;
    let mut socket_addrs = addrs.to_vec();
    if let Some(port) = port {
        let Some(detected) = detect_address() else {
            return Err(CliError::usage(
                "no_local_address",
                "this host has no routable non-loopback address; name one with --addr",
            )
            .with_detail("port", u64::from(port)));
        };
        socket_addrs.push(SocketAddr::new(detected, port));
    }
    socket_addrs.dedup();

    let addr = EndpointAddr {
        id: endpoint_id,
        addrs: socket_addrs
            .iter()
            .copied()
            .map(TransportAddr::Ip)
            .collect(),
    };
    let ticket = build_peer_ticket(addr);
    let document = NodeTicket {
        endpoint_id: ctx.source()?,
        addrs: socket_addrs
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        ticket: ticket.clone(),
    };
    Outcome::new(&document, ticket)
}

/// This host's own address on the route it would use to leave the machine.
///
/// A UDP `connect` sends no packet: it picks a route and binds a local
/// address, which is the address a peer on that route would see. 192.0.2.1 is
/// TEST-NET-1 (RFC 5737) and is never contacted. A host with no default route,
/// which is what the `compose.internal.yaml` overlay makes, has no answer here
/// and has to be told its address with `--addr`.
fn detect_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let local = socket.local_addr().ok()?.ip();
    (!local.is_unspecified() && !local.is_loopback()).then_some(local)
}
