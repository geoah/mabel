//! `mabel sync push|fetch`, the two commands that cross the network
//! (proposal 001 section 9).
//!
//! A push reports one row per witness, so a witness that did not answer is
//! visible rather than fatal; the command fails with code 30 only when no
//! witness accepted anything. A fetch verifies what the source served from
//! nothing before a byte of it is stored, and requires the chain's ledger id
//! to equal the one that was asked for, which is what makes an untrusted
//! source safe to fetch from (proposal 001 section 3.7).

use std::sync::Arc;

use mabel_node::api::documents::{Binding, PushStatus};
use mabel_node::graph::{Resolution, SourceClass, plan_sources};
use mabel_node::verification::{HickoryResolver, Resolver, caller_zone, query_name};
use mabel_node::wallet::{Fetched, WalletCore, WalletSync};

use crate::context::Context;
use crate::documents::{FetchedLedger, PushedLedger};
use crate::error::{CliError, Result};
use crate::ids;
use crate::network::on_network;
use crate::render::Outcome;

/// `mabel sync push --identity <alias|id> [--to <endpoint id>] [--peer
/// <ticket>]`.
///
/// # Errors
///
/// Returns code 2 when the ledger names no witness and none is pinned, code 30
/// when no witness accepted the push, and code 20 when one rejected it.
pub fn push(
    ctx: &Context,
    identity: &str,
    to: Option<&str>,
    tickets: &[String],
) -> Result<Outcome> {
    // A push signs, so the subject is local and a link's hints have nothing to
    // reach: they are dropped with a warning naming the flag (proposal 006
    // section 7).
    let identity = ctx.resolve_local_hinted(identity, "--identity")?;
    // Which witnesses to push to is decided before an endpoint is bound: a
    // ledger that names none has nothing to dial.
    // `--to` is an endpoint a person named for this push, so it is dialled and
    // never written to `peers.json` (proposal 006 section 5.3).
    let (witnesses, caller) = match to.map(ids::parse_endpoint).transpose()? {
        Some(endpoint) => (vec![endpoint], vec![endpoint]),
        None => (
            WalletCore::new(ctx.home().clone()).witnesses_of(identity)?,
            Vec::new(),
        ),
    };
    if witnesses.is_empty() {
        return Err(CliError::usage(
            "no_witness_configured",
            format!("no endpoint is configured to push {identity} to"),
        )
        .with_detail("ledger_id", identity.to_string()));
    }
    let pushed = on_network(
        ctx,
        tickets,
        |core: WalletCore, sync: WalletSync| async move {
            Ok(sync.push_from(&core, identity, &witnesses, &caller).await?)
        },
    )?;

    let accepted = pushed
        .results
        .iter()
        .filter(|result| result.status == PushStatus::Accepted)
        .count();
    let text = std::iter::once(format!(
        "{} at seq {}, {accepted} of {} witnesses accepted",
        pushed.ledger_id,
        pushed.head_seq,
        pushed.results.len()
    ))
    .chain(pushed.results.iter().map(|result| {
        let status = match result.status {
            PushStatus::Accepted => format!("accepted, stored {}", result.stored),
            PushStatus::Rejected => format!(
                "rejected {} at seq {}",
                result.reject_code.as_deref().unwrap_or("unknown"),
                result.at_seq.unwrap_or_default()
            ),
            PushStatus::Unreachable => "unreachable".to_owned(),
        };
        match &result.message {
            Some(message) => format!("{} {status}: {message}", result.endpoint),
            None => format!("{} {status}", result.endpoint),
        }
    }))
    .collect::<Vec<String>>()
    .join("\n");

    // A hinted endpoint is one no witness identity's ledger confirms, which is
    // a warning and never a refusal: the first push to a new witness always
    // happens before this home holds that witness's ledger (proposal 006
    // section 4.2).
    for result in &pushed.results {
        if result.status == PushStatus::Accepted && result.binding == Binding::Hinted {
            eprintln!(
                "warning: nobody's ledger confirms that {} answers for a witness of {identity}",
                result.endpoint
            );
        }
    }

    if accepted == 0 {
        return Err(CliError::network(
            "all_witnesses_failed",
            format!("no configured witness accepted the push for {identity}"),
        )
        .with_detail("ledger_id", identity.to_string())
        .with_detail("results", &pushed.results));
    }
    Outcome::new(
        &PushedLedger {
            identity_id: ids::identity(identity),
            pushed,
        },
        text,
    )
}

/// Where one fetch may read from: at most one of the three keys.
#[derive(Debug, Clone, Copy, Default)]
pub struct Source<'a> {
    /// `--from`, one endpoint id.
    pub from: Option<&'a str>,
    /// `--from-witness`, one witness identity.
    pub witness: Option<&'a str>,
    /// `--from-host`, one hostname.
    pub host: Option<&'a str>,
}

/// `mabel sync fetch <ledger id> [--from <endpoint id>] [--from-witness
/// <mabel id>] [--from-host <hostname>] [--peer <ticket>]`.
///
/// `--from` is a plain `CallerHint`: the endpoint a human named for this fetch,
/// asked whether or not this home has heard of it (proposal 006 section 5,
/// source 2). `--from-witness` names a witness identity instead and is resolved
/// to endpoints through section 5.1. `--from-host` names a hostname and is
/// resolved to endpoints through its `mabel-endpoints=` records, which are
/// `DnsEndpoint` sources for this fetch (section 6). With none of the three,
/// the link's hints are asked, and then every source of section 5 in order.
///
/// # Errors
///
/// Returns code 2 with reason `conflicting_source` when two keys are given,
/// `unresolvable_witness` when `--from-witness` names a witness this home can
/// reach no endpoint for, `malformed_hostname` for a `--from-host` value that
/// is not a hostname and `unresolvable_hostname` for a zone that names no
/// machine, code 30 when no source could be reached or none holds the ledger,
/// code 20 when what one served does not verify, and code 50 when this home
/// already holds a different event at some position.
pub fn fetch(
    ctx: &Context,
    ledger_id: &str,
    source: Source<'_>,
    tickets: &[String],
) -> Result<Outcome> {
    let Source {
        from,
        witness: from_witness,
        host: from_host,
    } = source;
    if from.is_some() && from_witness.is_some() {
        return Err(CliError::usage(
            "conflicting_source",
            "--from names an endpoint and --from-witness names an identity: give one",
        )
        .with_detail("parameter", "--from-witness"));
    }
    if from_host.is_some() && (from.is_some() || from_witness.is_some()) {
        return Err(CliError::usage(
            "conflicting_source",
            "--from-host names a hostname, --from an endpoint and --from-witness an identity: \
             give one",
        )
        .with_detail("parameter", "--from-host"));
    }
    // The hostname is checked before anything binds an endpoint: a name that
    // could not be claimed by any profile is a typo, not a lookup.
    let hostname = from_host.map(ids::parse_hostname).transpose()?;
    // The subject of a fetch is the thing being fetched, which is the one row
    // of the matrix where a link's hints apply (proposal 006 section 7).
    let (ledger, hints) = ctx.resolve_hinted(ledger_id)?;
    let core = WalletCore::new(ctx.home().clone());
    // `None` means the sources are the zone's, read once the runtime is up.
    let planned: Option<Vec<iroh_base::EndpointId>> = match (from, from_witness, &hostname) {
        (Some(from), _, _) => {
            warn_ignored_hints(&hints, "--from says which peer to ask");
            Some(vec![ids::parse_endpoint(from)?])
        }
        (None, Some(witness), _) => {
            let witness = ctx.resolve(witness)?;
            let resolution = Resolution::for_operation().with_caller_hints(hints);
            let endpoints = resolution.witness_endpoints(&core, witness)?;
            if endpoints.is_empty() {
                return Err(CliError::usage(
                    "unresolvable_witness",
                    format!("no endpoint is known for the witness {witness}"),
                )
                .with_detail("witness", witness.to_string())
                .with_detail("endpoints_tried", Vec::<String>::new()));
            }
            Some(endpoints)
        }
        (None, None, Some(_)) => {
            warn_ignored_hints(&hints, "--from-host says which zone to ask");
            None
        }
        (None, None, None) => {
            // Every source of section 5, the link's hints first as source 2.
            let resolution = Resolution::for_operation().with_caller_hints(hints);
            let planned = plan_sources(&core, ledger, &[], &resolution)?;
            let sources: Vec<iroh_base::EndpointId> = planned
                .iter()
                .filter_map(|planned| planned.endpoint)
                .collect();
            if sources.is_empty() {
                return Err(CliError::usage(
                    "missing_argument",
                    "sync fetch needs --from <endpoint id>, --from-witness <mabel id>, \
                     --from-host <hostname>, a link that names one, or a configured default \
                     witness",
                ));
            }
            Some(sources)
        }
    };
    let fetched: Fetched = on_network(
        ctx,
        tickets,
        |core: WalletCore, sync: WalletSync| async move {
            let sources = match planned {
                Some(sources) => sources,
                None => {
                    let hostname = hostname.expect("--from-host named a hostname");
                    host_sources(system_resolver()?.as_ref(), &hostname).await?
                }
            };
            // The hints are tried in the order the link named them, and only a
            // source that could not be reached moves on to the next: a chain that
            // does not verify is the answer, not a reason to ask elsewhere.
            let mut last: Option<CliError> = None;
            for source in sources {
                match sync.fetch(&core, ledger, source).await {
                    Ok(fetched) => return Ok(fetched),
                    Err(error) => {
                        let error = CliError::from(error);
                        if error.exit_code() != 30 {
                            return Err(error);
                        }
                        last = Some(error);
                    }
                }
            }
            Err(last.expect("at least one source was tried"))
        },
    )?;

    let document = FetchedLedger {
        ledger_id: ids::identity(ledger),
        source: ids::key(&fetched.source),
        event_count: fetched.event_count,
        stored: fetched.stored,
        head_seq: fetched.head_seq,
        head_event: ids::event(fetched.head_event),
        fetched_at_ms: fetched.fetched_at_ms,
        controlled_by: fetched.controlled_by.map(ids::identity),
    };
    let mut text = format!(
        "fetched {} events of {} from {}, stored {}\nhead seq {}, verified from nothing",
        document.event_count,
        document.ledger_id,
        document.source,
        document.stored,
        document.head_seq
    );
    // Whether this home may append to what it just stored is the one thing a
    // fetch decides beyond the bytes (ticket 031).
    match &document.controlled_by {
        Some(controller) => text.push_str(&format!(
            "\nthis home may append to it, signing as {controller}"
        )),
        None => text.push_str("\nstored read-only: no identity here controls it"),
    }
    Outcome::new(&document, text)
}

/// Says on stderr that a link's endpoints were dropped, because a flag named
/// the source instead (proposal 006 section 7).
fn warn_ignored_hints(hints: &[iroh_base::EndpointId], because: &str) {
    if hints.is_empty() {
        return;
    }
    eprintln!(
        "warning: ignoring the {} the link names: {because}",
        if hints.len() == 1 {
            "endpoint"
        } else {
            "endpoints"
        }
    );
}

/// The machines `--from-host` names: the `mabel-endpoints=` records at
/// `_mabel.<hostname>.`, read under row 1 of the applicability matrix
/// (proposal 006 section 6).
///
/// The caller typed this hostname for this fetch, so the response may yield
/// both an identity and the endpoints beside it, and the endpoints are read out
/// only when the same response resolved to one. The identity a zone names need
/// not be the ledger being fetched: one machine answers for many ledgers, and a
/// hint authorizes nothing, since the chain it serves is verified from nothing
/// and its ledger id must equal the one that was asked for.
///
/// Each endpoint is charged to the operation's `Dns` class, so a zone naming
/// more than the class cap spends no more than its share of the 16 dials of
/// section 5.2.
///
/// # Errors
///
/// Returns code 30 with reason `hostname_unreachable` when the query did not
/// answer, and code 2 with reason `unresolvable_hostname` when the zone names
/// no machine for a mabel identity.
async fn host_sources(
    resolver: &dyn Resolver,
    hostname: &str,
) -> Result<Vec<iroh_base::EndpointId>> {
    let name = query_name(hostname);
    let records = resolver.lookup_txt(&name).await.map_err(|error| {
        CliError::network(
            "hostname_unreachable",
            format!("the records at {name} could not be read: {error}"),
        )
        .with_detail("hostname", hostname)
        .with_detail("error", error.to_string())
    })?;
    let zone = caller_zone(&records);
    let resolution = Resolution::for_operation();
    let sources: Vec<iroh_base::EndpointId> = zone
        .endpoints
        .into_iter()
        .filter(|endpoint| resolution.admit(SourceClass::Dns, *endpoint))
        .collect();
    if sources.is_empty() {
        return Err(CliError::usage(
            "unresolvable_hostname",
            format!("{name} names no machine for a mabel identity"),
        )
        .with_detail("hostname", hostname)
        .with_detail("endpoints_tried", Vec::<String>::new()));
    }
    Ok(sources)
}

/// The DNS resolver `--from-host` queries, built from the system
/// configuration.
///
/// A machine with no resolver cannot answer a `--from-host` fetch at all, which
/// is a network failure and not a bad command line.
///
/// # Errors
///
/// Returns code 30 with reason `resolver_unavailable`.
fn system_resolver() -> Result<Arc<dyn Resolver>> {
    match HickoryResolver::system() {
        Ok(resolver) => Ok(Arc::new(resolver)),
        Err(error) => Err(CliError::network(
            "resolver_unavailable",
            format!("this machine has no DNS resolver: {error}"),
        )
        .with_detail("error", error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use mabel_node::verification::{StubResolver, TxtRecord};

    use super::host_sources;

    /// One rendered id from a fixed seed: an id in a TXT record has to be a
    /// real public key, since the parser decodes the point.
    fn id(seed: u8) -> String {
        crate::ids::key(&iroh_base::SecretKey::from_bytes(&[seed; 32]).public())
            .as_str()
            .to_owned()
    }

    /// `--from-host` reads the endpoints the zone names for the identity the
    /// same response resolved to, and queries the absolute label.
    #[tokio::test]
    async fn a_zone_that_names_an_identity_yields_its_endpoints() {
        let (ledger, one, two) = (id(1), id(2), id(3));
        let resolver = StubResolver::new().with_records(
            "_mabel.mabel.example.",
            vec![
                TxtRecord::from_strings([format!("mabel={ledger}")]),
                TxtRecord::from_strings([format!("mabel-endpoints={one},{two}")]),
            ],
        );

        let sources = host_sources(&resolver, "mabel.example")
            .await
            .expect("the zone names two machines");
        let rendered: Vec<String> = sources
            .iter()
            .map(|endpoint| crate::ids::key(endpoint).as_str().to_owned())
            .collect();
        assert_eq!(rendered.len(), 2, "{rendered:?}");
        assert!(rendered.contains(&one), "{rendered:?}");
        assert!(rendered.contains(&two), "{rendered:?}");
        assert_eq!(resolver.queries(), vec!["_mabel.mabel.example.".to_owned()]);
    }

    /// Row 1 of the applicability matrix: a label that resolved to no identity
    /// has no identity to offer endpoints for.
    #[tokio::test]
    async fn a_zone_with_no_mabel_record_names_no_machine() {
        let resolver = StubResolver::new().with_text(
            "_mabel.mabel.example.",
            &format!("mabel-endpoints={}", id(2)),
        );

        let error = host_sources(&resolver, "mabel.example")
            .await
            .expect_err("no identity, no endpoints");
        assert_eq!(error.exit_code(), 2);
        assert_eq!(
            error.to_document()["details"]["reason"],
            "unresolvable_hostname"
        );
    }

    /// A resolver that could not answer is code 30, not a refusal of the name.
    #[tokio::test]
    async fn a_query_that_times_out_is_a_network_failure() {
        let resolver = StubResolver::new().with_timeout("_mabel.mabel.example.");

        let error = host_sources(&resolver, "mabel.example")
            .await
            .expect_err("the query timed out");
        assert_eq!(error.exit_code(), 30);
        assert_eq!(
            error.to_document()["details"]["reason"],
            "hostname_unreachable"
        );
    }
}
