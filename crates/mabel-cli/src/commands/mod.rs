//! One module per command group, and the dispatch that reaches them.

pub mod identity;
pub mod node;
pub mod trust;
pub mod verify;
pub mod witness;

use crate::cli::{
    Cli, Command, IdentityCommand, NodeCommand, TrustCommand, VerifyCommand, WitnessCommand,
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
            IdentityCommand::Rotate { .. } => identity::rotate(),
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
