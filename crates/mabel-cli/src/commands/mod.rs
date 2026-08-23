//! One module per command group, and the dispatch that reaches them.

pub mod identity;
pub mod membership;
pub mod node;
pub mod trust;
pub mod verify;
pub mod witness;
pub mod witness_run;

use crate::cli::{
    Cli, Command, IdentityCommand, MembershipCommand, NodeCommand, TrustCommand, VerifyCommand,
    WitnessCommand,
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
            } => identity::create(ctx, alias, *kind, founder.as_deref()),
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
            } => membership::invite(ctx, ledger, by, invitee, *role, out),
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
            } => membership::admit(ctx, ledger, by, acceptance),
            MembershipCommand::Remove { ledger, by, member } => {
                membership::remove(ctx, ledger, by, member)
            }
            MembershipCommand::List { ledger } => membership::list(ctx, ledger),
        },
        Command::Trust { command } => match command {
            TrustCommand::Add { issuer, subject } => trust::add(ctx, issuer, subject),
            TrustCommand::Revoke {
                issuer,
                attestation,
            } => trust::revoke(ctx, issuer, attestation),
            TrustCommand::List { issuer } => trust::list(ctx, issuer),
        },
        Command::Witness { command } => match command {
            WitnessCommand::Add { identity, endpoint } => witness::add(ctx, identity, endpoint),
            WitnessCommand::Run {
                http,
                iroh_port,
                peer,
            } => witness_run::run(ctx, *http, *iroh_port, peer),
        },
        Command::Verify { command } => match command {
            VerifyCommand::Ledger { ledger_id } => verify::ledger(ctx, ledger_id),
            VerifyCommand::Trust { issuer, subject } => verify::trust(ctx, issuer, subject),
        },
        Command::Node { command } => match command {
            NodeCommand::Id => node::id(ctx),
        },
    }
}
