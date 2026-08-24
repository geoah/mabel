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
    /// Replace the profile a ledger publishes: display name, hostname and email.
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Read and write the private note this node keeps on an identity.
    Contact {
        #[command(subcommand)]
        command: ContactCommand,
    },
    /// Crawl outward from this node's identities, and report the last crawl.
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
    /// Answer "how do I know this identity" from the last crawl.
    Lookup {
        /// The identity to look up, by id or by a local alias.
        identity: String,
        /// The local identity the answer is relative to, by alias or id.
        /// Defaults to the lowest identity id in this home.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        from: Option<String>,
    },
    /// Attest to an identity, revoke an attestation, list what one issued.
    Trust {
        #[command(subcommand)]
        command: TrustCommand,
    },
    /// Configure the witnesses a ledger is pushed to, or this node's own set.
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
    /// Serve this home until ctrl-c: the HTTP API, the UI and the sync server.
    Serve {
        #[command(flatten)]
        options: ServeOptions,
    },
    /// The old name of `serve`, kept so an existing command line still runs
    /// (proposal 006 section 8).
    #[command(hide = true)]
    Wallet {
        #[command(subcommand)]
        command: WalletCommand,
    },
    /// Report on this node.
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
    /// Fill a home with data to develop and test against.
    Dev {
        #[command(subcommand)]
        command: DevCommand,
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
        /// The display name the new identity publishes. With --email, it lands
        /// as one ProfileUpdate at seq 1.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
        /// The public email the new identity publishes, which becomes public.
        #[arg(long, value_name = "EMAIL")]
        email: Option<String>,
    },
    /// List every identity in this home.
    List,
    /// Show one identity.
    Show {
        /// The identity, by alias or id.
        identity: String,
    },
    /// Publish the machines that answer for an identity.
    Endpoints {
        #[command(subcommand)]
        command: EndpointsCommand,
    },
    /// Print an identity as one `mabel://` link, and optionally a QR square
    /// and a file.
    Share {
        /// The identity, by alias, id or link.
        identity: String,
        /// `auto` for the machines the identity advertises, `none` for a link
        /// with no hint, or a comma-separated list of up to four endpoint ids.
        #[arg(long, value_name = "AUTO_OR_ENDPOINTS", default_value = "auto")]
        endpoints: String,
        /// Also write the link to this file, one line, UTF-8.
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
        /// Also print the link as a QR square.
        #[arg(long)]
        qr: bool,
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

/// `mabel identity endpoints ...`.
#[derive(Debug, Subcommand)]
pub enum EndpointsCommand {
    /// Replace the whole list of machines that answer for an identity.
    ///
    /// One event says "these and only these", so a rotation names the machine
    /// it keeps as well as the new one.
    Replace {
        /// The identity, by alias or id.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        identity: String,
        /// `auto` for this node's own endpoint id, `none` to advertise nothing,
        /// or a comma-separated list of endpoint ids, base32 or hex.
        #[arg(long, value_name = "AUTO_OR_ENDPOINTS")]
        endpoints: String,
        #[command(flatten)]
        append: AppendOptions,
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

/// `mabel profile ...` (proposal 003 section 1).
///
/// The operation is replacement, not patch: an omitted flag **clears** that
/// field. A patch verb is not offered, because a partial update over a
/// whole-document payload is the shape that silently drops a hostname.
#[derive(Debug, Subcommand)]
pub enum ProfileCommand {
    /// Replace the whole profile. An omitted flag clears that field.
    Replace {
        /// The ledger whose profile is replaced, by alias or id.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        identity: String,
        /// The name to publish. Omitted clears it.
        #[arg(long, value_name = "NAME")]
        display_name: Option<String>,
        /// The hostname to claim, which becomes public. Omitted clears it.
        #[arg(long, value_name = "HOSTNAME")]
        hostname: Option<String>,
        /// The email to publish, which becomes public. Omitted clears it.
        #[arg(long, value_name = "EMAIL")]
        email: Option<String>,
        /// Replace without the interactive confirmation.
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        append: AppendOptions,
    },
}

/// `mabel contact ...` (proposal 003 section 1).
///
/// The note lives in `contacts/<identity_id>.json`, is never signed and never
/// leaves this node. It is valid for a foreign identity too.
#[derive(Debug, Subcommand)]
pub enum ContactCommand {
    /// Replace the private note on an identity. An omitted flag clears that
    /// field, and clearing both removes the file.
    Set {
        /// The identity the note is about, by id or by a local alias.
        identity: String,
        /// A private name, at most 64 bytes.
        #[arg(long, value_name = "NICKNAME")]
        nickname: Option<String>,
        /// A private note, at most 512 bytes.
        #[arg(long, value_name = "NOTE")]
        note: Option<String>,
    },
    /// Show the private note on an identity.
    Show {
        /// The identity, by id or by a local alias.
        identity: String,
    },
}

/// `mabel graph ...` (proposal 003 section 3).
///
/// Synchronizing is manual: nothing here runs on a timer, and a sync tells
/// each contacted witness which identities this wallet cares about.
#[derive(Debug, Subcommand)]
pub enum GraphCommand {
    /// Crawl outward from every identity in this home and store the result.
    Sync {
        /// Levels to walk, held inside 1 through 4. Defaults to 2.
        #[arg(long, value_name = "DEPTH")]
        depth: Option<u32>,
        /// Endpoint ticket to seed into address lookup. Repeatable.
        #[arg(long, value_name = "TICKET")]
        peer: Vec<String>,
    },
    /// Report the last crawl: counts, caps hit and how old it is.
    Status,
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
    /// Add a witness identity to an identity's witness set.
    ///
    /// A witness is named by its Mabel id, not by the endpoint it answers at,
    /// so replacing the machine behind a witness leaves this event standing
    /// (proposal 006 section 1).
    Add {
        /// The identity whose witness set is replaced, by alias or id.
        #[arg(long)]
        identity: String,
        /// The witness identity, by alias or id.
        #[arg(long)]
        witness: String,
        #[command(flatten)]
        append: AppendOptions,
    },
    /// Replace the node-wide witness set in `node.json`.
    ///
    /// These are the witnesses this node queries for any ledger, which is a
    /// separate thing from the witnesses a ledger names in its own chain:
    /// `witness add` signs a ledger event, `witness set-default` edits this
    /// node's configuration and signs nothing.
    SetDefault {
        /// The witness identity, by alias, id or link. The set is replaced,
        /// not added to.
        #[arg(long, value_name = "MABEL_ID", required = true)]
        witness: String,
        /// Endpoint ids to record beside the identity, comma-separated or
        /// repeated. These are the bootstrap addresses of proposal 006
        /// section 5.4.
        #[arg(long, value_name = "ENDPOINT", value_delimiter = ',')]
        endpoints: Vec<String>,
    },
    /// The old name of `serve`.
    #[command(hide = true)]
    Run {
        #[command(flatten)]
        options: ServeOptions,
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
        /// The ledger to fetch, by alias, id or link.
        ledger_id: String,
        /// The endpoint to fetch from. Required unless a link, a witness or a
        /// configured default names one.
        #[arg(long, value_name = "ENDPOINT_ID")]
        from: Option<String>,
        /// A witness identity to fetch from, resolved to endpoints through
        /// proposal 006 section 5.1. Not with --from.
        #[arg(long, value_name = "MABEL_ID")]
        from_witness: Option<String>,
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

/// What `mabel serve` takes, shared with the two hidden aliases.
#[derive(Debug, Args)]
pub struct ServeOptions {
    /// Address the HTTP API binds, overriding node.json's http_bind.
    #[arg(long, value_name = "ADDR")]
    pub http: Option<SocketAddr>,
    /// UDP port the Iroh endpoint binds, instead of an ephemeral one.
    #[arg(long, value_name = "PORT")]
    pub iroh_port: Option<u16>,
    /// Endpoint ticket to seed into address lookup. Repeatable.
    #[arg(long, value_name = "TICKET")]
    pub peer: Vec<String>,
    /// Serve the UI from this directory instead of the embedded bundle.
    #[arg(long, value_name = "DIR")]
    pub ui_dir: Option<PathBuf>,
    /// Also accept requests whose Host is this value, added to node.json's
    /// allowed_hosts. Repeatable.
    ///
    /// The HTTP API has no authentication, so allowing a host beyond loopback
    /// hands whatever keys this home holds to whoever can reach that name: the
    /// operator owns the network boundary (decision 018).
    #[arg(long, value_name = "HOST")]
    pub allow_host: Vec<String>,
}

/// `mabel wallet ...`, the hidden alias of `serve`.
#[derive(Debug, Subcommand)]
pub enum WalletCommand {
    /// The old name of `serve`.
    #[command(hide = true)]
    Serve {
        #[command(flatten)]
        options: ServeOptions,
    },
}

/// `mabel node ...`.
#[derive(Debug, Subcommand)]
pub enum NodeCommand {
    /// Print this node's Iroh endpoint id.
    Id,
    /// Print this node's `EndpointTicket`, the string `--peer` takes.
    ///
    /// The ticket names this node's endpoint id and the addresses it is
    /// reachable at. With no `--addr` and no `--port` it carries no address,
    /// which is enough for a node whose `node.json` sets `relay: "n0"`.
    Ticket {
        /// An address this node is reachable at, `IP:PORT`. Repeatable.
        #[arg(long, value_name = "ADDR")]
        addr: Vec<SocketAddr>,
        /// Pair this node's detected IPv4 address with this UDP port, which is
        /// the port the Iroh endpoint binds.
        #[arg(long, value_name = "PORT")]
        port: Option<u16>,
    },
}

/// `mabel dev ...`.
///
/// One subcommand, and everything it writes it writes through the commands
/// above: there is no second code path that mints a ledger, so a seeded home
/// holds the same bytes a person typing the commands would have produced.
#[derive(Debug, Subcommand)]
pub enum DevCommand {
    /// Fill an empty home with five identities, one organization, four
    /// attestations and one private note.
    ///
    /// Refuses a home that already holds an identity: this writes real signed
    /// ledgers, and there is no way to take an event back.
    Seed {
        /// Endpoint ticket of a machine to push every seeded ledger to.
        /// Repeatable.
        ///
        /// Given none, the seed stays local: the ledgers name no witness,
        /// nothing is pushed and no crawl runs.
        #[arg(long, value_name = "TICKET")]
        peer: Vec<String>,
        /// A witness identity to name in every seeded witness set, beside the
        /// one the seed creates. Repeatable.
        ///
        /// A ticket names a machine, and a machine only takes a push for a
        /// ledger whose witness set names an identity it witnesses for, so a
        /// push to somebody else's witness needs that witness's Mabel id.
        #[arg(long, value_name = "ALIAS_OR_ID")]
        witness: Vec<String>,
    },
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
