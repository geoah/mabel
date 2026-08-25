//! `mabel dev seed`: fill an empty home with data to develop and test against.
//!
//! Every event this writes is written by the command that owns it:
//! [`identity::create`], [`profile::replace`], [`trust::add`],
//! [`contact::set`], [`witness::add`] and [`witness::set_default`] are called
//! here exactly as `main` calls them, so a seeded ledger holds the same bytes a
//! person typing those command lines would have produced. There is no second
//! way to mint an identity and no fixture bytes anywhere: what the seed leaves
//! behind is real signed history, and `mabel verify ledger` accepts it.
//!
//! The three membership signatures are the one flow spelled out here rather
//! than called, because `mabel membership invite|accept|admit` hands each step
//! to the next one through a file (proposal 001 section 3.8) and a seed has no
//! reason to touch the filesystem. The artifacts are the same
//! [`IdentityDescriptor`], [`InvitationBundle`] and [`AcceptanceFile`] those
//! commands read and write, built and folded in memory, and the appends go
//! through [`append`] like every other append in this binary.
//!
//! Seeding refuses a home that already holds an identity. An event cannot be
//! taken back, so there is no `--force`: a home to seed again is a home to
//! delete.

use iroh_base::EndpointId;
use mabel_core::IdentityId;
use mabel_core::artifacts::{AcceptanceFile, ArtifactError, IdentityDescriptor, InvitationBundle};
use mabel_core::sign::{
    build_acceptance, build_membership_acceptance, build_membership_invitation,
};
use mabel_node::api::documents::{GraphStatus, Identity, PushStatus, Pushed};
use mabel_node::graph::{CrawlOptions, GraphStore, NetLedgerFetcher, crawl};
use mabel_node::now_ms;
use mabel_node::wallet::{Names, WalletCore, graph_status};

use crate::append::{append, ensure_fresh};
use crate::cli::{AppendOptions, Kind, RoleArg};
use crate::commands::{contact, identity, profile, trust, witness};
use crate::context::Context;
use crate::documents::SeededHome;
use crate::error::{CliError, Result};
use crate::ids;
use crate::network::{on_network, parse_peers};
use crate::render::Outcome;

/// One seeded person, as the two commands that publish a profile take them.
struct Person {
    /// The local label, which is also how the seed names it below.
    alias: &'static str,
    /// The name the identity publishes, `None` for an identity that publishes
    /// no profile at all.
    display_name: Option<&'static str>,
    /// The hostname it claims, published as a second `ProfileUpdate`.
    hostname: Option<&'static str>,
    /// The email it publishes.
    email: Option<&'static str>,
}

/// The three people. Carol publishes nothing, so a home always holds one
/// identity whose only event is its inception.
const PEOPLE: [Person; 3] = [
    Person {
        alias: "alice",
        display_name: Some("Alice Ashworth"),
        hostname: Some("alice.example"),
        email: Some("alice@alice.example"),
    },
    Person {
        alias: "bob",
        display_name: Some("Bob Baxter"),
        hostname: Some("bob.example"),
        email: Some("bob@bob.example"),
    },
    Person {
        alias: "carol",
        display_name: None,
        hostname: None,
        email: None,
    },
];

/// The organization alice founds.
const ORGANIZATION: &str = "acme";

/// The witness identity the seed creates.
///
/// A witness is an identity like any other (proposal 006 section 1): this one
/// advertises the seeding node's own endpoint, and every ledger the seed pushes
/// names it in its `WitnessSet`.
const WITNESS: &str = "witness";

/// The name the witness identity publishes, which `identity create` lands as a
/// `ProfileUpdate` at seq 1 like every other seeded name. It carries no word
/// from the protocol, because a wallet prints this name wherever it would
/// otherwise print the bare id.
const WITNESS_NAME: &str = "The Keeper";

/// The name it publishes.
const ORGANIZATION_NAME: &str = "Acme Corporation";

/// The identity admitted as a controller of the organization, which is the
/// membership flow of proposal 002 section 6 run once.
const CONTROLLER: &str = "bob";

/// Who attests whom, as `(issuer, subject)` aliases, in signing order. Trust is
/// one-way and lives in the issuer's ledger alone (decision 003), so all four
/// rows below are four separate events in four different chains.
const ATTESTATIONS: [(&str, &str); 4] = [
    ("alice", "bob"),
    ("bob", "carol"),
    ("bob", "alice"),
    ("acme", "bob"),
];

/// The private note the seed writes, as `(identity, nickname, note)`.
///
/// A contact note is a fact about this node, not about an identity in it:
/// `contacts/<identity_id>.json` is one file per subject for the whole home,
/// not one per local identity (proposal 003 section 1). So the seed writes one
/// note about bob, and every identity in the home reads the same one.
const CONTACT: (&str, &str, &str) = (CONTROLLER, "bob from the pub", "met at the meetup");

/// Every alias the seed creates, in creation order.
const ALIASES: [&str; 5] = ["alice", "bob", "carol", ORGANIZATION, WITNESS];

/// `mabel dev seed [--peer <ticket>] [--witness <alias|id>]`.
///
/// # Errors
///
/// Returns code 2 with reason `home_not_empty` when the home already holds an
/// identity, code 2 for a `--peer` value that is not an endpoint ticket and
/// code 2 for a `--witness` value that is neither an id nor a local alias,
/// plus whatever the commands it calls report.
pub fn seed(ctx: &Context, tickets: &[String], named: &[String]) -> Result<Outcome> {
    refuse_a_home_that_holds_an_identity(ctx)?;
    // A ticket or a witness id that does not parse stops the run before the
    // first key file is written, not after the third ledger.
    let endpoints: Vec<EndpointId> = parse_peers(tickets)?.iter().map(|peer| peer.id).collect();
    let mut given = Vec::with_capacity(named.len());
    for name in named {
        let witness = ctx.resolve(name)?;
        if !given.contains(&witness) {
            given.push(witness);
        }
    }
    let options = AppendOptions {
        no_sync: false,
        peer: tickets.to_vec(),
    };

    for person in &PEOPLE {
        identity::create(
            ctx,
            person.alias,
            Kind::Person,
            None,
            person.display_name,
            person.email,
        )?;
        // A hostname is a claim made after the fact: `identity create`
        // publishes the name and the email at seq 1, and the handle lands as
        // its own replacement at seq 2, which is the order a person meets.
        if let Some(hostname) = person.hostname {
            profile::replace(
                ctx,
                person.alias,
                profile::Fields::new(person.display_name, Some(hostname), person.email),
                true,
                // The diff and the confirmation belong to a person at a
                // terminal; the seed prints its own summary instead.
                true,
                &options,
            )?;
        }
    }
    identity::create(
        ctx,
        ORGANIZATION,
        Kind::Organization,
        Some(PEOPLE[0].alias),
        Some(ORGANIZATION_NAME),
        None,
    )?;

    admit_controller(ctx, ORGANIZATION, PEOPLE[0].alias, CONTROLLER, &options)?;

    for (issuer, subject) in ATTESTATIONS {
        trust::add(ctx, issuer, subject, &options)?;
    }

    let (subject, nickname, note) = CONTACT;
    contact::set(ctx, subject, Some(nickname), Some(note))?;

    // The witness identity, and the machine that answers for it: this node.
    // `--endpoints auto` reads `node.key`, so the advertisement names the
    // endpoint this home would serve reads on (proposal 006 section 2).
    identity::create(ctx, WITNESS, Kind::Service, None, Some(WITNESS_NAME), None)?;
    identity::replace_endpoints(ctx, WITNESS, "auto", &options)?;
    let witness_identity = ctx.resolve_local(WITNESS)?;

    let mut ledgers = Vec::with_capacity(ALIASES.len());
    for alias in ALIASES {
        ledgers.push(ctx.resolve_local(alias)?);
    }

    // The witness sets come last, so every append above ran against a ledger
    // that named none and needed no head query (proposal 001 section 5). They
    // are written only when a ticket was given, because a seed with nowhere to
    // push has nothing to say about who keeps its chains.
    let witnesses = if endpoints.is_empty() {
        Vec::new()
    } else {
        let configured = given.first().copied().unwrap_or(witness_identity);
        let mut naming = vec![witness_identity];
        naming.extend(given);
        let dialled: Vec<String> = endpoints.iter().map(ToString::to_string).collect();
        for ledger in &ledgers {
            for witness in &naming {
                // The ticket's machines are this call's hints: the second
                // witness a ledger names is added on a chain the first already
                // witnesses, and nothing on disk names a machine for it yet.
                witness::add(
                    ctx,
                    &ledger.to_string(),
                    &witness.to_string(),
                    &dialled,
                    &options,
                )?;
            }
        }
        // The ticket's endpoints answer for the witness identity named on the
        // command line when there is one, and for the seeded identity
        // otherwise: a bootstrap record says which machine answers for which
        // identity, and pairing the wrong two would configure a witness that
        // does nothing (proposal 006 section 5.4).
        witness::set_default(ctx, &configured.to_string(), &dialled)?;
        naming.iter().copied().map(ids::identity).collect()
    };
    let (pushed, graph) = if endpoints.is_empty() {
        (Vec::new(), None)
    } else {
        push_and_crawl(ctx, tickets, &ledgers)?
    };

    let mut identities = Vec::with_capacity(ledgers.len());
    for ledger in &ledgers {
        identities.push(ctx.identity_document(*ledger)?);
    }
    let document = SeededHome {
        identities,
        witnesses,
        pushed,
        graph,
    };
    let text = text(&document);
    Outcome::new(&document, text)
}

/// Refuses a home that already holds an identity.
///
/// # Errors
///
/// Returns code 2 with reason `home_not_empty`.
fn refuse_a_home_that_holds_an_identity(ctx: &Context) -> Result<()> {
    let held = ctx.home().identities()?;
    if held.is_empty() {
        return Ok(());
    }
    Err(CliError::usage(
        "home_not_empty",
        format!(
            "this home already holds {} {}; mabel dev seed only fills an empty home",
            held.len(),
            if held.len() == 1 {
                "identity"
            } else {
                "identities"
            }
        ),
    )
    .with_detail("identity_count", held.len()))
}

/// Admits `invitee` as a controller of `ledger`, running all three signatures
/// of proposal 002 section 6 with the artifacts crossing in memory.
///
/// # Errors
///
/// Returns whatever the two appends report, and code 2 when the invitee is an
/// identity-rooted ledger with no key of its own to be invited under.
fn admit_controller(
    ctx: &Context,
    ledger: &str,
    by: &str,
    invitee: &str,
    options: &AppendOptions,
) -> Result<()> {
    let ledger = ctx.resolve_local(ledger)?;
    let by = ctx.resolve_local(by)?;
    let invitee = ctx.resolve_local(invitee)?;

    // The invitee's descriptor, the artifact `mabel identity export` writes.
    let inception = ctx.store(invitee).read_event(0)?;
    let witnesses = ctx.load(invitee)?.state.witness_endpoints().to_vec();
    let descriptor = IdentityDescriptor::new(&inception, &witnesses)
        .map_err(|error| unbuildable("IdentityDescriptor", &error))?;
    let invitee_key = descriptor.active_key().ok_or_else(|| {
        CliError::policy(
            "invitee_holds_no_key",
            format!(
                "{} holds no key of its own, so it cannot be invited",
                ids::shown(invitee)
            ),
        )
        .with_detail("invitee", invitee.to_string())
    })?;

    // A controller appends the invitation.
    ensure_fresh(ctx, ledger, options)?;
    let mut loaded = ctx.load(ledger)?;
    append(ctx, by, &mut loaded, |key, at, timestamp_ms| {
        build_membership_invitation(
            key,
            at,
            invitee,
            &invitee_key,
            RoleArg::Controller.proto(),
            descriptor.inception(),
            timestamp_ms,
        )
    })?;

    // The invitee folds the ledger as it now stands and signs an acceptance of
    // the invitation it ends with. This is the second of the two signatures
    // decision 004 requires: nobody is added to a ledger without their own.
    let events = ctx
        .store(ledger)
        .read_all()?
        .into_iter()
        .map(|stored| stored.bytes)
        .collect();
    let bundle =
        InvitationBundle::new(events).map_err(|error| unbuildable("InvitationBundle", &error))?;
    let summary = bundle
        .summary()
        .map_err(|error| unbuildable("InvitationBundle", &error))?;
    let signer = ctx.signing_key(invitee)?;
    let signed = build_acceptance(&signer, summary.ledger, summary.invitation_event, invitee);
    let acceptance =
        AcceptanceFile::new(&signed).map_err(|error| unbuildable("AcceptanceFile", &error))?;

    // A controller appends the acceptance, which is what admits the principal.
    let detached = acceptance.detached();
    ensure_fresh(ctx, ledger, options)?;
    let mut loaded = ctx.load(ledger)?;
    append(ctx, by, &mut loaded, |key, at, timestamp_ms| {
        build_membership_acceptance(key, at, &detached, timestamp_ms)
    })?;
    Ok(())
}

/// Pushes every seeded ledger and runs one crawl, on one bound endpoint.
///
/// The crawl is what makes `mabel lookup` and the degrees in the wallet answer
/// at all: without a stored generation every lookup reports "no path in this
/// crawl" (proposal 003 section 3).
///
/// # Errors
///
/// Returns code 30 when the witness cannot be reached and code 20 when it
/// rejects a push.
fn push_and_crawl(
    ctx: &Context,
    tickets: &[String],
    ledgers: &[IdentityId],
) -> Result<(Vec<Pushed>, Option<GraphStatus>)> {
    let roots = ctx.home().identities()?;
    let pushing = ledgers.to_vec();
    let (pushed, generation) = on_network(ctx, tickets, |core, sync| async move {
        let mut pushed = Vec::with_capacity(pushing.len());
        for ledger in pushing {
            let witnesses = core.witnesses_of(ledger)?;
            pushed.push(sync.push(&core, ledger, &witnesses).await?);
        }
        let fetcher = NetLedgerFetcher::new(core, sync);
        Ok((pushed, crawl(&roots, &CrawlOptions::new(), &fetcher).await))
    })?;
    GraphStore::in_home(ctx.home()).publish(&generation)?;

    let core = WalletCore::new(ctx.home().clone());
    let graph = graph_status(
        &Names::new(&core, Some(&generation)),
        &generation.summary,
        now_ms(),
    );
    Ok((pushed, Some(graph)))
}

/// An artifact the seed built from its own bytes and could not read back,
/// which the types above do not permit.
fn unbuildable(artifact: &'static str, error: &ArtifactError) -> CliError {
    CliError::internal(
        "artifact_not_buildable",
        format!("the seed could not build a {artifact}: {error}"),
    )
    .with_detail("artifact", artifact)
}

/// What the seed created, as a person reads it.
fn text(document: &SeededHome) -> String {
    let identities = &document.identities;
    let mut lines = vec![format!("seeded {} identities", identities.len())];
    lines.extend(identities.iter().map(line));
    for identity in identities {
        for principal in &identity.principals {
            if !principal.is_root {
                lines.push(format!(
                    "{} is a {} of {}",
                    named(identities, principal.identity.as_str()),
                    principal.role.as_str(),
                    identity.alias
                ));
            }
        }
    }
    for identity in identities {
        for entry in &identity.trust {
            lines.push(format!(
                "{} attests {}",
                identity.alias,
                named(identities, entry.subject.as_str())
            ));
        }
    }
    for identity in identities {
        if let Some(contact) = &identity.contact {
            lines.push(format!(
                "a private note on {}: nickname {}, note {}",
                identity.alias,
                contact.nickname.as_deref().unwrap_or("(unset)"),
                contact.note.as_deref().unwrap_or("(unset)")
            ));
        }
    }
    for witness in &document.witnesses {
        lines.push(format!(
            "{} witnesses all {} ledgers and advertises this node's endpoint",
            ids::shown(witness),
            identities.len()
        ));
    }
    for pushed in &document.pushed {
        let accepted = pushed
            .results
            .iter()
            .filter(|result| result.status == PushStatus::Accepted)
            .count();
        lines.push(format!(
            "pushed {} at seq {}, {accepted} of {} witnesses accepted",
            named(identities, pushed.ledger_id.as_str()),
            pushed.head_seq,
            pushed.results.len()
        ));
    }
    match &document.graph {
        Some(graph) => lines.push(format!(
            "crawl {} at depth {}: {} nodes, {} edges",
            graph.sync_id, graph.depth, graph.node_count, graph.edge_count
        )),
        None => lines.push(
            "no witness was given, so nothing was pushed and no crawl ran; \
             mabel lookup reports no path until one does"
                .to_owned(),
        ),
    }
    lines.join("\n")
}

/// One identity as a line: what it is, and what it publishes.
fn line(identity: &Identity) -> String {
    let mut text = format!(
        "{} {} {}",
        identity.alias,
        ids::shown(&identity.identity_id),
        identity.declared_kind
    );
    match &identity.profile {
        Some(profile) => {
            for value in [
                profile.display_name.as_deref(),
                profile.hostname.as_deref(),
                profile.email.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                text.push_str(&format!(", {value}"));
            }
        }
        None => text.push_str(", publishes no profile"),
    }
    let count = identity.event_count;
    format!(
        "{text}, {count} {}",
        if count == 1 { "event" } else { "events" }
    )
}

/// The alias of a seeded identity, or the id when the seed did not create it.
///
/// The fallback is a bare id in a sentence of aliases, so it carries the prefix
/// that says what it is.
fn named(identities: &[Identity], identity: &str) -> String {
    identities
        .iter()
        .find(|held| held.identity_id.as_str() == identity)
        .map_or_else(|| ids::shown(identity), |held| held.alias.clone())
}
