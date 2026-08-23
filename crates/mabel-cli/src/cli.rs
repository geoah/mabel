//! The command surface of proposal 001 section 9, as proposal 002 section 6
//! renames it.
//!
//! The global flags are `global = true`, so `mabel identity create --alias
//! alice --json` and `mabel --json identity create --alias alice` are the same
//! command line; the fixtures in `contracts/cli/` write `--json` last.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use mabel_node::DeclaredKind as StoredKind;
use mabel_proto::v0::DeclaredKind as ProtoKind;

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
    /// Verify a ledger, or trust from an issuer to a subject.
    Verify {
        #[command(subcommand)]
        command: VerifyCommand,
    },
    /// Report on this node.
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
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
    /// Not part of this POC; exits 70.
    Rotate {
        /// The identity, by alias or id.
        identity: Option<String>,
    },
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
    },
    /// Revoke an attestation earlier in --issuer's ledger.
    Revoke {
        /// The identity whose ledger signs, by alias or id.
        #[arg(long)]
        issuer: String,
        /// The attestation's event id.
        #[arg(long)]
        attestation: String,
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
    },
}

/// `mabel verify ...`.
#[derive(Debug, Subcommand)]
pub enum VerifyCommand {
    /// Verify one ledger held by this home.
    Ledger {
        /// The ledger, by alias or id.
        ledger_id: String,
    },
    /// Verify whether an issuer trusts a subject.
    Trust {
        /// The issuing identity, by alias or id.
        #[arg(long)]
        issuer: String,
        /// The subject, by alias or id.
        #[arg(long)]
        subject: String,
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
