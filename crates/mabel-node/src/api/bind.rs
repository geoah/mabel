//! Binding the HTTP listener, and the warning for a bind that is not
//! loopback.
//!
//! Both roles bind `127.0.0.1` by default and there is no authentication, so
//! any other bind hands the node to whoever can reach the port (proposal 001
//! section 10). That is allowed, and it warns.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tokio::net::TcpListener;

/// Where both roles bind unless told otherwise.
///
/// Kept here so the api layer needs nothing from `node.json`; it is the same
/// address as `crate::DEFAULT_HTTP_BIND`.
pub const DEFAULT_HTTP_BIND: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_HTTP_PORT);

/// Port of [`DEFAULT_HTTP_BIND`].
pub const DEFAULT_HTTP_PORT: u16 = 9080;

/// A bound listener and what the runtime should say about it.
#[derive(Debug)]
pub struct HttpBind {
    /// The listener to hand to `axum::serve`.
    pub listener: TcpListener,
    /// The address that was actually bound, with port 0 resolved.
    pub address: SocketAddr,
    /// The line to warn with, `None` for a loopback bind.
    pub warning: Option<String>,
}

/// The warning for `address`, or `None` when it is loopback.
#[must_use]
pub fn non_loopback_warning(address: SocketAddr) -> Option<String> {
    if address.ip().is_loopback() {
        return None;
    }
    Some(format!(
        "the HTTP API is bound to {address}, which is not loopback: it has no authentication, so anyone who can reach that address can use the keys of this node"
    ))
}

/// Binds the HTTP listener, warning when the address is not loopback.
///
/// The warning goes to `tracing` and comes back in [`HttpBind::warning`] so a
/// runtime can also print it.
///
/// # Errors
///
/// Returns the bind error: the port is taken, or the address is not local.
pub async fn bind(address: SocketAddr) -> std::io::Result<HttpBind> {
    let warning = non_loopback_warning(address);
    if let Some(warning) = &warning {
        tracing::warn!("{warning}");
    }
    let listener = TcpListener::bind(address).await?;
    let address = listener.local_addr()?;
    Ok(HttpBind {
        listener,
        address,
        warning,
    })
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_HTTP_BIND, DEFAULT_HTTP_PORT, bind, non_loopback_warning};

    #[test]
    fn the_default_bind_is_loopback_and_warns_about_nothing() {
        assert!(DEFAULT_HTTP_BIND.ip().is_loopback());
        assert_eq!(DEFAULT_HTTP_BIND.port(), DEFAULT_HTTP_PORT);
        assert!(non_loopback_warning(DEFAULT_HTTP_BIND).is_none());
    }

    #[test]
    fn any_address_that_is_not_loopback_warns() {
        for address in ["0.0.0.0:9080", "192.168.1.10:9080", "[::]:9080"] {
            let warning = non_loopback_warning(address.parse().unwrap())
                .unwrap_or_else(|| panic!("{address} is not loopback"));
            assert!(warning.contains(address), "{warning}");
            assert!(warning.contains("no authentication"), "{warning}");
        }
        assert!(non_loopback_warning("[::1]:9080".parse().unwrap()).is_none());
    }

    #[tokio::test]
    async fn binding_loopback_resolves_the_port_and_stays_quiet() {
        let bound = bind("127.0.0.1:0".parse().unwrap())
            .await
            .expect("loopback binds");
        assert!(bound.warning.is_none());
        assert_ne!(bound.address.port(), 0);
        assert!(bound.address.ip().is_loopback());
    }

    #[tokio::test]
    async fn binding_a_wildcard_address_carries_the_warning() {
        let Ok(bound) = bind("0.0.0.0:0".parse().unwrap()).await else {
            // A sandbox that refuses the wildcard bind still exercises the
            // warning through the pure function above.
            return;
        };
        assert!(bound.warning.is_some(), "a wildcard bind must warn");
    }
}
