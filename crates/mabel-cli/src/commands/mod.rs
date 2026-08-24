//! One module per command group, and the dispatch that reaches them.

pub mod contact;
pub mod graph;
pub mod identity;
pub mod membership;
pub mod node;
pub mod profile;
pub mod sync;
pub mod trust;
pub mod verify;
pub mod wallet_serve;
pub mod witness;
pub mod witness_run;

use crate::cli::{
    Cli, Command, ContactCommand, GraphCommand, IdentityCommand, MembershipCommand, NodeCommand,
    ProfileCommand, SyncCommand, TrustCommand, VerifyCommand, WalletCommand, WitnessCommand,
};
use crate::context::Context;
use crate::error::Result;
use crate::render::Outcome;

/// Runs the command the arguments name.
///
/// # Errors
///
/// Returns whatever the command reports; every failure carries its exit code.
pub fn run(cli: &Cli) -> Result<Outcome> {
    let ctx = Context::open(cli.home.as_deref(), cli.allow_insecure_permissions)?;
    let root = ctx.root();
    dispatch(&ctx, cli).map_err(|error| error.relative_to_home(&root))
}

fn dispatch(ctx: &Context, cli: &Cli) -> Result<Outcome> {
    match &cli.command {
        Command::Identity { command } => match command {
            IdentityCommand::Create {
                alias,
                kind,
                founder,
                name,
                email,
            } => identity::create(
                ctx,
                alias,
                *kind,
                founder.as_deref(),
                name.as_deref(),
                email.as_deref(),
            ),
            IdentityCommand::List => identity::list(ctx),
            IdentityCommand::Show { identity } => identity::show(ctx, identity),
            IdentityCommand::Export { identity, out } => identity::export(ctx, identity, out),
            IdentityCommand::Rotate { .. } => identity::rotate(),
        },
        Command::Membership { command } => match command {
            MembershipCommand::Invite {
                ledger,
                by,
                invitee,
                role,
                out,
                append,
            } => membership::invite(ctx, ledger, by, invitee, *role, out, append),
            MembershipCommand::Accept {
                bundle,
                identity,
                out,
                yes,
            } => membership::accept(ctx, bundle, identity, out, *yes, cli.json),
            MembershipCommand::Admit {
                ledger,
                by,
                acceptance,
                append,
            } => membership::admit(ctx, ledger, by, acceptance, append),
            MembershipCommand::Remove {
                ledger,
                by,
                member,
                append,
            } => membership::remove(ctx, ledger, by, member, append),
            MembershipCommand::List { ledger } => membership::list(ctx, ledger),
        },
        Command::Profile { command } => match command {
            ProfileCommand::Replace {
                identity,
                display_name,
                hostname,
                email,
                yes,
                append,
            } => profile::replace(
                ctx,
                identity,
                profile::Fields::new(
                    display_name.as_deref(),
                    hostname.as_deref(),
                    email.as_deref(),
                ),
                *yes,
                cli.json,
                append,
            ),
        },
        Command::Contact { command } => match command {
            ContactCommand::Set {
                identity,
                nickname,
                note,
            } => contact::set(ctx, identity, nickname.as_deref(), note.as_deref()),
            ContactCommand::Show { identity } => contact::show(ctx, identity),
        },
        Command::Graph { command } => match command {
            GraphCommand::Sync { depth, peer } => graph::sync(ctx, *depth, peer),
            GraphCommand::Status => graph::status(ctx),
        },
        Command::Lookup { identity, from } => graph::lookup(ctx, identity, from.as_deref()),
        Command::Trust { command } => match command {
            TrustCommand::Add {
                issuer,
                subject,
                append,
            } => trust::add(ctx, issuer, subject, append),
            TrustCommand::Revoke {
                issuer,
                attestation,
                append,
            } => trust::revoke(ctx, issuer, attestation, append),
            TrustCommand::List { issuer } => trust::list(ctx, issuer),
        },
        Command::Witness { command } => match command {
            WitnessCommand::Add {
                identity,
                endpoint,
                append,
            } => witness::add(ctx, identity, endpoint, append),
            WitnessCommand::SetDefault { endpoints } => witness::set_default(ctx, endpoints),
            WitnessCommand::Run {
                http,
                iroh_port,
                peer,
                ui_dir,
            } => witness_run::run(ctx, *http, *iroh_port, peer, ui_dir.clone()),
        },
        Command::Sync { command } => match command {
            SyncCommand::Push { identity, to, peer } => {
                sync::push(ctx, identity, to.as_deref(), peer)
            }
            SyncCommand::Fetch {
                ledger_id,
                from,
                peer,
            } => sync::fetch(ctx, ledger_id, from, peer),
        },
        Command::Verify { command } => match command {
            VerifyCommand::Ledger {
                ledger_id,
                from,
                peer,
            } => verify::ledger(ctx, ledger_id, from.as_deref(), peer),
            VerifyCommand::Trust {
                issuer,
                subject,
                from,
                peer,
            } => verify::trust(ctx, issuer, subject, from.as_deref(), peer),
        },
        Command::Wallet { command } => match command {
            WalletCommand::Serve {
                http,
                iroh_port,
                peer,
                ui_dir,
            } => wallet_serve::serve(ctx, *http, *iroh_port, peer, ui_dir.clone()),
        },
        Command::Node { command } => match command {
            NodeCommand::Id => node::id(ctx),
            NodeCommand::Ticket { addr, port } => node::ticket(ctx, addr, *port),
        },
    }
}
