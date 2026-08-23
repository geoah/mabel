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

    /// This node's Iroh endpoint id, as every document spells it.
    fn endpoint(&self) -> String {
        text(&self.json(&["node", "id"])["endpoint_id"])
    }

    /// Creates an identity and returns its id.
    fn create(&self, alias: &str) -> String {
        text(&self.json(&["identity", "create", "--alias", alias])["identity_id"])
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

/// A running witness: its home, its endpoint id and the ticket that reaches
/// it.
struct Witness {
    _home: Home,
    endpoint: String,
    ticket: String,
    daemon: Daemon,
}

impl Witness {
    fn start() -> Self {
        let home = Home::new("witness");
        let endpoint = home.endpoint();
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
            endpoint,
            _home: home,
            daemon,
        }
    }

    fn stop(self) {
        assert_eq!(self.daemon.interrupt(), 0, "the witness stops cleanly");
    }
}

/// A wallet home whose ledger names `witness`, with the ledger pushed to it.
fn wallet_with(witness: &Witness) -> (Home, String) {
    let home = Home::new("wallet");
    let alice = home.create("alice");
    home.json(&[
        "witness",
        "add",
        "--identity",
        "alice",
        "--endpoint",
        &witness.endpoint,
    ]);
    (home, alice)
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

    // The text names the witness and what it did.
    let (code, stdout, _) = home.run(&[
        "sync",
        "push",
        "--identity",
        "alice",
        "--peer",
        &witness.ticket,
    ]);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("1 of 1 witnesses accepted"), "{stdout}");
    assert!(stdout.contains("accepted, stored 0"), "{stdout}");

    witness.stop();
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
    home.json(&[
        "witness",
        "add",
        "--identity",
        "alice",
        "--endpoint",
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
    let mut stream = TcpStream::connect(address).expect("the api is listening");
    let mut request = format!(
        "{line}\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n",
        address.port()
    );
    if let Some(body) = body {
        request.push_str(&format!(
            "Origin: http://127.0.0.1:{}\r\ncontent-type: application/json\r\nContent-Length: {}\r\n",
            address.port(),
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
