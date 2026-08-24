//! DNS hostname verification and its cache (proposal 003 section 2).
//!
//! An identity claims a hostname on its ledger; this module checks the TXT
//! record at `_mabel.<hostname>` for `mabel=<identity id>` and caches the
//! verdict under `verification/<identity_id>.json`. The verdict is advisory:
//! it never gates ledger validity (decision 015), and it is the wallet's job
//! alone, since a witness holds no user context and a crawling resolver is a
//! signal a witness should not emit.
//!
//! Nothing here dials out on a timer. Ticket 026 wires the routes: the
//! single-identity GET answers from cache and may start one background
//! refresh when [`should_refresh`] says so, the list route is cache-only, and
//! the forced check waits.
//!
//! ```no_run
//! use mabel_node::verification::{HickoryResolver, VerificationStore, verify_hostname};
//!
//! # async fn check(home: &mabel_node::NodeHome, identity: mabel_core::IdentityId)
//! # -> Result<(), Box<dyn std::error::Error>> {
//! let resolver = HickoryResolver::system()?;
//! let outcome = verify_hostname(&resolver, "alice.example", identity).await;
//! VerificationStore::new(home).record(identity, &outcome, mabel_node::now_ms())?;
//! # Ok(())
//! # }
//! ```

mod cache;
mod resolver;
mod verify;

pub use cache::{
    FRESH_FOR_MS, UnreachableCheck, VERIFICATION_DIR, VerificationEntry, VerificationStore, merge,
    should_refresh,
};
pub use resolver::{
    HickoryResolver, ResolveError, ResolveFuture, Resolver, StubResolver, TxtRecord,
};
pub use verify::{
    MAX_CNAME_LINKS, MAX_LABEL_ENDPOINTS, TXT_ENDPOINTS_PREFIX, TXT_LABEL, TXT_PREFIX,
    VerificationOutcome, VerificationStatus, check_hostname, endpoints_at_label, endpoints_claim,
    endpoints_for_claim, mabel_claim, query_name, verify_hostname,
};
