//! The command surface of proposal 001 section 9, as proposal 002 section 6
//! renames it.
//!
//! The global flags are `global = true`, so `mabel identity create --alias
//! alice --json` and `mabel --json identity create --alias alice` are the same
//! command line; the fixtures in `contracts/cli/` write `--json` last.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use mabel_node::DeclaredKind as StoredKind;
use mabel_proto::v0::DeclaredKind as ProtoKind;
use mabel_proto::v0::Role as ProtoRole;

/// mabel: peer-to-peer identity ledgers over Iroh.
#[derive(Debug, Parser)]
#[command(name = "mabel", version, about, long_about = None)]
pub struct Cli {
    /// Node home, overriding `$MABEL_HOME` and `~/.mabel`.
    #[arg(long, global = true, value_name = "PATH")]
    pub home: Option<PathBuf>,

    /// Print the JSON document instead of text.
    #[arg(long, global = true)]
    pub json: bool,

    /// Log what the command does to stderr.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Read a group- or world-accessible key file instead of exiting 60.
    #[arg(long, global = true)]
    pub allow_insecure_permissions: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// The command groups.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create, list and show the identities this home holds.
    Identity {
        #[command(subcommand)]
        command: IdentityCommand,
    },
    /// Invite, admit, remove and list the principals of a ledger.
    ///
    /// `org` and `member` are accepted for the same command and stay out of
    /// `--help`, so a reader of the help text meets one spelling (proposal 002
    /// section 6).
    #[command(alias = "org", alias = "member")]
    Membership {
        #[command(subcommand)]
        command: MembershipCommand,
    },
    /// Attest to an identity, revoke an attestation, list what one issued.
    Trust {
        #[command(subcommand)]
        command: TrustCommand,
    },
    /// Configure the witnesses an identity's ledger is pushed to.
    Witness {
        #[command(subcommand)]
        command: WitnessCommand,
    },
    /// Push a ledger to its witnesses, or fetch one from a peer.
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },
    /// Verify a ledger, or trust from an issuer to a subject.
    Verify {
        #[command(subcommand)]
        command: VerifyCommand,
    },
    /// Serve this home as a wallet.
    Wallet {
        #[command(subcommand)]
        command: WalletCommand,
    },
    /// Report on this node.
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
}

/// The append discipline of proposal 001 section 5, on every command that
/// appends.
///
/// Before appending to a ledger this wallet does not solely control, the
/// witnesses the ledger names are asked where it ends. Reaching them needs the
/// same `--peer` address hints a `sync` command takes, and an offline caller
/// needs a way out, which is `--no-sync`.
#[derive(Debug, Args)]
pub struct AppendOptions {
    /// Append without asking the ledger's witnesses where it ends.
    #[arg(long)]
    pub no_sync: bool,
    /// Endpoint ticket to seed into address lookup. Repeatable.
    #[arg(long, value_name = "TICKET")]
    pub peer: Vec<String>,
}

/// `mabel identity ...`.
#[derive(Debug, Subcommand)]
pub enum IdentityCommand {
    /// Create an identity: a raw root, or an identity root under --founder.
    Create {
        /// Local label for the new identity. Never signed.
        #[arg(long)]
        alias: String,
        /// What the identity declares itself to be. Gates nothing.
        #[arg(long, value_enum, default_value_t = Kind::Person)]
        kind: Kind,
        /// Found the identity under this identity's key instead of its own.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        founder: Option<String>,
    },
    /// List every identity in this home.
    List,
    /// Show one identity.
    Show {
        /// The identity, by alias or id.
        identity: String,
    },
    /// Write an identity's `IdentityDescriptor` file, the artifact an invitation
    /// embeds.
    Export {
        /// The identity, by alias or id.
        identity: String,
        /// Where to write the descriptor.
        #[arg(long, value_name = "PATH")]
        out: PathBuf,
    },
    /// Not part of this POC; exits 70.
    Rotate {
        /// The identity, by alias or id.
        identity: Option<String>,
    },
}

/// `mabel membership ...`, the three-step flow of proposal 002 section 6.
///
/// Two parties sign: a controller invites, the invitee accepts, a controller
/// admits. Each step hands the next one a file (proposal 001 section 3.8).
#[derive(Debug, Subcommand)]
pub enum MembershipCommand {
    /// Append an invitation naming the identity a descriptor file describes.
    Invite {
        /// The ledger the invitation is appended to, by alias or id.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        ledger: String,
        /// The controller identity that signs it, by alias or id.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        by: String,
        /// The invitee's `IdentityDescriptor` file, from `identity export`.
        #[arg(long, value_name = "PATH")]
        invitee: PathBuf,
        /// The role offered.
        #[arg(long, value_enum)]
        role: RoleArg,
        /// Where to write the `InvitationBundle` for the invitee.
        #[arg(long, value_name = "PATH")]
        out: PathBuf,
        #[command(flatten)]
        append: AppendOptions,
    },
    /// Sign an acceptance of an invitation, after showing what it admits to.
    Accept {
        /// The `InvitationBundle` file the inviter sent.
        #[arg(value_name = "INVITATION_BUNDLE")]
        bundle: PathBuf,
        /// The invited identity, by alias or id.
        #[arg(long = "as", value_name = "ALIAS_OR_ID")]
        identity: String,
        /// Where to write the `AcceptanceFile` for a controller to admit.
        #[arg(long, value_name = "PATH")]
        out: PathBuf,
        /// Accept without the interactive confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Append the acceptance an invitee signed, admitting them.
    Admit {
        /// The ledger the acceptance is appended to, by alias or id.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        ledger: String,
        /// The controller identity that signs it, by alias or id.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        by: String,
        /// The `AcceptanceFile` the invitee sent.
        #[arg(value_name = "ACCEPTANCE_FILE")]
        acceptance: PathBuf,
        #[command(flatten)]
        append: AppendOptions,
    },
    /// Remove a principal and cancel its open invitation, whichever exist.
    Remove {
        /// The ledger the removal is appended to, by alias or id.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        ledger: String,
        /// The controller identity that signs it, by alias or id.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        by: String,
        /// The identity to remove, by alias or id. `--target` is accepted for
        /// the same flag, since the event field is `target`.
        #[arg(long, alias = "target", value_name = "ALIAS_OR_ID")]
        member: String,
        #[command(flatten)]
        append: AppendOptions,
    },
    /// Show a ledger's principals and its invitations.
    List {
        /// The ledger, by alias or id.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        ledger: String,
    },
}

/// The role an invitation offers (proposal 002 section 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum RoleArg {
    /// Recorded data with no signing authority.
    Member,
    /// May append to the ledger.
    Controller,
}

impl RoleArg {
    /// The enum value the invitation carries.
    #[must_use]
    pub const fn proto(self) -> ProtoRole {
        match self {
            Self::Member => ProtoRole::Member,
            Self::Controller => ProtoRole::Controller,
        }
    }
}

/// `mabel trust ...`.
#[derive(Debug, Subcommand)]
pub enum TrustCommand {
    /// Append an attestation naming --subject to --issuer's ledger.
    Add {
        /// The identity whose ledger signs, by alias or id.
        #[arg(long)]
        issuer: String,
        /// The identity the attestation names, by alias or id.
        #[arg(long)]
        subject: String,
        #[command(flatten)]
        append: AppendOptions,
    },
    /// Revoke an attestation earlier in --issuer's ledger.
    Revoke {
        /// The identity whose ledger signs, by alias or id.
        #[arg(long)]
        issuer: String,
        /// The attestation's event id.
        #[arg(long)]
        attestation: String,
        #[command(flatten)]
        append: AppendOptions,
    },
    /// List the attestations one ledger has issued.
    List {
        /// The identity whose ledger is read, by alias or id.
        #[arg(long)]
        issuer: String,
    },
}

/// `mabel witness ...`.
#[derive(Debug, Subcommand)]
pub enum WitnessCommand {
    /// Add an endpoint to an identity's witness set.
    Add {
        /// The identity, by alias or id.
        #[arg(long)]
        identity: String,
        /// The witness endpoint id, base32 or hex.
        #[arg(long)]
        endpoint: String,
        #[command(flatten)]
        append: AppendOptions,
    },
    /// Serve this home as a witness until ctrl-c.
    Run {
        /// Address the HTTP API binds, overriding node.json's http_bind.
        #[arg(long, value_name = "ADDR")]
        http: Option<SocketAddr>,
        /// UDP port the Iroh endpoint binds, instead of an ephemeral one.
        #[arg(long, value_name = "PORT")]
        iroh_port: Option<u16>,
        /// Endpoint ticket to seed into address lookup. Repeatable.
        #[arg(long, value_name = "TICKET")]
        peer: Vec<String>,
        /// Serve the UI from this directory instead of the embedded bundle.
        #[arg(long, value_name = "DIR")]
        ui_dir: Option<PathBuf>,
    },
}

/// `mabel sync ...`, the two network commands of proposal 001 section 9.
#[derive(Debug, Subcommand)]
pub enum SyncCommand {
    /// Push an identity's ledger to the witnesses it names.
    Push {
        /// The identity whose ledger is pushed, by alias or id.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        identity: String,
        /// Push to this endpoint alone instead of the configured witnesses.
        #[arg(long, value_name = "ENDPOINT_ID")]
        to: Option<String>,
        /// Endpoint ticket to seed into address lookup. Repeatable.
        #[arg(long, value_name = "TICKET")]
        peer: Vec<String>,
    },
    /// Fetch a ledger from a peer, verify it from nothing and store it.
    Fetch {
        /// The ledger to fetch.
        ledger_id: String,
        /// The endpoint to fetch from.
        #[arg(long, value_name = "ENDPOINT_ID")]
        from: String,
        /// Endpoint ticket to seed into address lookup. Repeatable.
        #[arg(long, value_name = "TICKET")]
        peer: Vec<String>,
    },
}

/// `mabel verify ...`.
///
/// With no `--from` a ledger that names witnesses is verified against every
/// one of them in parallel; a ledger that names none is read from this home
/// (proposal 001 section 3.7).
#[derive(Debug, Subcommand)]
pub enum VerifyCommand {
    /// Verify one ledger.
    Ledger {
        /// The ledger, by alias or id.
        ledger_id: String,
        /// Read from this endpoint alone.
        #[arg(long, value_name = "ENDPOINT_ID")]
        from: Option<String>,
        /// Endpoint ticket to seed into address lookup. Repeatable.
        #[arg(long, value_name = "TICKET")]
        peer: Vec<String>,
    },
    /// Verify whether an issuer trusts a subject.
    Trust {
        /// The issuing identity, by alias or id.
        #[arg(long)]
        issuer: String,
        /// The subject, by alias or id.
        #[arg(long)]
        subject: String,
        /// Read from this endpoint alone.
        #[arg(long, value_name = "ENDPOINT_ID")]
        from: Option<String>,
        /// Endpoint ticket to seed into address lookup. Repeatable.
        #[arg(long, value_name = "TICKET")]
        peer: Vec<String>,
    },
}

/// `mabel wallet ...`.
#[derive(Debug, Subcommand)]
pub enum WalletCommand {
    /// Serve this home as a wallet until ctrl-c.
    Serve {
        /// Address the HTTP API binds, overriding node.json's http_bind.
        #[arg(long, value_name = "ADDR")]
        http: Option<SocketAddr>,
        /// UDP port the Iroh endpoint binds, instead of an ephemeral one.
        #[arg(long, value_name = "PORT")]
        iroh_port: Option<u16>,
        /// Endpoint ticket to seed into address lookup. Repeatable.
        #[arg(long, value_name = "TICKET")]
        peer: Vec<String>,
        /// Serve the UI from this directory instead of the embedded bundle.
        #[arg(long, value_name = "DIR")]
        ui_dir: Option<PathBuf>,
    },
}

/// `mabel node ...`.
#[derive(Debug, Subcommand)]
pub enum NodeCommand {
    /// Print this node's Iroh endpoint id.
    Id,
}

/// The declared kind an identity is created with (proposal 002 section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum Kind {
    /// A person.
    Person,
    /// An organization.
    Organization,
    /// An agent acting for someone.
    Agent,
    /// A service.
    Service,
}

impl Kind {
    /// The enum value the inception carries.
    #[must_use]
    pub const fn proto(self) -> ProtoKind {
        match self {
            Self::Person => ProtoKind::Person,
            Self::Organization => ProtoKind::Organization,
            Self::Agent => ProtoKind::Agent,
            Self::Service => ProtoKind::Service,
        }
    }

    /// The value `identities/<id>/meta.json` records.
    #[must_use]
    pub const fn stored(self) -> StoredKind {
        match self {
            Self::Person => StoredKind::Person,
            Self::Organization => StoredKind::Organization,
            Self::Agent => StoredKind::Agent,
            Self::Service => StoredKind::Service,
        }
    }
}
