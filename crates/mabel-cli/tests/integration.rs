//! The end-to-end suite: real `mabel` processes, real Iroh endpoints, two
//! witnesses and several wallet homes (ticket 016).
//!
//! Nothing here is a double. Each witness is a `mabel witness run` child, each
//! wallet is a temp home the binary is pointed at with `--home`, and the only
//! things that cross between homes are pushed events and the artifact files of
//! proposal 001 section 3.8. Every home sets `relay: "disabled"` in `node.json`
//! and every peer is dialled through a `--peer` ticket carrying its loopback
//! address, so no test touches DNS, a relay or the internet (section 11).
//!
//! The exit codes this ticket owns end to end are 0, 20 for equivocation across
//! two witnesses, 30 for a witness that cannot be reached, and 50 for an append
//! that lost a race. The component-level twins live in ticket 011.

#![cfg(unix)]

use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as Process, Stdio};
use std::time::{Duration, Instant};

use assert_cmd::Command;
use iroh_base::{EndpointAddr, EndpointId, TransportAddr};
use iroh_tickets::Ticket;
use iroh_tickets::endpoint::EndpointTicket;
use serde_json::Value;
use tempfile::TempDir;

/// Decision 013: a networked test never waits longer than this.
const TIMEOUT: Duration = Duration::from_secs(10);

/// A temp node home and the binary that runs against it.
struct Home {
    directory: TempDir,
}

impl Home {
    /// A home with `relay: "disabled"` and the given role.
    fn new(role: &str) -> Self {
        let home = Self {
            directory: TempDir::new().expect("a temp directory"),
        };
        // `node id` creates the home, node.json and node.key.
        home.json(&["node", "id"]);
        let path = home.path().join("node.json");
        let mut config: Value =
            serde_json::from_slice(&std::fs::read(&path).expect("node.json reads"))
                .expect("node.json is JSON");
        config["relay"] = Value::from("disabled");
        config["role"] = Value::from(role);
        std::fs::write(&path, serde_json::to_vec_pretty(&config).expect("json")).expect("written");
        home
    }

    /// A byte-for-byte copy of another home: the same identity keys, the same
    /// ledgers, a second machine holding the same wallet.
    ///
    /// This is how a test gets two homes that may both append to one shared
    /// ledger without either of them holding every controller key, which is
    /// what the append discipline of proposal 001 section 5 is about.
    fn fork(other: &Self) -> Self {
        let directory = TempDir::new().expect("a temp directory");
        copy_tree(other.path(), directory.path());
        Self { directory }
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::cargo_bin("mabel").expect("the mabel binary is built");
        command.arg("--home").arg(self.path()).args(arguments);
        command
    }

    /// Runs a command, returning its exit code, stdout and stderr.
    fn run(&self, arguments: &[&str]) -> (i32, String, String) {
        let output = self.command(arguments).output().expect("the binary runs");
        (
            output.status.code().expect("the process exited"),
            String::from_utf8(output.stdout).expect("stdout is utf-8"),
            String::from_utf8(output.stderr).expect("stderr is utf-8"),
        )
    }

    /// Runs a `--json` command that must succeed.
    fn json(&self, arguments: &[&str]) -> Value {
        let mut arguments = arguments.to_vec();
        arguments.push("--json");
        let (code, stdout, stderr) = self.run(&arguments);
        assert_eq!(code, 0, "{arguments:?} failed: {stdout}{stderr}");
        let document = parse(&stdout);
        assert_eq!(document["ok"], Value::Bool(true), "{document}");
        document
    }

    /// Runs a `--json` command that must fail, returning the code and the
    /// error envelope.
    fn failure(&self, arguments: &[&str]) -> (i32, Value) {
        let mut arguments = arguments.to_vec();
        arguments.push("--json");
        let (code, stdout, stderr) = self.run(&arguments);
        assert_ne!(code, 0, "{arguments:?} unexpectedly succeeded: {stdout}");
        let document = parse(&stdout);
        assert_eq!(document["ok"], Value::Bool(false), "{stderr}");
        assert_eq!(document["code"], Value::from(code), "{document}");
        (code, document)
    }

    /// Empties `node.json.witnesses`, so this home names no machine to ask for
    /// a ledger and reads its own copy instead.
    fn clear_default_witnesses(&self) {
        let path = self.path().join("node.json");
        let mut config: Value =
            serde_json::from_slice(&std::fs::read(&path).expect("node.json reads"))
                .expect("node.json is JSON");
        config["witnesses"] = Value::Array(Vec::new());
        std::fs::write(&path, serde_json::to_vec_pretty(&config).expect("json")).expect("written");
    }

    /// Sets `node.json.witness_for`, the witness identities this home takes
    /// pushes for (proposal 006 section 4). No command edits it yet, so a test
    /// writes the file the way an operator would.
    fn set_witness_for(&self, identities: &[&str]) {
        let path = self.path().join("node.json");
        let mut config: Value =
            serde_json::from_slice(&std::fs::read(&path).expect("node.json reads"))
                .expect("node.json is JSON");
        config["witness_for"] = Value::Array(
            identities
                .iter()
                .map(|identity| Value::from(*identity))
                .collect(),
        );
        std::fs::write(&path, serde_json::to_vec_pretty(&config).expect("json")).expect("written");
    }

    /// This node's Iroh endpoint id, as every document spells it.
    fn endpoint(&self) -> String {
        text(&self.json(&["node", "id"])["endpoint_id"])
    }

    /// Creates a raw-rooted identity and returns its id.
    fn create(&self, alias: &str) -> String {
        text(&self.json(&["identity", "create", "--alias", alias])["identity_id"])
    }

    /// Creates an identity-rooted ledger under `founder` and returns its id.
    fn found(&self, alias: &str, founder: &str) -> String {
        text(
            &self.json(&[
                "identity",
                "create",
                "--alias",
                alias,
                "--kind",
                "organization",
                "--founder",
                founder,
            ])["identity_id"],
        )
    }

    /// The identities this home has private keys for.
    fn key_files(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        walk(&self.path().join("identities"), &mut |path| {
            if path.extension().is_some_and(|extension| extension == "key") {
                found.push(path.to_path_buf());
            }
        });
        found
    }
}

/// The directory homes exchange artifact files through.
struct Exchange {
    directory: TempDir,
}

impl Exchange {
    fn new() -> Self {
        Self {
            directory: TempDir::new().expect("a temp directory"),
        }
    }

    fn file(&self, name: &str) -> String {
        self.directory.path().join(name).display().to_string()
    }
}

fn parse(stdout: &str) -> Value {
    serde_json::from_str(stdout).unwrap_or_else(|error| panic!("{error}: {stdout}"))
}

fn text(value: &Value) -> String {
    value.as_str().expect("a string").to_owned()
}

/// Copies a whole home, keeping the 0600 mode of every key file.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("the target directory is created");
    for entry in std::fs::read_dir(from).expect("the source directory reads") {
        let entry = entry.expect("a directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("a file copies");
        }
    }
}

/// Calls `visit` for every file under `root`, if `root` exists.
fn walk(root: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, visit);
        } else {
            visit(&path);
        }
    }
}

/// One `document` from a `contracts/cli/` fixture, by case name.
fn fixture(file: &str, case: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/cli")
        .join(format!("{file}.json"));
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let fixture: Value = serde_json::from_slice(&bytes).expect("the fixture is JSON");
    let cases = fixture["cases"].as_array().expect("cases is an array");
    cases
        .iter()
        .find(|entry| entry["case"] == *case)
        .unwrap_or_else(|| panic!("{file}.json has no case {case}"))["document"]
        .clone()
}

/// Asserts that `actual` has the keys of `expected`, and the same kind of
/// value under each.
fn assert_shape(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Null, _) | (_, Value::Null) => {}
        (Value::Object(actual_fields), Value::Object(expected_fields)) => {
            let mut actual_keys: Vec<&String> = actual_fields.keys().collect();
            let mut expected_keys: Vec<&String> = expected_fields.keys().collect();
            actual_keys.sort();
            expected_keys.sort();
            assert_eq!(actual_keys, expected_keys, "keys differ at {path}");
            for (key, expected) in expected_fields {
                assert_shape(&actual_fields[key], expected, &format!("{path}.{key}"));
            }
        }
        (Value::Array(actual_items), Value::Array(expected_items)) => {
            if let (Some(actual), Some(expected)) = (actual_items.first(), expected_items.first()) {
                assert_shape(actual, expected, &format!("{path}[0]"));
            }
        }
        (Value::Bool(_), Value::Bool(_))
        | (Value::Number(_), Value::Number(_))
        | (Value::String(_), Value::String(_)) => {}
        _ => panic!("{path}: {actual} does not match the shape of {expected}"),
    }
}

/// A port nothing is listening on, taken by binding and releasing it.
fn free_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0")
        .expect("a udp socket binds")
        .local_addr()
        .expect("the socket has an address")
        .port()
}

/// The `--peer` ticket for an endpoint reachable at `127.0.0.1:port`.
fn ticket(endpoint: &str, port: u16) -> String {
    let bytes: [u8; 32] = data_encoding::BASE32_NOPAD
        .decode(endpoint.to_ascii_uppercase().as_bytes())
        .expect("a rendered endpoint id decodes")
        .try_into()
        .expect("32 bytes");
    let id = EndpointId::from_bytes(&bytes).expect("a public key");
    EndpointTicket::new(EndpointAddr {
        id,
        addrs: [TransportAddr::Ip(SocketAddr::from(([127, 0, 0, 1], port)))]
            .into_iter()
            .collect(),
    })
    .encode_string()
}

/// A `mabel` daemon running in the background, with its stderr on disk.
struct Daemon {
    child: Child,
    log: PathBuf,
}

impl Daemon {
    /// Starts `mabel --home <home> <arguments>` with stderr redirected.
    fn start(home: &Home, name: &str, arguments: &[&str]) -> Self {
        let log = home.path().join(format!("{name}.log"));
        let file = std::fs::File::create(&log).expect("the log file is created");
        let binary = assert_cmd::cargo::cargo_bin("mabel");
        let child = Process::new(binary)
            .arg("--home")
            .arg(home.path())
            .args(arguments)
            .stdout(Stdio::null())
            .stderr(Stdio::from(file))
            .spawn()
            .expect("the daemon starts");
        Self { child, log }
    }

    /// Waits until the log holds a line starting with `prefix`.
    fn wait_for(&self, prefix: &str) {
        let deadline = Instant::now() + TIMEOUT;
        while Instant::now() < deadline {
            let log = std::fs::read_to_string(&self.log).unwrap_or_default();
            if log.lines().any(|line| line.starts_with(prefix)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!(
            "no {prefix:?} line within {}s: {}",
            TIMEOUT.as_secs(),
            std::fs::read_to_string(&self.log).unwrap_or_default()
        );
    }

    /// Sends SIGINT and waits for the process to stop, returning its code.
    fn interrupt(mut self) -> i32 {
        let pid = self.child.id().to_string();
        Process::new("kill")
            .args(["-INT", &pid])
            .status()
            .expect("kill runs");
        let deadline = Instant::now() + TIMEOUT;
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().expect("the child is waitable") {
                return status.code().unwrap_or(-1);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        panic!("the daemon ignored SIGINT");
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A running witness: its home, its endpoint id and the ticket that reaches it.
struct Witness {
    _home: Home,
    identity: String,
    endpoint: String,
    ticket: String,
    daemon: Daemon,
}

impl Witness {
    fn start() -> Self {
        let home = Home::new("witness");
        let endpoint = home.endpoint();
        // A witness is an identity (proposal 006 section 1): this home mints
        // one, advertises the machine that answers for it, and names it in
        // `node.json.witness_for`, which is what makes it take a push.
        let identity = home.create("keeper");
        home.json(&[
            "identity",
            "endpoints",
            "replace",
            "--identity",
            "keeper",
            "--endpoints",
            "auto",
        ]);
        home.set_witness_for(&[&identity]);
        let port = free_port();
        let daemon = Daemon::start(
            &home,
            "witness",
            &[
                "witness",
                "run",
                "--http",
                "127.0.0.1:0",
                "--iroh-port",
                &port.to_string(),
            ],
        );
        daemon.wait_for("witness ");
        Self {
            ticket: ticket(&endpoint, port),
            identity,
            endpoint,
            _home: home,
            daemon,
        }
    }

    fn stop(self) {
        assert_eq!(self.daemon.interrupt(), 0, "the witness stops cleanly");
    }
}

/// Alice's ledger with Bob admitted as a controller from a second home, named
/// by `witness` and pushed to it.
///
/// Neither home holds every controller key afterwards, so every further append
/// to Alice's ledger runs the append discipline of proposal 001 section 5.
struct Shared {
    alice_home: Home,
    bob_home: Home,
    alice: String,
    bob: String,
}

impl Shared {
    fn build(witness: &Witness) -> Self {
        let alice_home = Home::new("wallet");
        let bob_home = Home::new("wallet");
        let exchange = Exchange::new();
        let alice = alice_home.create("alice");
        let bob = bob_home.create("bob");

        // Both ledgers name the witness identity, so both are admissible
        // pushes, and both homes record the machine that answers for it, which
        // is where a push dials.
        for (home, alias) in [(&alice_home, "alice"), (&bob_home, "bob")] {
            home.json(&["witness", "set-default", &witness.endpoint]);
            home.json(&[
                "witness",
                "add",
                "--identity",
                alias,
                "--witness",
                &witness.identity,
            ]);
        }
        bob_home.json(&[
            "sync",
            "push",
            "--identity",
            "bob",
            "--peer",
            &witness.ticket,
        ]);

        // Two parties sign one admission, and a file carries each step.
        let descriptor = exchange.file("bob.descriptor");
        bob_home.json(&["identity", "export", "bob", "--out", &descriptor]);
        let bundle = exchange.file("alice.invitation");
        alice_home.json(&[
            "membership",
            "invite",
            "--ledger",
            "alice",
            "--by",
            "alice",
            "--invitee",
            &descriptor,
            "--role",
            "controller",
            "--out",
            &bundle,
            "--peer",
            &witness.ticket,
        ]);
        let acceptance = exchange.file("bob.acceptance");
        bob_home.json(&[
            "membership",
            "accept",
            &bundle,
            "--as",
            "bob",
            "--out",
            &acceptance,
            "--yes",
        ]);
        alice_home.json(&[
            "membership",
            "admit",
            "--ledger",
            "alice",
            "--by",
            "alice",
            &acceptance,
            "--peer",
            &witness.ticket,
        ]);
        alice_home.json(&[
            "sync",
            "push",
            "--identity",
            "alice",
            "--peer",
            &witness.ticket,
        ]);

        Self {
            alice_home,
            bob_home,
            alice,
            bob,
        }
    }
}

/// The story of proposal 001 section 11, end to end, ending in the test the
/// section calls the best acceptance test: a home with no identities, no
/// ledgers and no keys, which learns the witness only from a `--peer` ticket
/// and verifies trust from bytes.
#[test]
fn the_whole_story_runs_and_a_fresh_home_verifies_it_from_the_witness_alone() {
    let witness = Witness::start();
    let shared = Shared::build(&witness);
    let Shared {
        alice_home,
        bob_home,
        alice,
        bob,
    } = &shared;

    // An attestation on the shared ledger: the discipline queries the witness
    // first, because Bob's key could have moved the head.
    let attested = alice_home.json(&[
        "trust",
        "add",
        "--issuer",
        "alice",
        "--subject",
        bob,
        "--peer",
        &witness.ticket,
    ]);
    let attestation = text(&attested["attestation_event"]);
    assert_eq!(attested["attestation_seq"], Value::from(4));
    alice_home.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);

    // And one from an identity-rooted ledger, which holds no key of its own:
    // Alice's key signs for it, and the report has to say so.
    //
    // This ledger names no witness and is verified from this home, because a
    // witness refuses its inception: `Push` wraps a `SignedEvent` two levels
    // down, and an identity root already reaches depth 7 of the 8 the wire
    // scanner allows, so `mabel-net` rejects it MALFORMED at seq 0. Filed
    // against `mabel-net`; the CLI is not what is wrong here.
    let team = alice_home.found("team", "alice");
    alice_home.json(&["trust", "add", "--issuer", "team", "--subject", bob]);
    let alice_key = text(&alice_home.json(&["identity", "show", "alice"])["active_key"]);

    // The fresh verifier: nothing in this home but a node key and a ticket.
    let fresh = Home::new("wallet");
    assert_eq!(
        fresh.json(&["identity", "list"])["identities"],
        Value::Array(Vec::new())
    );
    assert!(fresh.key_files().is_empty(), "the fresh home holds no keys");

    let report = fresh.json(&[
        "verify",
        "trust",
        "--issuer",
        alice,
        "--subject",
        bob,
        "--peer",
        &witness.ticket,
    ]);
    assert_shape(&report, &fixture("verify-trust", "trusted"), "verify-trust");
    assert_eq!(report["trusted"], Value::Bool(true));
    assert_eq!(report["issuer"], Value::from(alice.as_str()));
    assert_eq!(report["subject"], Value::from(bob.as_str()));
    assert_eq!(report["source"], Value::from(witness.endpoint.as_str()));
    assert_eq!(
        report["sources_queried"],
        Value::from(vec![witness.endpoint.as_str()])
    );
    assert_eq!(
        report["attestation_event"],
        Value::from(attestation.as_str())
    );
    assert_eq!(report["attestation_seq"], Value::from(4));
    assert_eq!(report["head_seq"], Value::from(4));
    // Proposal 002 section 5: the answer names the principal that signed.
    assert_eq!(
        report["signing_principal"],
        serde_json::json!({"identity": alice, "key": alice_key})
    );
    // The subject's own ledger was fetched from the witness too, not assumed.
    assert_eq!(report["subject_resolution"], Value::from("resolved"));
    assert_eq!(report["subject_note"], Value::Null);

    // Both ledgers verified from nothing, whole chain: the event count is the
    // whole chain, not a suffix spliced onto a witness's folded state.
    for (ledger, events) in [(alice, 5), (bob, 2)] {
        let verified = fresh.json(&["verify", "ledger", ledger, "--peer", &witness.ticket]);
        assert_eq!(verified["valid"], Value::Bool(true), "{verified}");
        assert_eq!(verified["ledger_id"], Value::from(ledger.as_str()));
        assert_eq!(verified["event_count"], Value::from(events), "{verified}");
        assert_eq!(verified["valid_to_seq"], Value::from(events - 1));
        assert_eq!(verified["source"], Value::from(witness.endpoint.as_str()));
        assert!(verified["fetched_at_ms"].as_u64().is_some_and(|ms| ms > 0));
    }

    // The identity-rooted ledger signs under its founder's key, and both the
    // document and the text a person reads say whose key it was. Nothing here
    // holds Bob's ledger, so the subject is reported unresolved rather than
    // assumed (proposal 001 section 3.7).
    // A home that names a default witness asks it for any ledger, and this one
    // holds no copy of `team`, so the local answer is read on a second machine
    // of the same operator that names no witness.
    let offline = Home::fork(alice_home);
    offline.clear_default_witnesses();
    let founded = offline.json(&["verify", "trust", "--issuer", &team, "--subject", bob]);
    assert_shape(
        &founded,
        &fixture("verify-trust", "unresolved-subject"),
        "verify-trust",
    );
    assert_eq!(founded["trusted"], Value::Bool(true));
    assert_eq!(founded["issuer"], Value::from(team.as_str()));
    assert_eq!(founded["subject_resolution"], Value::from("unresolved"));
    assert_eq!(
        founded["signing_principal"],
        serde_json::json!({"identity": alice, "key": alice_key})
    );
    let (code, stdout, stderr) =
        offline.run(&["verify", "trust", "--issuer", &team, "--subject", bob]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.starts_with("trusted: true"), "{stdout}");
    assert!(
        stdout.contains(&format!("signed by principal {alice} ({alice_key})")),
        "{stdout}"
    );

    // Verifying stored nothing: the home is as empty as it started.
    assert!(fresh.key_files().is_empty(), "verifying wrote no key");
    assert_eq!(
        fresh.json(&["identity", "list"])["identities"],
        Value::Array(Vec::new())
    );

    // Revoking flips the answer for the same pair, and says which attestation
    // was revoked at which sequence.
    alice_home.json(&[
        "trust",
        "revoke",
        "--issuer",
        "alice",
        "--attestation",
        &attestation,
        "--peer",
        &witness.ticket,
    ]);
    alice_home.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);
    let revoked = fresh.json(&[
        "verify",
        "trust",
        "--issuer",
        alice,
        "--subject",
        bob,
        "--peer",
        &witness.ticket,
    ]);
    assert_shape(
        &revoked,
        &fixture("verify-trust", "not-trusted-because-revoked"),
        "verify-trust",
    );
    assert_eq!(revoked["trusted"], Value::Bool(false));
    assert_eq!(revoked["revoked_count"], Value::from(1));
    assert_eq!(revoked["signing_principal"], Value::Null);
    assert_eq!(revoked["head_seq"], Value::from(5));
    let entry = &revoked["revoked_attestations"][0];
    assert_eq!(
        entry["attestation_event"],
        Value::from(attestation.as_str())
    );
    assert_eq!(entry["attestation_seq"], Value::from(4));
    assert_eq!(entry["revocation_seq"], Value::from(5));

    // Removing the controller takes Bob off the ledger, and the fresh home sees
    // that too, from the witness alone.
    let removed = alice_home.json(&[
        "membership",
        "remove",
        "--ledger",
        "alice",
        "--by",
        "alice",
        "--member",
        bob,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(removed["principal_removed"], Value::Bool(true));
    alice_home.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);
    let verified = fresh.json(&["verify", "ledger", alice, "--peer", &witness.ticket]);
    assert_eq!(verified["head_seq"], Value::from(6));
    assert_eq!(verified["event_count"], Value::from(7));

    // Bob's home never learned any of this: it holds its own ledger only.
    let held = bob_home.json(&["identity", "list"])["identities"]
        .as_array()
        .expect("an array")
        .len();
    assert_eq!(held, 1);

    witness.stop();
}

/// An append that lost the race exits 50, leaves no stale event behind, and the
/// same command run again lands on the head the witness served (proposal 001
/// section 5).
#[test]
fn an_append_that_lost_the_race_exits_50_and_the_retry_lands_on_the_new_head() {
    let witness = Witness::start();
    let shared = Shared::build(&witness);
    let alice_home = &shared.alice_home;
    let bob = shared.bob.as_str();
    let carol = shared.bob_home.create("carol");

    // The second machine holding Alice's key. Neither it nor the first holds
    // Bob's key, so both must ask the witness before they append.
    let second = Home::fork(alice_home);

    // This home appends offline, so its event at seq 4 is never pushed.
    let lost = alice_home.json(&[
        "trust",
        "add",
        "--issuer",
        "alice",
        "--subject",
        bob,
        "--no-sync",
    ]);
    let lost_event = text(&lost["attestation_event"]);
    assert_eq!(lost["attestation_seq"], Value::from(4));

    // The other machine appends a different event at seq 4 and pushes it, so
    // the witness's seq 4 is the one every other party sees.
    let won = second.json(&[
        "trust",
        "add",
        "--issuer",
        "alice",
        "--subject",
        &carol,
        "--peer",
        &witness.ticket,
    ]);
    let won_event = text(&won["attestation_event"]);
    assert_eq!(won["attestation_seq"], Value::from(4));
    assert_ne!(won_event, lost_event);
    second.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);

    // The first machine tries again to attest Bob and finds it lost the race.
    let (code, document) = alice_home.failure(&[
        "trust",
        "add",
        "--issuer",
        "alice",
        "--subject",
        bob,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(code, 50);
    assert!(
        text(&document["message"]).starts_with("State error: "),
        "{document}"
    );
    assert_eq!(document["details"]["reason"], Value::from("stale_head"));
    assert_eq!(
        document["details"]["ledger_id"],
        Value::from(shared.alice.as_str())
    );
    assert_eq!(document["details"]["local_head_seq"], Value::from(4));
    assert_eq!(document["details"]["observed_head_seq"], Value::from(4));
    assert_eq!(
        document["details"]["source"],
        Value::from(witness.endpoint.as_str())
    );

    // Text mode prints the same prefixed line on stderr, and the stale event is
    // gone from the home: the witness's chain replaced it.
    let listed = alice_home.json(&["trust", "list", "--issuer", "alice"]);
    assert_eq!(listed["head_seq"], Value::from(4));
    let issued = listed["trust"].as_array().expect("an array");
    assert_eq!(issued.len(), 1, "{listed}");
    assert_eq!(
        issued[0]["attestation_event"],
        Value::from(won_event.as_str())
    );
    assert_eq!(issued[0]["subject"], Value::from(carol.as_str()));

    // Losing a race is a retry: the same intent, re-signed on the new head.
    let retried = alice_home.json(&[
        "trust",
        "add",
        "--issuer",
        "alice",
        "--subject",
        bob,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(retried["attestation_seq"], Value::from(5));
    assert_eq!(retried["head_seq"], Value::from(5));
    assert_ne!(text(&retried["attestation_event"]), lost_event);
    assert_eq!(retried["subject"], Value::from(bob));

    // A witness with no address hint cannot be dialled, so the append refuses
    // rather than signing on a head it could not check.
    let dave = shared.bob_home.create("dave");
    let (code, document) =
        alice_home.failure(&["trust", "add", "--issuer", "alice", "--subject", &dave]);
    assert_eq!(code, 30);
    assert_eq!(
        document["details"]["reason"],
        Value::from("peer_unreachable")
    );
    assert!(
        text(&document["message"]).starts_with("Network error: no route to"),
        "{document}"
    );
    let (code, stdout, stderr) =
        alice_home.run(&["trust", "add", "--issuer", "alice", "--subject", &dave]);
    assert_eq!(code, 30, "{stdout}");
    assert!(stderr.starts_with("Network error: "), "{stderr}");

    // `--no-sync` is the way out for an offline append, and says nothing about
    // whether that event will survive a later race.
    let offline = alice_home.json(&[
        "trust",
        "add",
        "--issuer",
        "alice",
        "--subject",
        &dave,
        "--no-sync",
    ]);
    assert_eq!(offline["attestation_seq"], Value::from(6));

    // A fast-forward is silent and lands the next event on the witness's head:
    // the second machine, still at seq 4, catches up to seq 6 before it signs.
    alice_home.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);
    let erin = shared.bob_home.create("erin");
    let caught_up = second.json(&[
        "trust",
        "add",
        "--issuer",
        "alice",
        "--subject",
        &erin,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(caught_up["attestation_seq"], Value::from(7));

    witness.stop();
}

/// Two witnesses served divergent branches of one ledger, so a verifier that
/// asks both exits 20 and names both sources and both events (proposal 001
/// sections 3.7 and 11).
#[test]
fn two_witnesses_on_divergent_branches_exit_20_naming_both_sources() {
    let first = Witness::start();
    let second = Witness::start();

    let home = Home::new("wallet");
    let alice = home.create("alice");
    let bob = home.create("bob");
    let carol = home.create("carol");
    // Both machines keep Alice's chain for the same witness identity, and
    // `node.json` names both as where to dial.
    home.json(&["witness", "set-default", &first.endpoint, &second.endpoint]);
    for witness in [&first, &second] {
        home.json(&[
            "witness",
            "add",
            "--identity",
            "alice",
            "--witness",
            &witness.identity,
        ]);
    }
    home.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &first.ticket,
        "--peer",
        &second.ticket,
    ]);

    // Two machines hold Alice's key and both append at seq 3. Each valid event
    // is a real event; nothing here forges a signature.
    let fork = Home::fork(&home);
    let one = home.json(&[
        "trust",
        "add",
        "--issuer",
        "alice",
        "--subject",
        &bob,
        "--no-sync",
    ]);
    let other = fork.json(&[
        "trust",
        "add",
        "--issuer",
        "alice",
        "--subject",
        &carol,
        "--no-sync",
    ]);
    assert_eq!(one["attestation_seq"], Value::from(3));
    assert_eq!(other["attestation_seq"], Value::from(3));

    // One branch to each witness, which is what equivocation looks like on the
    // wire: neither witness ever sees the other event.
    home.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--to",
        &first.endpoint,
        "--peer",
        &first.ticket,
    ]);
    fork.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--to",
        &second.endpoint,
        "--peer",
        &second.ticket,
    ]);

    // A third home, holding nothing, asks both.
    let fresh = Home::new("wallet");
    let (code, document) = fresh.failure(&[
        "verify",
        "ledger",
        &alice,
        "--peer",
        &first.ticket,
        "--peer",
        &second.ticket,
    ]);
    assert_eq!(code, 20);
    assert!(
        text(&document["message"]).starts_with("Ledger error: "),
        "{document}"
    );
    assert_eq!(document["details"]["reason"], Value::from("equivocation"));
    assert_eq!(
        document["details"]["ledger_id"],
        Value::from(alice.as_str())
    );
    assert_eq!(document["details"]["at_seq"], Value::from(3));
    let candidates = document["details"]["candidates"]
        .as_array()
        .expect("both sides are named");
    assert_eq!(candidates.len(), 2);
    assert_eq!(
        candidates[0]["source"],
        Value::from(first.endpoint.as_str())
    );
    assert_eq!(
        candidates[1]["source"],
        Value::from(second.endpoint.as_str())
    );
    assert_eq!(
        candidates[0]["event_id"],
        Value::from(text(&one["attestation_event"]))
    );
    assert_eq!(
        candidates[1]["event_id"],
        Value::from(text(&other["attestation_event"]))
    );

    // `verify trust` over the same pair of sources refuses for the same reason,
    // with the same prefix on stderr.
    let (code, document) = fresh.failure(&[
        "verify",
        "trust",
        "--issuer",
        &alice,
        "--subject",
        &bob,
        "--peer",
        &first.ticket,
        "--peer",
        &second.ticket,
    ]);
    assert_eq!(code, 20);
    assert_eq!(document["details"]["reason"], Value::from("equivocation"));
    let (code, stdout, stderr) = fresh.run(&[
        "verify",
        "trust",
        "--issuer",
        &alice,
        "--subject",
        &bob,
        "--peer",
        &first.ticket,
        "--peer",
        &second.ticket,
    ]);
    assert_eq!(code, 20, "{stdout}");
    assert!(stderr.starts_with("Ledger error: "), "{stderr}");

    // One witness alone answers, because one chain on its own is not evidence
    // of anything but itself.
    let single = fresh.json(&["verify", "ledger", &alice, "--peer", &first.ticket]);
    assert_eq!(single["valid"], Value::Bool(true));
    assert_eq!(single["head_seq"], Value::from(3));
    assert_eq!(single["source"], Value::from(first.endpoint.as_str()));

    first.stop();
    second.stop();
}
