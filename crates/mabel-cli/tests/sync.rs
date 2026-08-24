//! The network commands of the CLI: `sync push`, `sync fetch`, `verify` over
//! a peer, and `wallet serve`.
//!
//! Every home here sets `relay: "disabled"` in `node.json` and every peer is
//! dialled through a `--peer` ticket carrying its loopback address, so no test
//! touches DNS, a relay or the internet (proposal 001 section 11). Dialling
//! still names the `EndpointId`; the ticket only says where to look.

#![cfg(unix)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
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

/// An endpoint id nothing in these tests binds.
const NOWHERE: &str = "zbj22dym2k3btlvjftxmj7kwujgwjgovqthhsjl6ixh5qe43mctq";

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

    /// Creates an identity and returns its id.
    fn create(&self, alias: &str) -> String {
        text(&self.json(&["identity", "create", "--alias", alias])["identity_id"])
    }

    /// Founds an identity-rooted ledger under `founder` and returns its id.
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

    /// Writes an identity's descriptor file.
    fn export(&self, identity: &str, out: &Path) {
        self.json(&[
            "identity",
            "export",
            identity,
            "--out",
            &out.display().to_string(),
        ]);
    }

    /// `identities/<id>/meta.json`, or `None` when the home records none.
    fn identity_meta(&self, identity: &str) -> Option<Value> {
        let path = self
            .path()
            .join("identities")
            .join(identity)
            .join("meta.json");
        std::fs::read(path)
            .ok()
            .map(|bytes| serde_json::from_slice(&bytes).expect("meta.json is JSON"))
    }
}

fn parse(stdout: &str) -> Value {
    serde_json::from_str(stdout).unwrap_or_else(|error| panic!("{error}: {stdout}"))
}

fn text(value: &Value) -> String {
    value.as_str().expect("a string").to_owned()
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

    /// Waits until the log holds a line starting with `prefix`, and returns
    /// the rest of that line.
    fn wait_for(&self, prefix: &str) -> String {
        let deadline = Instant::now() + TIMEOUT;
        while Instant::now() < deadline {
            let log = std::fs::read_to_string(&self.log).unwrap_or_default();
            if let Some(line) = log
                .lines()
                .find_map(|line| line.strip_prefix(prefix).map(str::trim))
            {
                return line.to_owned();
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!(
            "no {prefix:?} line within {}s: {}",
            TIMEOUT.as_secs(),
            std::fs::read_to_string(&self.log).unwrap_or_default()
        );
    }

    /// Everything the daemon has written to stderr.
    fn log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
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
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        panic!("the daemon ignored SIGINT: {}", self.log());
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A running witness: its home, its witness identity, its endpoint id and the
/// ticket that reaches it.
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
        // `node.json.witness_for`, which is what makes it take a push for a
        // ledger whose witness set names that identity.
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

/// A wallet home whose ledger names `witness`, with the endpoint that answers
/// for it in `node.json`.
///
/// Two facts, because a push needs both: the chain says which identity may keep
/// the ledger, and `node.json` says which machine to dial for it, which is the
/// bootstrap raw endpoint of proposal 006 section 5.4.
fn wallet_with(witness: &Witness) -> (Home, String) {
    let home = Home::new("wallet");
    let alice = home.create("alice");
    home.json(&[
        "witness",
        "set-default",
        "--witness",
        &witness.identity,
        "--endpoints",
        &witness.endpoint,
    ]);
    name_witness(&home, "alice", witness);
    (home, alice)
}

/// Names `witness`'s identity in `ledger`'s witness set.
///
/// The ticket goes with it, because the append first asks the endpoints
/// `node.json` names where the ledger ends, and reaching one needs its address.
fn name_witness(home: &Home, ledger: &str, witness: &Witness) {
    home.json(&[
        "witness",
        "add",
        "--identity",
        ledger,
        "--witness",
        &witness.identity,
        "--peer",
        &witness.ticket,
    ]);
}

#[test]
fn sync_push_matches_the_fixture_and_the_witness_stores_the_ledger() {
    let witness = Witness::start();
    let (home, alice) = wallet_with(&witness);

    let document = home.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);

    assert_shape(
        &document,
        &fixture("sync-push", "every-witness-accepted"),
        "sync-push",
    );
    assert_eq!(document["identity_id"], Value::from(alice.as_str()));
    assert_eq!(document["ledger_id"], Value::from(alice.as_str()));
    assert_eq!(document["head_seq"], Value::from(1));
    let results = document["results"].as_array().expect("one row per witness");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0]["endpoint"],
        Value::from(witness.endpoint.as_str())
    );
    assert_eq!(results[0]["status"], Value::from("accepted"));
    assert_eq!(results[0]["stored"], Value::from(2));
    // One machine answers for this witness identity, and it is the machine that
    // was just pushed to, so the only evidence for it comes from itself: hinted
    // (proposal 006 section 4.2, condition 4).
    assert_eq!(results[0]["binding"], Value::from("hinted"));

    // The text names the witness and what it did, and the warning about the
    // binding goes to stderr, where it does not disturb the report.
    let (code, stdout, stderr) = home.run(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains("1 of 1 witnesses accepted"), "{stdout}");
    assert!(stdout.contains("accepted, stored 0"), "{stdout}");
    assert!(
        stderr.contains(&format!(
            "warning: nobody's ledger confirms that {} answers for a witness",
            witness.endpoint
        )),
        "a hinted push warns and still succeeds: {stderr}"
    );

    witness.stop();
}

/// Proposal 006 section 4.2: a binding is `verified` only on evidence that did
/// not come from the endpoint it vouches for, which needs a second machine
/// answering for the same witness identity.
///
/// The second machine is also the fleet case of section 4.1: it holds the
/// witness identity's id and a copy of its ledger, and no key of it.
#[test]
fn a_second_machine_for_the_witness_identity_verifies_the_binding() {
    // Both homes exist before either daemon serves: a witness's index is built
    // at startup, so the advertisement naming both machines has to be on the
    // ledger before the first one begins serving it.
    let first_home = Home::new("witness");
    let first_endpoint = first_home.endpoint();
    let identity = first_home.create("keeper");
    let second_home = Home::new("witness");
    let second_endpoint = second_home.endpoint();
    first_home.json(&[
        "identity",
        "endpoints",
        "replace",
        "--identity",
        "keeper",
        "--endpoints",
        &format!("{first_endpoint},{second_endpoint}"),
    ]);
    first_home.set_witness_for(&[&identity]);

    let first_port = free_port();
    let first = Daemon::start(
        &first_home,
        "witness",
        &[
            "witness",
            "run",
            "--http",
            "127.0.0.1:0",
            "--iroh-port",
            &first_port.to_string(),
        ],
    );
    first.wait_for("witness ");
    let first_ticket = ticket(&first_endpoint, first_port);

    // A fetch, not a push: a machine that holds no copy of the witness identity
    // cannot be pushed one, because the copy it is missing is what a push for a
    // ledger it does not store would be admitted under.
    second_home.json(&[
        "sync",
        "fetch",
        &identity,
        "--from",
        &first_endpoint,
        "--peer",
        &first_ticket,
    ]);
    second_home.set_witness_for(&[&identity]);
    let second_port = free_port();
    let second = Daemon::start(
        &second_home,
        "witness",
        &[
            "witness",
            "run",
            "--http",
            "127.0.0.1:0",
            "--iroh-port",
            &second_port.to_string(),
        ],
    );
    second.wait_for("witness ");
    let second_ticket = ticket(&second_endpoint, second_port);

    // Alice names the witness identity once and pushes to both its machines.
    let home = Home::new("wallet");
    home.create("alice");
    home.json(&[
        "witness",
        "set-default",
        "--witness",
        &identity,
        "--endpoints",
        &format!("{first_endpoint},{second_endpoint}"),
    ]);
    home.json(&[
        "witness",
        "add",
        "--identity",
        "alice",
        "--witness",
        &identity,
        "--peer",
        &first_ticket,
        "--peer",
        &second_ticket,
    ]);
    let document = home.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &first_ticket,
        "--peer",
        &second_ticket,
    ]);

    let results = document["results"]
        .as_array()
        .expect("one row per endpoint");
    assert_eq!(results.len(), 2, "{document}");
    let row = |endpoint: &str| {
        results
            .iter()
            .find(|row| row["endpoint"] == *endpoint)
            .unwrap_or_else(|| panic!("no row for {endpoint}: {document}"))
            .clone()
    };
    assert_eq!(
        row(&first_endpoint)["status"],
        Value::from("accepted"),
        "{document}"
    );
    assert_eq!(
        row(&second_endpoint)["status"],
        Value::from("accepted"),
        "{document}"
    );
    // The witness ledger came from the first machine, so it verifies the second
    // machine and says nothing about itself.
    assert_eq!(
        row(&second_endpoint)["binding"],
        Value::from("verified"),
        "{document}"
    );
    assert_eq!(row(&first_endpoint)["binding"], Value::from("hinted"));

    // The binding is on disk under the witness identity, as a derived cache.
    let bindings: Value = serde_json::from_slice(
        &std::fs::read(
            home.path()
                .join("bindings")
                .join(format!("{identity}.json")),
        )
        .expect("the binding file was written"),
    )
    .expect("the binding file is JSON");
    assert_eq!(bindings["identity"], Value::from(identity.as_str()));
    let bound = bindings["endpoints"].as_array().expect("an array");
    assert_eq!(bound.len(), 1, "{bindings}");
    assert!(
        bound[0]["head_seq"].as_u64().is_some_and(|seq| seq >= 1),
        "{bindings}"
    );

    assert_eq!(second.interrupt(), 0, "the second witness stops cleanly");
    assert_eq!(first.interrupt(), 0, "the first witness stops cleanly");
}

#[test]
fn sync_fetch_stores_a_ledger_this_home_never_held() {
    let witness = Witness::start();
    let (publisher, alice) = wallet_with(&witness);
    publisher.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);

    let reader = Home::new("wallet");
    let document = reader.json(&[
        "sync",
        "fetch",
        &alice,
        "--from",
        &witness.endpoint,
        "--peer",
        &witness.ticket,
    ]);

    assert_eq!(document["ledger_id"], Value::from(alice.as_str()));
    assert_eq!(document["source"], Value::from(witness.endpoint.as_str()));
    assert_eq!(document["event_count"], Value::from(2));
    assert_eq!(document["stored"], Value::from(2));
    assert_eq!(document["head_seq"], Value::from(1));
    assert!(document["fetched_at_ms"].as_u64().is_some_and(|ms| ms > 0));
    assert!(
        reader
            .path()
            .join("ledgers")
            .join(&alice)
            .join("head.json")
            .is_file(),
        "the fetched ledger landed under ledgers/"
    );

    // The fetched ledger verifies against the source it came from.
    let report = reader.json(&[
        "verify",
        "ledger",
        &alice,
        "--from",
        &witness.endpoint,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(report["valid"], Value::Bool(true));
    assert_eq!(report["source"], Value::from(witness.endpoint.as_str()));
    assert_eq!(report["head_seq"], Value::from(1));
    assert_eq!(
        report["sources_queried"],
        Value::from(vec![witness.endpoint.as_str()])
    );

    witness.stop();
}

/// The push path and the fetch path both find the witness through `node.json`
/// alone: the chain names a witness identity and `node.json` records the machine
/// that answers for it (proposal 006 sections 5.1 and 5.4).
///
/// No `--from` and no link on either side.
#[test]
fn a_push_and_a_fetch_resolve_the_witness_from_node_json_alone() {
    let witness = Witness::start();
    let (publisher, alice) = wallet_with(&witness);
    // The push reads its endpoints from resolution, not from a flag.
    let pushed = publisher.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);
    let rows = pushed["results"].as_array().expect("an array");
    assert!(
        rows.iter().any(
            |row| row["endpoint"] == *witness.endpoint.as_str() && row["status"] == *"accepted"
        ),
        "{pushed}"
    );

    // A home that holds nothing of alice, with the same one config entry and no
    // hint anywhere.
    let reader = Home::new("wallet");
    reader.json(&[
        "witness",
        "set-default",
        "--witness",
        &witness.identity,
        "--endpoints",
        &witness.endpoint,
    ]);
    let document = reader.json(&["sync", "fetch", &alice, "--peer", &witness.ticket]);
    assert_eq!(document["ledger_id"], Value::from(alice.as_str()));
    assert_eq!(document["source"], Value::from(witness.endpoint.as_str()));
    assert_eq!(document["stored"], Value::from(2));

    witness.stop();
}

/// `--from-witness` names an identity and resolution turns it into endpoints;
/// naming an endpoint too is `conflicting_source`.
#[test]
fn sync_fetch_from_a_witness_resolves_it_and_refuses_both_keys() {
    let witness = Witness::start();
    let (publisher, alice) = wallet_with(&witness);
    publisher.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);

    let reader = Home::new("wallet");
    reader.json(&[
        "witness",
        "set-default",
        "--witness",
        &witness.identity,
        "--endpoints",
        &witness.endpoint,
    ]);
    let document = reader.json(&[
        "sync",
        "fetch",
        &alice,
        "--from-witness",
        &witness.identity,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(document["source"], Value::from(witness.endpoint.as_str()));

    let (code, error) = reader.failure(&[
        "sync",
        "fetch",
        &alice,
        "--from",
        &witness.endpoint,
        "--from-witness",
        &witness.identity,
    ]);
    assert_eq!(code, 2);
    assert_eq!(
        error["details"]["reason"],
        Value::from("conflicting_source")
    );

    // A witness this home can reach no endpoint for is refused by name.
    let stranger = Home::new("wallet");
    let (code, error) =
        stranger.failure(&["sync", "fetch", &alice, "--from-witness", &witness.identity]);
    assert_eq!(code, 2);
    assert_eq!(
        error["details"]["reason"],
        Value::from("unresolvable_witness")
    );

    witness.stop();
}

/// A shared link is the only string a fetch needs: `mabel identity share`
/// names the machine to ask, and `mabel sync fetch <link>` asks it with no
/// `--from` (proposal 006 section 7).
#[test]
fn a_shared_link_round_trips_through_a_fetch_with_no_from() {
    let witness = Witness::start();
    let (publisher, alice) = wallet_with(&witness);
    publisher.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);

    let shared = publisher.json(&[
        "identity",
        "share",
        "alice",
        "--endpoints",
        &witness.endpoint,
    ]);
    assert_eq!(shared["identity_id"], Value::from(alice.as_str()));
    assert_eq!(shared["endpoints_from"], Value::from("flag"));
    let link = text(&shared["link"]);
    assert_eq!(
        link,
        format!("mabel://{alice}?endpoints={}", witness.endpoint)
    );

    let reader = Home::new("wallet");
    let document = reader.json(&["sync", "fetch", &link, "--peer", &witness.ticket]);
    assert_eq!(document["ledger_id"], Value::from(alice.as_str()));
    assert_eq!(document["source"], Value::from(witness.endpoint.as_str()));
    assert_eq!(document["stored"], Value::from(2));

    // The same link on an operand that signs locally warns on stderr, names
    // the flag, and does the work anyway.
    let (code, stdout, stderr) = publisher.run(&[
        "sync",
        "push",
        "--identity",
        &link,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stderr.contains("warning: ignoring"), "{stderr}");
    assert!(stderr.contains("--identity"), "{stderr}");
    assert!(stdout.contains(&alice), "{stdout}");

    // A fetch that names neither a source nor a link is a usage failure.
    let (code, error) = reader.failure(&["sync", "fetch", &alice]);
    assert_eq!(code, 2);
    assert_eq!(error["details"]["reason"], Value::from("missing_argument"));

    witness.stop();
}

/// Ticket 031: the story 002 flow continues on the invitee's home.
///
/// Alice founds a shared ledger and admits bob as a controller. Bob's home
/// fetches that ledger from the witness, and because the folded CONTROLLER set
/// names a key bob holds, the fetch records the `controlled_by` link that makes
/// the ledger actionable. Bob then appends to it signing as himself, and a
/// third home that has never met either of them reads bob as the principal who
/// signed.
#[test]
fn an_admitted_controller_appends_to_the_shared_ledger_from_their_own_home() {
    let witness = Witness::start();
    let exchange = TempDir::new().expect("a temp directory");
    let file = |name: &str| exchange.path().join(name);

    // Alice founds the shared ledger, which holds no key of its own. Both homes
    // record the witness's machine in `node.json`, which is where a push reads
    // the endpoints to dial.
    let alice_home = Home::new("wallet");
    alice_home.json(&[
        "witness",
        "set-default",
        "--witness",
        &witness.identity,
        "--endpoints",
        &witness.endpoint,
    ]);
    alice_home.create("alice");
    let acme = alice_home.found("acme", "alice");

    // Bob lives in another home and shares no disk with alice.
    let bob_home = Home::new("wallet");
    bob_home.json(&[
        "witness",
        "set-default",
        "--witness",
        &witness.identity,
        "--endpoints",
        &witness.endpoint,
    ]);
    let bob = bob_home.create("bob");
    let descriptor = file("bob.descriptor");
    bob_home.export("bob", &descriptor);

    // Invite, accept, admit: the three artifacts of story 002.
    let bundle = file("acme.invitation");
    alice_home.json(&[
        "membership",
        "invite",
        "--ledger",
        "acme",
        "--by",
        "alice",
        "--invitee",
        &descriptor.display().to_string(),
        "--role",
        "controller",
        "--out",
        &bundle.display().to_string(),
    ]);
    let acceptance = file("bob.acceptance");
    bob_home.json(&[
        "membership",
        "accept",
        &bundle.display().to_string(),
        "--as",
        "bob",
        "--out",
        &acceptance.display().to_string(),
        "--yes",
    ]);
    let admitted = alice_home.json(&[
        "membership",
        "admit",
        "--ledger",
        "acme",
        "--by",
        "alice",
        &acceptance.display().to_string(),
    ]);
    assert_eq!(admitted["invitee"], Value::from(bob.as_str()));
    assert_eq!(admitted["acceptance_seq"], Value::from(2));

    // The shared ledger names the witness and is pushed to it.
    name_witness(&alice_home, "acme", &witness);
    alice_home.json(&[
        "sync",
        "push",
        "--identity",
        "acme",
        "--peer",
        &witness.ticket,
    ]);

    // Bob fetches the shared ledger, which links to the identity that signs.
    let fetched = bob_home.json(&[
        "sync",
        "fetch",
        &acme,
        "--from",
        &witness.endpoint,
        "--peer",
        &witness.ticket,
    ]);
    assert_shape(
        &fetched,
        &fixture("sync-fetch", "locally-controlled"),
        "sync-fetch",
    );
    assert_eq!(fetched["controlled_by"], Value::from(bob.as_str()));
    assert_eq!(fetched["head_seq"], Value::from(3));

    // The link is on disk, and it carries no key of its own: bob's own active
    // key is what signs for the shared ledger.
    let meta = bob_home.identity_meta(&acme).expect("the link was written");
    assert_eq!(meta["controlled_by"], Value::from(bob.as_str()));
    assert_eq!(meta["declared_kind"], Value::from("organization"));
    let directory = bob_home.path().join("identities").join(&acme);
    assert!(!directory.join("active.key").exists(), "no key was written");
    assert!(
        !directory.join("reserve.key").exists(),
        "no key was written"
    );

    // Bob appends to it from his own home, signing as himself, and pushes.
    let attested = bob_home.json(&[
        "trust",
        "add",
        "--issuer",
        &acme,
        "--subject",
        &bob,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(attested["issuer"], Value::from(acme.as_str()));
    assert_eq!(attested["attestation_seq"], Value::from(4));
    bob_home.json(&[
        "sync",
        "push",
        "--identity",
        &acme,
        "--peer",
        &witness.ticket,
    ]);

    // A home that has met neither of them reads bob as the signing principal.
    let stranger = Home::new("wallet");
    let report = stranger.json(&[
        "verify",
        "trust",
        "--issuer",
        &acme,
        "--subject",
        &bob,
        "--from",
        &witness.endpoint,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(report["trusted"], Value::Bool(true));
    assert_eq!(
        report["signing_principal"]["identity"],
        Value::from(bob.as_str()),
        "the shared ledger holds no key, so bob signed for it"
    );

    witness.stop();
}

/// Ticket 031: a fetched ledger no local key controls stays read-only, and the
/// refusal names the ledger rather than claiming the home never heard of it.
#[test]
fn a_fetched_ledger_with_no_local_controller_key_is_refused_by_name() {
    let witness = Witness::start();
    let (publisher, alice) = wallet_with(&witness);
    publisher.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);

    let reader = Home::new("wallet");
    reader.create("carol");
    let fetched = reader.json(&[
        "sync",
        "fetch",
        &alice,
        "--from",
        &witness.endpoint,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(fetched["controlled_by"], Value::Null);
    assert!(reader.identity_meta(&alice).is_none(), "no link is written");
    let (code, stdout, _) = reader.run(&[
        "sync",
        "fetch",
        &alice,
        "--from",
        &witness.endpoint,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("stored read-only: no identity here controls it"),
        "{stdout}"
    );

    // Every command that resolves a ledger it must act as says the same thing.
    for arguments in [
        vec!["trust", "add", "--issuer", &alice, "--subject", "carol"],
        // A ledger may name itself as its own witness, so the only thing
        // wrong here is that this home cannot sign for it.
        vec!["witness", "add", "--identity", &alice, "--witness", &alice],
        vec!["sync", "push", "--identity", &alice, "--to", NOWHERE],
    ] {
        let (code, document) = reader.failure(&arguments);
        assert_eq!(code, 2, "{arguments:?}: {document}");
        assert_eq!(
            document["details"]["reason"],
            Value::from("not_locally_controlled"),
            "{arguments:?}: {document}"
        );
        assert_eq!(
            document["details"]["ledger_id"],
            Value::from(alice.as_str())
        );
        assert!(
            text(&document["message"]).contains(&alice),
            "{arguments:?}: {document}"
        );
    }

    // An id nothing here holds is still unknown, not read-only.
    let (code, document) =
        reader.failure(&["trust", "add", "--issuer", NOWHERE, "--subject", "carol"]);
    assert_eq!(code, 2);
    assert_eq!(
        document["details"]["reason"],
        Value::from("unknown_identity")
    );

    witness.stop();
}

/// Ticket 031: losing the controller role downgrades the ledger to read-only
/// at the next append, which runs the freshness query first.
#[test]
fn a_removed_controllers_append_is_refused_after_the_removal_lands() {
    let witness = Witness::start();
    let exchange = TempDir::new().expect("a temp directory");
    let file = |name: &str| exchange.path().join(name);

    let alice_home = Home::new("wallet");
    alice_home.json(&[
        "witness",
        "set-default",
        "--witness",
        &witness.identity,
        "--endpoints",
        &witness.endpoint,
    ]);
    alice_home.create("alice");
    let acme = alice_home.found("acme", "alice");
    let bob_home = Home::new("wallet");
    bob_home.json(&[
        "witness",
        "set-default",
        "--witness",
        &witness.identity,
        "--endpoints",
        &witness.endpoint,
    ]);
    let bob = bob_home.create("bob");
    let descriptor = file("bob.descriptor");
    bob_home.export("bob", &descriptor);
    let bundle = file("acme.invitation");
    alice_home.json(&[
        "membership",
        "invite",
        "--ledger",
        "acme",
        "--by",
        "alice",
        "--invitee",
        &descriptor.display().to_string(),
        "--role",
        "controller",
        "--out",
        &bundle.display().to_string(),
    ]);
    let acceptance = file("bob.acceptance");
    bob_home.json(&[
        "membership",
        "accept",
        &bundle.display().to_string(),
        "--as",
        "bob",
        "--out",
        &acceptance.display().to_string(),
        "--yes",
    ]);
    alice_home.json(&[
        "membership",
        "admit",
        "--ledger",
        "acme",
        "--by",
        "alice",
        &acceptance.display().to_string(),
    ]);
    name_witness(&alice_home, "acme", &witness);
    alice_home.json(&[
        "sync",
        "push",
        "--identity",
        "acme",
        "--peer",
        &witness.ticket,
    ]);
    let fetched = bob_home.json(&[
        "sync",
        "fetch",
        &acme,
        "--from",
        &witness.endpoint,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(fetched["controlled_by"], Value::from(bob.as_str()));

    // Alice takes the controller role away and pushes the removal.
    let removed = alice_home.json(&[
        "membership",
        "remove",
        "--ledger",
        "acme",
        "--by",
        "alice",
        "--member",
        &bob,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(removed["principal_removed"], Value::Bool(true));
    alice_home.json(&[
        "sync",
        "push",
        "--identity",
        "acme",
        "--peer",
        &witness.ticket,
    ]);

    // Bob's append fast-forwards to the removal, which drops the link.
    let (code, document) = bob_home.failure(&[
        "trust",
        "add",
        "--issuer",
        &acme,
        "--subject",
        &bob,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(code, 2, "{document}");
    assert_eq!(
        document["details"]["reason"],
        Value::from("not_locally_controlled")
    );
    assert_eq!(document["details"]["ledger_id"], Value::from(acme.as_str()));
    assert_eq!(
        bob_home.identity_meta(&acme).expect("the metadata stays")["controlled_by"],
        Value::Null,
        "the link is dropped, the ledger is not"
    );

    // The ledger itself is still readable, and a re-fetch keeps it read-only.
    let refetched = bob_home.json(&[
        "sync",
        "fetch",
        &acme,
        "--from",
        &witness.endpoint,
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(refetched["controlled_by"], Value::Null);
    assert_eq!(refetched["head_seq"], Value::from(4));

    witness.stop();
}

#[test]
fn verify_over_the_network_reports_the_witness_as_its_source() {
    let witness = Witness::start();
    let (home, alice) = wallet_with(&witness);
    let bob = home.create("bob");
    home.json(&["trust", "add", "--issuer", "alice", "--subject", "bob"]);
    home.json(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);

    // No --from: the ledger names a witness, so the witness is queried.
    let document = home.json(&[
        "verify",
        "trust",
        "--issuer",
        "alice",
        "--subject",
        "bob",
        "--peer",
        &witness.ticket,
    ]);

    assert_shape(
        &document,
        &fixture("verify-trust", "trusted"),
        "verify-trust",
    );
    assert_eq!(document["trusted"], Value::Bool(true));
    assert_eq!(document["issuer"], Value::from(alice.as_str()));
    assert_eq!(document["subject"], Value::from(bob.as_str()));
    assert_eq!(document["source"], Value::from(witness.endpoint.as_str()));
    assert_eq!(document["subject_resolution"], Value::from("resolved"));
    // Proposal 002 section 5: the answer names who signed the attestation.
    assert_eq!(
        document["signing_principal"]["identity"],
        Value::from(alice.as_str())
    );
    assert!(
        document["signing_principal"]["key"].as_str().is_some(),
        "{document}"
    );

    // The text says it too, so a person reading the terminal sees the signer.
    let (code, stdout, stderr) = home.run(&[
        "verify",
        "trust",
        "--issuer",
        "alice",
        "--subject",
        "bob",
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        stdout.contains(&format!("signed by principal {alice} (")),
        "{stdout}"
    );

    witness.stop();
}

#[test]
fn a_peer_that_cannot_be_reached_exits_30_with_the_network_prefix() {
    let home = Home::new("wallet");
    home.create("alice");
    // Nothing answers at this endpoint, and `node.json` is where a push reads
    // the machines to dial.
    let nobody = home.create("nobody");
    home.json(&[
        "witness",
        "set-default",
        "--witness",
        &nobody,
        "--endpoints",
        NOWHERE,
    ]);

    let (code, document) = home.failure(&["sync", "push", "--identity", "alice"]);
    assert_eq!(code, 30);
    assert_eq!(
        document["details"]["reason"],
        Value::from("all_witnesses_failed")
    );
    let message = text(&document["message"]);
    assert!(message.starts_with("Network error: "), "{message}");
    let results = document["details"]["results"]
        .as_array()
        .expect("the failures are listed per endpoint");
    assert_eq!(results[0]["status"], Value::from("unreachable"));
    assert!(
        text(&results[0]["message"]).starts_with("Network error: no route to"),
        "{document}"
    );

    // Text mode prints the same prefixed line on stderr.
    let (code, stdout, stderr) = home.run(&["sync", "push", "--identity", "alice"]);
    assert_eq!(code, 30, "{stdout}{stderr}");
    assert!(stderr.starts_with("Network error: "), "{stderr}");

    let (code, document) = home.failure(&["sync", "fetch", &home.create("bob"), "--from", NOWHERE]);
    assert_eq!(code, 30);
    assert_eq!(
        document["details"]["reason"],
        Value::from("peer_unreachable")
    );
    assert!(
        text(&document["message"]).starts_with("Network error: no route to"),
        "{document}"
    );
}

#[test]
fn wallet_serve_answers_the_node_route_and_stops_on_sigint() {
    let home = Home::new("wallet");
    let alice = home.create("alice");
    let daemon = Daemon::start(
        &home,
        "wallet",
        &["wallet", "serve", "--http", "127.0.0.1:0"],
    );

    let endpoint = daemon.wait_for("wallet ");
    assert_eq!(endpoint, home.endpoint(), "the endpoint id is printed");
    let address: SocketAddr = daemon
        .wait_for("http ")
        .parse()
        .expect("the http line carries an address");
    assert!(
        daemon.log().contains("holding 1 identities"),
        "{}",
        daemon.log()
    );

    let node = get(address, "/api/node");
    assert_eq!(node["ok"], Value::Bool(true));
    assert_eq!(node["role"], Value::from("wallet"));
    assert_eq!(node["endpoint_id"], Value::from(endpoint.as_str()));
    assert_eq!(node["identity_count"], Value::from(1));
    assert_eq!(node["relay"], Value::from("disabled"));

    // The real service answers from the home, not from a fixture.
    let identities = get(address, "/api/identities");
    let held = identities["identities"].as_array().expect("an array");
    assert_eq!(held.len(), 1);
    assert_eq!(held[0]["identity_id"], Value::from(alice.as_str()));
    assert_eq!(held[0]["alias"], Value::from("alice"));

    // The membership routes of ticket 021 are served by the same wallet
    // process, and validate their bodies rather than answering 501.
    let (status, body) = request(
        address,
        &format!("POST /api/identities/{alice}/memberships/invitations HTTP/1.1"),
        Some("{}"),
    );
    assert_eq!(status, 400, "{body}");
    let answer = parse(&body);
    assert_eq!(answer["details"]["reason"], Value::from("missing_field"));
    assert_eq!(answer["details"]["field"], Value::from("by"));

    assert_eq!(daemon.interrupt(), 0, "the wallet stops cleanly");
}

/// `--ui-dir` serves the files in that directory instead of the bundle
/// compiled into the binary (ticket 012).
#[test]
fn wallet_serve_ui_dir_serves_the_files_in_that_directory() {
    let home = Home::new("wallet");
    home.create("alice");
    let ui = home.path().join("ui-dist");
    std::fs::create_dir_all(&ui).expect("the ui directory is created");
    let page = "<!doctype html><title>ui-dir</title><p>served from disk";
    std::fs::write(ui.join("index.html"), page).expect("index.html is written");
    std::fs::write(ui.join("app.js"), "export const from_disk = true;\n").expect("app.js");

    let ui_dir = ui.to_str().expect("a utf-8 path");
    let daemon = Daemon::start(
        &home,
        "wallet",
        &[
            "wallet",
            "serve",
            "--http",
            "127.0.0.1:0",
            "--ui-dir",
            ui_dir,
        ],
    );
    let address: SocketAddr = daemon
        .wait_for("http ")
        .parse()
        .expect("the http line carries an address");

    let (status, body) = request(address, "GET / HTTP/1.1", None);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, page, "the index comes from the directory");

    let (status, body) = request(address, "GET /app.js HTTP/1.1", None);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, "export const from_disk = true;\n");

    // The JSON API is unchanged by the flag.
    assert_eq!(get(address, "/api/node")["role"], Value::from("wallet"));

    assert_eq!(daemon.interrupt(), 0, "the wallet stops cleanly");
}

/// `--allow-host` is the one way a node answers a name that is not loopback,
/// and it changes nothing else: every other host is refused exactly as before
/// (decision 018).
#[test]
fn wallet_serve_allow_host_accepts_that_host_and_no_other() {
    let home = Home::new("wallet");
    home.create("alice");
    let daemon = Daemon::start(
        &home,
        "wallet",
        &[
            "wallet",
            "serve",
            "--http",
            "127.0.0.1:0",
            "--allow-host",
            "wallet.tailnet.example",
        ],
    );
    let address: SocketAddr = daemon
        .wait_for("http ")
        .parse()
        .expect("the http line carries an address");
    assert!(
        daemon
            .log()
            .contains("wallet.tailnet.example is accepted beyond loopback"),
        "the process says what it accepts: {}",
        daemon.log()
    );

    // The allowed host reads, and its https origin passes on a mutating route.
    let (status, body) = request_as(
        address,
        "GET /api/node HTTP/1.1",
        "wallet.tailnet.example",
        None,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(parse(&body)["role"], Value::from("wallet"));
    let (status, body) = request_as(
        address,
        "POST /api/identities HTTP/1.1",
        "wallet.tailnet.example",
        Some(r#"{"alias": "bob"}"#),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(parse(&body)["identity"]["alias"], Value::from("bob"));

    // Loopback still works, and every other host is still refused.
    assert_eq!(get(address, "/api/node")["role"], Value::from("wallet"));
    let (status, body) = request_as(address, "GET /api/node HTTP/1.1", "evil.example", None);
    assert_eq!(status, 403, "{body}");
    let refused = parse(&body);
    assert_eq!(
        refused["details"]["reason"],
        Value::from("host_not_loopback")
    );
    assert_eq!(
        refused["message"],
        Value::from(
            "request rejected: Host header must be 127.0.0.1:9080, localhost:9080 \
             or wallet.tailnet.example"
                .replace("9080", &address.port().to_string())
        )
    );

    assert_eq!(daemon.interrupt(), 0, "the wallet stops cleanly");
}

/// Without the flag the default is unchanged: loopback alone.
#[test]
fn wallet_serve_without_allow_host_refuses_every_name() {
    let home = Home::new("wallet");
    let daemon = Daemon::start(
        &home,
        "wallet",
        &["wallet", "serve", "--http", "127.0.0.1:0"],
    );
    let address: SocketAddr = daemon
        .wait_for("http ")
        .parse()
        .expect("the http line carries an address");
    assert!(
        !daemon.log().contains("beyond loopback"),
        "nothing is accepted beyond loopback: {}",
        daemon.log()
    );

    let (status, body) = request_as(
        address,
        "GET /api/node HTTP/1.1",
        "wallet.tailnet.example",
        None,
    );
    assert_eq!(status, 403, "{body}");
    assert_eq!(
        parse(&body)["message"],
        Value::from(format!(
            "request rejected: Host header must be 127.0.0.1:{port} or localhost:{port}",
            port = address.port()
        ))
    );

    assert_eq!(daemon.interrupt(), 0, "the wallet stops cleanly");
}

/// `node.json`'s `allowed_hosts` and `--allow-host` merge, so a one-off flag
/// never drops the set the file records (decision 018).
#[test]
fn allowed_hosts_in_node_json_and_the_flag_are_both_accepted() {
    let home = Home::new("wallet");
    let path = home.path().join("node.json");
    let mut config: Value =
        serde_json::from_slice(&std::fs::read(&path).expect("node.json reads")).expect("json");
    config["allowed_hosts"] = Value::from(vec!["from-the-file.example"]);
    std::fs::write(&path, serde_json::to_vec_pretty(&config).expect("json")).expect("written");

    let daemon = Daemon::start(
        &home,
        "wallet",
        &[
            "wallet",
            "serve",
            "--http",
            "127.0.0.1:0",
            "--allow-host",
            "from-the-flag.example",
        ],
    );
    let address: SocketAddr = daemon
        .wait_for("http ")
        .parse()
        .expect("the http line carries an address");

    for host in ["from-the-file.example", "from-the-flag.example"] {
        let (status, body) = request_as(address, "GET /api/node HTTP/1.1", host, None);
        assert_eq!(status, 200, "{host}: {body}");
    }
    let (status, _) = request_as(address, "GET /api/node HTTP/1.1", "other.example", None);
    assert_eq!(status, 403);

    assert_eq!(daemon.interrupt(), 0, "the wallet stops cleanly");
}

/// The `--peer` half of `mabel dev seed`: every seeded ledger names the
/// witness, is pushed to it, and one crawl runs so `mabel lookup` answers with
/// degrees rather than "no path in this crawl".
#[test]
fn dev_seed_with_a_witness_pushes_every_ledger_and_runs_one_crawl() {
    let witness = Witness::start();
    let home = Home::new("wallet");

    // The ticket says which machine to push to; `--witness` says whose ledger
    // that machine keeps, which is what the push is admitted under.
    let document = home.json(&[
        "dev",
        "seed",
        "--peer",
        &witness.ticket,
        "--witness",
        &witness.identity,
    ]);
    assert_shape(
        &document,
        &fixture("dev-seed", "seeded-with-a-witness"),
        "dev-seed/with-a-witness",
    );
    // The seed creates its own witness identity and names it beside the one
    // `--witness` gave, and neither is an endpoint id (proposal 006 section 1).
    let seeded_witness = text(&document["witnesses"][0]);
    assert_eq!(
        document["witnesses"],
        Value::Array(vec![
            Value::from(seeded_witness.as_str()),
            Value::from(witness.identity.as_str())
        ])
    );
    let witness_identity = document["identities"]
        .as_array()
        .expect("an array")
        .iter()
        .find(|identity| identity["alias"] == *"witness")
        .expect("the seed created a witness identity")
        .clone();
    assert_eq!(
        witness_identity["identity_id"],
        Value::from(seeded_witness.as_str())
    );
    assert_eq!(
        witness_identity["declared_kind"],
        Value::from("service"),
        "{witness_identity}"
    );

    // One push per seeded ledger, every one accepted by the one witness.
    let pushed = document["pushed"].as_array().expect("an array");
    assert_eq!(pushed.len(), 5, "{document}");
    // The seeded witness identity advertises this home's own machine, and this
    // home is a CLI and not a server, so resolution finds that endpoint too and
    // its row is unreachable. Exactly one endpoint accepts: the ticket's.
    for result in pushed {
        let rows = result["results"].as_array().expect("an array");
        let accepted: Vec<&Value> = rows
            .iter()
            .filter(|row| row["status"] == *"accepted")
            .collect();
        assert_eq!(accepted.len(), 1, "{result}");
        assert_eq!(
            accepted[0]["endpoint"],
            Value::from(witness.endpoint.as_str())
        );
    }

    // Every seeded ledger names the witness identity, so the witness set landed
    // on all five before the push, the witness's own ledger included.
    for identity in document["identities"].as_array().expect("an array") {
        assert_eq!(
            identity["witnesses"],
            Value::Array(vec![
                Value::from(seeded_witness.as_str()),
                Value::from(witness.identity.as_str())
            ]),
            "{identity}"
        );
    }
    // `node.json.witnesses`, which `iroh_base` spells as hex rather than as the
    // base32 every document uses.
    let config: Value =
        serde_json::from_slice(&std::fs::read(home.path().join("node.json")).expect("node.json"))
            .expect("node.json is JSON");
    let defaults: Vec<String> = config["witnesses"]
        .as_array()
        .expect("witnesses is an array")
        .iter()
        .flat_map(|entry| {
            entry["endpoints"]
                .as_array()
                .expect("endpoints is an array")
                .iter()
                .map(|value| {
                    let bytes = data_encoding::HEXLOWER
                        .decode(text(value).as_bytes())
                        .expect("a hex endpoint id");
                    data_encoding::BASE32_NOPAD.encode(&bytes).to_lowercase()
                })
                .collect::<Vec<String>>()
        })
        .collect();
    assert_eq!(
        defaults,
        vec![witness.endpoint.clone()],
        "node.json names the witness as a default too"
    );

    // The crawl ran, and the graph it stored is the one `graph status` reports.
    let graph = &document["graph"];
    assert_eq!(graph["depth"], Value::from(2));
    assert!(
        graph["node_count"].as_u64().is_some_and(|count| count >= 5),
        "the crawl reached the five seeded identities: {graph}"
    );
    assert_eq!(
        home.json(&["graph", "status"])["graph"]["sync_id"],
        graph["sync_id"]
    );

    // A lookup answers with degrees, which is what the crawl is for: alice
    // attests bob, so bob is one hop from alice.
    let bob = text(&document["identities"][1]["identity_id"]);
    let lookup = home.json(&["lookup", &bob, "--from", "alice"]);
    assert_eq!(lookup["degrees"], Value::from(1), "{lookup}");

    witness.stop();
}

/// One `GET` against the loopback API, parsed.
fn get(address: SocketAddr, path: &str) -> Value {
    let (status, body) = request(address, &format!("GET {path} HTTP/1.1"), None);
    assert_eq!(status, 200, "{body}");
    parse(&body)
}

/// One raw HTTP request against the loopback API.
///
/// The loopback rules need a matching `Host`, an `Origin` on a mutating
/// request and `content-type: application/json` on one with a body (proposal
/// 001 section 10), so the request is written by hand rather than through a
/// client that would add none of them.
fn request(address: SocketAddr, line: &str, body: Option<&str>) -> (u16, String) {
    let host = format!("127.0.0.1:{}", address.port());
    raw(address, line, &host, "http", body)
}

/// One raw HTTP request naming `host`, for the `--allow-host` rules.
///
/// The `Origin` matches the `Host` over `https`, which is what a request
/// arriving through a TLS-terminating proxy carries (decision 018).
fn request_as(address: SocketAddr, line: &str, host: &str, body: Option<&str>) -> (u16, String) {
    raw(address, line, host, "https", body)
}

fn raw(
    address: SocketAddr,
    line: &str,
    host: &str,
    scheme: &str,
    body: Option<&str>,
) -> (u16, String) {
    let mut stream = TcpStream::connect(address).expect("the api is listening");
    let mut request = format!("{line}\r\nHost: {host}\r\nConnection: close\r\n");
    if let Some(body) = body {
        request.push_str(&format!(
            "Origin: {scheme}://{host}\r\ncontent-type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    request.push_str("\r\n");
    if let Some(body) = body {
        request.push_str(body);
    }
    stream
        .write_all(request.as_bytes())
        .expect("the request writes");
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .expect("the answer is utf-8");
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("no header break in {answer}"));
    let status = head
        .lines()
        .next()
        .and_then(|status| status.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("no status in {head}"));
    (status, body.to_owned())
}
