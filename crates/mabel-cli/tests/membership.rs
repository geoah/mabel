//! The membership commands and the file artifacts (ticket 018).
//!
//! Two temp homes stand for two machines: the inviter holds one identity, the
//! invitee another, and the only thing that crosses between them is a file.
//! The `--json` assertions name the keys of each document here; the frozen
//! copies are `contracts/cli/membership-*.json` and their HTTP counterparts,
//! indexed in `contracts/README.md`.
//!
//! The exit codes this ticket owns are 0, 2, 10, 20 and 50.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

/// A temp node home and the binary that runs against it.
struct Home {
    directory: TempDir,
}

impl Home {
    fn new() -> Self {
        Self {
            directory: TempDir::new().expect("a temp directory"),
        }
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
        finish(&mut self.command(arguments))
    }

    /// Runs a command with `answer` on stdin, for the accept confirmation.
    fn answer(&self, arguments: &[&str], answer: &str) -> (i32, String, String) {
        finish(self.command(arguments).write_stdin(answer.to_owned()))
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

    /// Writes an identity's descriptor file.
    fn export(&self, identity: &str, out: &Path) -> Value {
        self.json(&[
            "identity",
            "export",
            identity,
            "--out",
            &out.display().to_string(),
        ])
    }
}

/// The directory the two homes exchange files through.
struct Exchange {
    directory: TempDir,
}

impl Exchange {
    fn new() -> Self {
        Self {
            directory: TempDir::new().expect("a temp directory"),
        }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }
}

fn finish(command: &mut Command) -> (i32, String, String) {
    let output = command.output().expect("the binary runs");
    (
        output.status.code().expect("the process exited"),
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        String::from_utf8(output.stderr).expect("stderr is utf-8"),
    )
}

fn parse(stdout: &str) -> Value {
    serde_json::from_str(stdout).unwrap_or_else(|error| panic!("{error}: {stdout}"))
}

fn text(value: &Value) -> String {
    value.as_str().expect("a string").to_owned()
}

fn path(value: &Path) -> String {
    value.display().to_string()
}

fn is_id(value: &Value) -> bool {
    value.as_str().is_some_and(|id| {
        id.len() == 52
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c))
    })
}

/// Asserts that a document carries exactly these keys.
fn assert_keys(document: &Value, expected: &[&str]) {
    let mut actual: Vec<&str> = document
        .as_object()
        .unwrap_or_else(|| panic!("not an object: {document}"))
        .keys()
        .map(String::as_str)
        .collect();
    actual.sort_unstable();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected, "{document}");
}

/// One invitation, accepted and admitted: the flow every test starts from.
struct Flow {
    inviter: Home,
    invitee: Home,
    exchange: Exchange,
    ledger: String,
    invited: String,
    bundle: PathBuf,
    acceptance: PathBuf,
}

impl Flow {
    /// Alice invites Bob, who lives in another home, and Bob accepts.
    ///
    /// Nothing is admitted yet: the tests that follow either admit, replay or
    /// misuse the acceptance file.
    fn accepted(role: &str) -> Self {
        let inviter = Home::new();
        let invitee = Home::new();
        let exchange = Exchange::new();
        let ledger = inviter.create("alice");
        let invited = invitee.create("bob");

        let descriptor = exchange.file("bob.descriptor");
        invitee.export("bob", &descriptor);
        let bundle = exchange.file("alice.invitation");
        inviter.json(&[
            "membership",
            "invite",
            "--ledger",
            "alice",
            "--by",
            "alice",
            "--invitee",
            &path(&descriptor),
            "--role",
            role,
            "--out",
            &path(&bundle),
        ]);
        let acceptance = exchange.file("bob.acceptance");
        invitee.json(&[
            "membership",
            "accept",
            &path(&bundle),
            "--as",
            "bob",
            "--out",
            &path(&acceptance),
            "--yes",
        ]);
        Self {
            inviter,
            invitee,
            exchange,
            ledger,
            invited,
            bundle,
            acceptance,
        }
    }

    fn admit(&self) -> Value {
        self.inviter.json(&[
            "membership",
            "admit",
            "--ledger",
            "alice",
            "--by",
            "alice",
            &path(&self.acceptance),
        ])
    }
}

#[test]
fn the_invite_accept_admit_flow_runs_between_two_homes() {
    let flow = Flow::accepted("controller");

    // The invitation document names what it appended and what it wrote.
    let invited = flow
        .inviter
        .json(&["membership", "list", "--ledger", "alice"]);
    let invitation = &invited["invitations"][0];
    assert_eq!(invitation["invitee"], Value::from(flow.invited.clone()));
    assert_eq!(invitation["role"], Value::from("controller"));
    assert_eq!(invitation["status"], Value::from("open"));

    let admitted = flow.admit();
    assert_keys(
        &admitted,
        &[
            "ok",
            "ledger_id",
            "by",
            "invitee",
            "invitee_key",
            "role",
            "invitation_event",
            "acceptance_event",
            "acceptance_seq",
            "timestamp_ms",
            "head_seq",
            "head_event",
            "path",
        ],
    );
    assert_eq!(admitted["ledger_id"], Value::from(flow.ledger.clone()));
    assert_eq!(admitted["invitee"], Value::from(flow.invited.clone()));
    assert_eq!(admitted["role"], Value::from("controller"));
    assert_eq!(admitted["acceptance_seq"], Value::from(2));
    assert_eq!(admitted["head_seq"], admitted["acceptance_seq"]);
    assert_eq!(admitted["invitation_event"], invitation["invitation_event"]);

    // The fold now records two controllers, and the invitation is spent.
    let listed = flow
        .inviter
        .json(&["membership", "list", "--ledger", "alice"]);
    assert_keys(
        &listed,
        &[
            "ok",
            "ledger_id",
            "declared_kind",
            "root",
            "head_seq",
            "head_event",
            "principals",
            "invitations",
        ],
    );
    assert_eq!(listed["root"], Value::from("raw"));
    assert_eq!(listed["invitations"][0]["status"], Value::from("accepted"));
    let principals = listed["principals"].as_array().expect("an array");
    assert_eq!(principals.len(), 2, "{listed}");
    assert_keys(
        &principals[0],
        &["identity", "active_key", "role", "is_root"],
    );
    let root = principals
        .iter()
        .find(|entry| entry["identity"] == *flow.ledger)
        .expect("the raw root is a principal");
    assert_eq!(root["is_root"], Value::Bool(true));
    let delegate = principals
        .iter()
        .find(|entry| entry["identity"] == *flow.invited)
        .expect("bob is a principal");
    assert_eq!(delegate["role"], Value::from("controller"));
    assert_eq!(delegate["is_root"], Value::Bool(false));

    // The text rendering says the same thing.
    let (code, stdout, _) = flow
        .inviter
        .run(&["membership", "list", "--ledger", "alice"]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("2 principals, 0 open invitations"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("controller {} ", flow.invited)),
        "{stdout}"
    );
}

#[test]
fn the_invitation_document_and_bundle_name_the_invitee_from_the_descriptor() {
    let inviter = Home::new();
    let invitee = Home::new();
    let exchange = Exchange::new();
    let ledger = inviter.create("alice");
    let invited = invitee.create("bob");
    let descriptor = exchange.file("bob.descriptor");
    let exported = invitee.export("bob", &descriptor);
    let bundle = exchange.file("alice.invitation");

    let document = inviter.json(&[
        "membership",
        "invite",
        "--ledger",
        "alice",
        "--by",
        "alice",
        "--invitee",
        &path(&descriptor),
        "--role",
        "member",
        "--out",
        &path(&bundle),
    ]);
    assert_keys(
        &document,
        &[
            "ok",
            "ledger_id",
            "by",
            "invitee",
            "invitee_key",
            "role",
            "invitation_event",
            "invitation_seq",
            "timestamp_ms",
            "head_seq",
            "head_event",
            "path",
            "bytes",
            "event_count",
        ],
    );
    assert_eq!(document["ledger_id"], Value::from(ledger));
    assert_eq!(document["by"], document["ledger_id"]);
    assert_eq!(document["invitee"], Value::from(invited));
    assert_eq!(document["invitee_key"], exported["active_key"]);
    assert_eq!(document["role"], Value::from("member"));
    assert_eq!(document["invitation_seq"], Value::from(1));
    assert_eq!(document["head_event"], document["invitation_event"]);
    assert_eq!(document["event_count"], Value::from(2));
    assert_eq!(document["path"], Value::from(path(&bundle)));
    assert!(is_id(&document["invitation_event"]), "{document}");
    assert_eq!(
        std::fs::metadata(&bundle).expect("the bundle exists").len(),
        document["bytes"].as_u64().expect("a number")
    );
}

#[test]
fn identity_export_round_trips_and_carries_the_configured_witnesses() {
    let home = Home::new();
    let exchange = Exchange::new();
    let alice = home.create("alice");
    let endpoint = text(&home.json(&["node", "id"])["endpoint_id"]);
    home.json(&[
        "witness",
        "add",
        "--identity",
        "alice",
        "--endpoint",
        &endpoint,
    ]);

    let descriptor = exchange.file("alice.descriptor");
    let document = home.export("alice", &descriptor);
    assert_keys(
        &document,
        &[
            "ok",
            "identity_id",
            "declared_kind",
            "root",
            "active_key",
            "witnesses",
            "path",
            "bytes",
        ],
    );
    assert_eq!(document["identity_id"], Value::from(alice.clone()));
    assert_eq!(document["declared_kind"], Value::from("person"));
    assert_eq!(document["root"], Value::from("raw"));
    assert_eq!(document["witnesses"], Value::from(vec![endpoint]));
    assert!(is_id(&document["active_key"]), "{document}");

    // The same ledger exports the same bytes, and the id resolves like the
    // alias.
    let again = exchange.file("alice-again.descriptor");
    let repeated = home.export(&alice, &again);
    assert_eq!(repeated["bytes"], document["bytes"]);
    assert_eq!(
        std::fs::read(&descriptor).expect("the file"),
        std::fs::read(&again).expect("the file")
    );

    // An identity-rooted ledger holds no key of its own, so its descriptor
    // reports none.
    home.found("acme", "alice");
    let organization = home.export("acme", &exchange.file("acme.descriptor"));
    assert_eq!(organization["root"], Value::from("identity"));
    assert!(organization.get("active_key").is_none(), "{organization}");
}

#[test]
fn a_member_is_promoted_by_a_second_invitation_carrying_the_same_key() {
    let home = Home::new();
    let exchange = Exchange::new();
    let alice = home.create("alice");
    let bob = home.create("bob");
    let descriptor = exchange.file("bob.descriptor");
    home.export("bob", &descriptor);

    for (role, seq) in [("member", 1), ("controller", 3)] {
        let bundle = exchange.file(&format!("{role}.invitation"));
        let acceptance = exchange.file(&format!("{role}.acceptance"));
        let invited = home.json(&[
            "membership",
            "invite",
            "--ledger",
            "alice",
            "--by",
            "alice",
            "--invitee",
            &path(&descriptor),
            "--role",
            role,
            "--out",
            &path(&bundle),
        ]);
        assert_eq!(invited["invitation_seq"], Value::from(seq));
        home.json(&[
            "membership",
            "accept",
            &path(&bundle),
            "--as",
            "bob",
            "--out",
            &path(&acceptance),
            "--yes",
        ]);
        let admitted = home.json(&[
            "membership",
            "admit",
            "--ledger",
            "alice",
            "--by",
            "alice",
            &path(&acceptance),
        ]);
        assert_eq!(admitted["role"], Value::from(role));
        assert_eq!(admitted["acceptance_seq"], Value::from(seq + 1));
    }

    let listed = home.json(&["membership", "list", "--ledger", "alice"]);
    let promoted = listed["principals"]
        .as_array()
        .expect("an array")
        .iter()
        .find(|entry| entry["identity"] == *bob)
        .expect("bob is a principal");
    assert_eq!(promoted["role"], Value::from("controller"));
    assert_eq!(listed["invitations"].as_array().expect("an array").len(), 2);
    assert_ne!(bob, alice);
}

#[test]
fn replaying_an_acceptance_exits_50() {
    let flow = Flow::accepted("controller");
    flow.admit();

    let (code, document) = flow.inviter.failure(&[
        "membership",
        "admit",
        "--ledger",
        "alice",
        "--by",
        "alice",
        &path(&flow.acceptance),
    ]);
    assert_eq!(code, 50);
    assert_eq!(
        document["details"]["reason"],
        Value::from("acceptance_already_used")
    );
    assert_eq!(document["details"]["ledger_id"], Value::from(flow.ledger));
    assert_eq!(document["details"]["at_seq"], Value::from(2));
    assert_eq!(
        document["details"]["path"],
        Value::from(path(&flow.acceptance))
    );
    assert!(
        is_id(&document["details"]["invitation_event"]),
        "{document}"
    );
    assert!(
        text(&document["message"]).starts_with("Replay error: "),
        "{document}"
    );

    // Text mode carries the same prefix on stderr.
    let (code, _, stderr) = flow.inviter.run(&[
        "membership",
        "admit",
        "--ledger",
        "alice",
        "--by",
        "alice",
        &path(&flow.acceptance),
    ]);
    assert_eq!(code, 50);
    assert!(stderr.starts_with("Replay error: "), "{stderr}");
}

#[test]
fn an_acceptance_for_another_ledger_exits_20() {
    let flow = Flow::accepted("controller");
    flow.admit();
    flow.inviter.found("acme", "alice");

    // The blob binds the ledger it was signed for, so the org refuses it.
    let (code, document) = flow.inviter.failure(&[
        "membership",
        "admit",
        "--ledger",
        "acme",
        "--by",
        "alice",
        &path(&flow.acceptance),
    ]);
    assert_eq!(code, 20);
    assert_eq!(
        document["details"]["reason"],
        Value::from("acceptance_for_another_ledger")
    );
    assert!(
        text(&document["message"]).starts_with("Policy error: "),
        "{document}"
    );
}

#[test]
fn an_over_cap_artifact_exits_10() {
    let home = Home::new();
    let exchange = Exchange::new();
    home.create("alice");
    let descriptor = exchange.file("big.descriptor");
    let bundle = exchange.file("big.invitation");
    let acceptance = exchange.file("big.acceptance");
    std::fs::write(&descriptor, vec![0u8; 64 * 1024 + 1]).expect("writes");
    std::fs::write(&bundle, vec![0u8; 1024 * 1024 + 1]).expect("writes");
    std::fs::write(&acceptance, vec![0u8; 4096 + 1]).expect("writes");

    let cases = [
        (
            vec![
                "membership".to_owned(),
                "invite".to_owned(),
                "--ledger".to_owned(),
                "alice".to_owned(),
                "--by".to_owned(),
                "alice".to_owned(),
                "--invitee".to_owned(),
                path(&descriptor),
                "--role".to_owned(),
                "member".to_owned(),
                "--out".to_owned(),
                path(&exchange.file("unused")),
            ],
            "IdentityDescriptor",
            65_536,
        ),
        (
            vec![
                "membership".to_owned(),
                "accept".to_owned(),
                path(&bundle),
                "--as".to_owned(),
                "alice".to_owned(),
                "--out".to_owned(),
                path(&exchange.file("unused")),
                "--yes".to_owned(),
            ],
            "InvitationBundle",
            1_048_576,
        ),
        (
            vec![
                "membership".to_owned(),
                "admit".to_owned(),
                "--ledger".to_owned(),
                "alice".to_owned(),
                "--by".to_owned(),
                "alice".to_owned(),
                path(&acceptance),
            ],
            "AcceptanceFile",
            4096,
        ),
    ];
    for (arguments, artifact, cap) in cases {
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        let (code, document) = home.failure(&arguments);
        assert_eq!(code, 10, "{artifact}: {document}");
        assert_eq!(
            document["details"]["reason"],
            Value::from("message_too_large"),
            "{document}"
        );
        assert_eq!(document["details"]["artifact"], Value::from(artifact));
        assert_eq!(document["details"]["cap"], Value::from(cap));
        assert_eq!(document["details"]["bytes"], Value::from(cap + 1));
        assert!(
            text(&document["message"]).starts_with("Schema error: "),
            "{document}"
        );
    }
}

#[test]
fn a_malformed_artifact_exits_10_and_a_bundle_that_does_not_fold_exits_20() {
    let flow = Flow::accepted("controller");

    // Bytes that are not the artifact at all.
    let garbage = flow.exchange.file("garbage.invitation");
    std::fs::write(&garbage, vec![0xff; 64]).expect("writes");
    let (code, document) = flow.invitee.failure(&[
        "membership",
        "accept",
        &path(&garbage),
        "--as",
        "bob",
        "--out",
        &path(&flow.exchange.file("unused")),
        "--yes",
    ]);
    assert_eq!(code, 10);
    assert_eq!(
        document["details"]["artifact"],
        Value::from("InvitationBundle")
    );
    assert!(
        text(&document["message"]).starts_with("Schema error: "),
        "{document}"
    );

    // A well-formed bundle whose last event no longer verifies: the file is an
    // artifact, the ledger inside it is not.
    let tampered = flow.exchange.file("tampered.invitation");
    let mut bytes = std::fs::read(&flow.bundle).expect("the bundle");
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    std::fs::write(&tampered, &bytes).expect("writes");
    let (code, document) = flow.invitee.failure(&[
        "membership",
        "accept",
        &path(&tampered),
        "--as",
        "bob",
        "--out",
        &path(&flow.exchange.file("unused")),
        "--yes",
    ]);
    assert_eq!(code, 20);
    assert_eq!(document["details"]["reason"], Value::from("bad_signature"));
    assert_eq!(document["details"]["failed_at_seq"], Value::from(1));
    assert!(
        text(&document["message"]).starts_with("Ledger error: "),
        "{document}"
    );
}

#[test]
fn the_accept_surface_warns_before_signing_a_controller_role_on_a_raw_root() {
    let inviter = Home::new();
    let invitee = Home::new();
    let exchange = Exchange::new();
    let ledger = inviter.create("alice");
    let invited = invitee.create("bob");
    let descriptor = exchange.file("bob.descriptor");
    invitee.export("bob", &descriptor);
    let bundle = exchange.file("alice.invitation");
    inviter.json(&[
        "membership",
        "invite",
        "--ledger",
        "alice",
        "--by",
        "alice",
        "--invitee",
        &path(&descriptor),
        "--role",
        "controller",
        "--out",
        &path(&bundle),
    ]);

    let out = exchange.file("bob.acceptance");
    let document = invitee.json(&[
        "membership",
        "accept",
        &path(&bundle),
        "--as",
        "bob",
        "--out",
        &path(&out),
        "--yes",
    ]);
    assert_keys(
        &document,
        &[
            "ok",
            "ledger_id",
            "declared_kind",
            "root",
            "controllers",
            "invitation_event",
            "invitee",
            "invitee_key",
            "role",
            "controller_on_raw_root",
            "warning",
            "path",
            "bytes",
        ],
    );
    assert_eq!(document["ledger_id"], Value::from(ledger.clone()));
    assert_eq!(document["declared_kind"], Value::from("person"));
    assert_eq!(document["root"], Value::from("raw"));
    assert_eq!(document["role"], Value::from("controller"));
    assert_eq!(document["invitee"], Value::from(invited));
    assert_eq!(document["controller_on_raw_root"], Value::Bool(true));
    assert!(
        text(&document["warning"]).contains(&format!("means signing as {ledger}")),
        "{document}"
    );
    let controllers = document["controllers"].as_array().expect("an array");
    assert_eq!(controllers.len(), 1, "{document}");
    assert_eq!(controllers[0]["identity"], Value::from(ledger.clone()));
    assert_eq!(controllers[0]["is_root"], Value::Bool(true));

    // Text mode prints the surface, and the warning, before it signs.
    let second = exchange.file("bob-again.acceptance");
    let (code, stdout, _) = invitee.run(&[
        "membership",
        "accept",
        &path(&bundle),
        "--as",
        "bob",
        "--out",
        &path(&second),
        "--yes",
    ]);
    assert_eq!(code, 0);
    let warning = stdout.find("warning: ").expect("the warning is printed");
    let signed = stdout
        .find("signed acceptance")
        .expect("the file is signed");
    assert!(warning < signed, "{stdout}");
    assert!(
        stdout.contains(&format!("invitation to {ledger}")),
        "{stdout}"
    );
    assert!(stdout.contains("role offered controller"), "{stdout}");
}

#[test]
fn the_whole_flow_runs_on_an_identity_rooted_ledger_without_the_warning() {
    let inviter = Home::new();
    let invitee = Home::new();
    let exchange = Exchange::new();
    let alice = inviter.create("alice");
    let acme = inviter.found("acme", "alice");
    let bob = invitee.create("bob");
    let descriptor = exchange.file("bob.descriptor");
    invitee.export("bob", &descriptor);
    let bundle = exchange.file("acme.invitation");
    inviter.json(&[
        "membership",
        "invite",
        "--ledger",
        "acme",
        "--by",
        "alice",
        "--invitee",
        &path(&descriptor),
        "--role",
        "controller",
        "--out",
        &path(&bundle),
    ]);

    // The founder is an ordinary controller here, so a controller offer means
    // signing for the org and not as anyone's own identity.
    let acceptance = exchange.file("bob.acceptance");
    let document = invitee.json(&[
        "membership",
        "accept",
        &path(&bundle),
        "--as",
        "bob",
        "--out",
        &path(&acceptance),
        "--yes",
    ]);
    assert_eq!(document["ledger_id"], Value::from(acme.clone()));
    assert_eq!(document["root"], Value::from("identity"));
    assert_eq!(document["declared_kind"], Value::from("organization"));
    assert_eq!(document["controller_on_raw_root"], Value::Bool(false));
    assert_eq!(document["warning"], Value::Null);
    assert_eq!(document["controllers"][0]["identity"], Value::from(alice));

    let admitted = inviter.json(&[
        "membership",
        "admit",
        "--ledger",
        "acme",
        "--by",
        "alice",
        &path(&acceptance),
    ]);
    assert_eq!(admitted["ledger_id"], Value::from(acme.clone()));
    assert_eq!(admitted["invitee"], Value::from(bob.clone()));
    assert_eq!(admitted["acceptance_seq"], Value::from(2));

    // With two controllers, the founder is removable.
    let removed = inviter.json(&[
        "membership",
        "remove",
        "--ledger",
        "acme",
        "--by",
        "alice",
        "--member",
        "alice",
    ]);
    assert_eq!(removed["principal_removed"], Value::Bool(true));
    let listed = inviter.json(&["membership", "list", "--ledger", "acme"]);
    let principals = listed["principals"].as_array().expect("an array");
    assert_eq!(principals.len(), 1, "{listed}");
    assert_eq!(principals[0]["identity"], Value::from(bob));
    assert_eq!(principals[0]["is_root"], Value::Bool(false));
}

#[test]
fn accept_needs_a_confirmation_and_signs_nothing_without_one() {
    let flow = Flow::accepted("member");
    let out = flow.exchange.file("declined.acceptance");

    // A refused prompt exits 2 and writes no file.
    let (code, stdout, stderr) = flow.invitee.answer(
        &[
            "membership",
            "accept",
            &path(&flow.bundle),
            "--as",
            "bob",
            "--out",
            &path(&out),
        ],
        "no\n",
    );
    assert_eq!(code, 2, "{stdout}{stderr}");
    assert!(stdout.contains("type yes to sign"), "{stdout}");
    assert_eq!(stderr.trim(), "not confirmed; nothing was signed");
    assert!(!out.exists(), "nothing is written when nobody confirmed");

    // A typed yes signs it.
    let (code, stdout, stderr) = flow.invitee.answer(
        &[
            "membership",
            "accept",
            &path(&flow.bundle),
            "--as",
            "bob",
            "--out",
            &path(&out),
        ],
        "yes\n",
    );
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(out.exists(), "{stdout}");

    // `--json` cannot prompt, so it demands --yes rather than half-asking.
    let (code, document) = flow.invitee.failure(&[
        "membership",
        "accept",
        &path(&flow.bundle),
        "--as",
        "bob",
        "--out",
        &path(&flow.exchange.file("unused")),
    ]);
    assert_eq!(code, 2);
    assert_eq!(
        document["details"]["reason"],
        Value::from("confirmation_required")
    );
}

#[test]
fn accepting_an_invitation_addressed_to_someone_else_exits_2() {
    let flow = Flow::accepted("member");
    let carol = flow.invitee.create("carol");

    let (code, document) = flow.invitee.failure(&[
        "membership",
        "accept",
        &path(&flow.bundle),
        "--as",
        "carol",
        "--out",
        &path(&flow.exchange.file("unused")),
        "--yes",
    ]);
    assert_eq!(code, 2);
    assert_eq!(
        document["details"]["reason"],
        Value::from("not_the_invitee")
    );
    assert_eq!(document["details"]["invitee"], Value::from(flow.invited));
    assert_ne!(document["details"]["invitee"], Value::from(carol));
}

#[test]
fn removing_the_raw_root_or_the_last_controller_exits_20() {
    let flow = Flow::accepted("controller");
    flow.admit();
    let acme = flow.inviter.found("acme", "alice");

    // The raw root is permanent (proposal 002 section 2).
    let (code, document) = flow.inviter.failure(&[
        "membership",
        "remove",
        "--ledger",
        "alice",
        "--by",
        "alice",
        "--member",
        &flow.ledger,
    ]);
    assert_eq!(code, 20);
    assert_eq!(
        document["details"]["reason"],
        Value::from("root_not_removable")
    );

    // The org's founder is its only controller, so removing them leaves
    // nobody who can append.
    let (code, document) = flow.inviter.failure(&[
        "membership",
        "remove",
        "--ledger",
        "acme",
        "--by",
        "alice",
        "--member",
        "alice",
    ]);
    assert_eq!(code, 20);
    assert_eq!(
        document["details"]["reason"],
        Value::from("last_controller")
    );
    assert_eq!(document["details"]["ledger_id"], Value::from(acme));
    assert!(
        text(&document["message"]).starts_with("Policy error: "),
        "{document}"
    );
}

#[test]
fn remove_takes_the_principal_and_cancels_the_open_invitation() {
    let flow = Flow::accepted("controller");
    flow.admit();

    let removed = flow.inviter.json(&[
        "membership",
        "remove",
        "--ledger",
        "alice",
        "--by",
        "alice",
        "--member",
        &flow.invited,
    ]);
    assert_keys(
        &removed,
        &[
            "ok",
            "ledger_id",
            "by",
            "target",
            "principal_removed",
            "invitation_cancelled",
            "removal_event",
            "removal_seq",
            "timestamp_ms",
            "head_seq",
            "head_event",
        ],
    );
    assert_eq!(removed["target"], Value::from(flow.invited.clone()));
    assert_eq!(removed["principal_removed"], Value::Bool(true));
    assert_eq!(removed["invitation_cancelled"], Value::Null);
    assert_eq!(removed["removal_seq"], Value::from(3));

    let listed = flow
        .inviter
        .json(&["membership", "list", "--ledger", "alice"]);
    assert_eq!(listed["principals"].as_array().expect("an array").len(), 1);

    // An open invitation is cancelled by the same command, with no acceptance
    // in between.
    let descriptor = flow.exchange.file("bob.descriptor");
    flow.inviter.json(&[
        "membership",
        "invite",
        "--ledger",
        "alice",
        "--by",
        "alice",
        "--invitee",
        &path(&descriptor),
        "--role",
        "member",
        "--out",
        &path(&flow.exchange.file("second.invitation")),
    ]);
    let cancelled = flow.inviter.json(&[
        "membership",
        "remove",
        "--ledger",
        "alice",
        "--by",
        "alice",
        "--target",
        &flow.invited,
    ]);
    assert_eq!(cancelled["principal_removed"], Value::Bool(false));
    assert!(is_id(&cancelled["invitation_cancelled"]), "{cancelled}");
}

#[test]
fn an_identity_rooted_ledger_cannot_be_invited() {
    let home = Home::new();
    let exchange = Exchange::new();
    home.create("alice");
    let acme = home.found("acme", "alice");
    let bob = home.create("bob");
    let descriptor = exchange.file("acme.descriptor");
    home.export("acme", &descriptor);

    let (code, document) = home.failure(&[
        "membership",
        "invite",
        "--ledger",
        "bob",
        "--by",
        "bob",
        "--invitee",
        &path(&descriptor),
        "--role",
        "member",
        "--out",
        &path(&exchange.file("unused")),
    ]);
    assert_eq!(code, 20);
    assert_eq!(
        document["details"]["reason"],
        Value::from("invitee_holds_no_key")
    );
    assert_eq!(document["details"]["invitee"], Value::from(acme));
    assert_ne!(document["details"]["invitee"], Value::from(bob));
}

#[test]
fn a_missing_artifact_file_exits_2() {
    let home = Home::new();
    home.create("alice");
    let (code, document) = home.failure(&[
        "membership",
        "admit",
        "--ledger",
        "alice",
        "--by",
        "alice",
        "/no/such/acceptance",
    ]);
    assert_eq!(code, 2);
    assert_eq!(document["details"]["reason"], Value::from("no_such_file"));
    assert_eq!(
        document["details"]["path"],
        Value::from("/no/such/acceptance")
    );
}

#[test]
fn org_and_member_run_the_membership_commands_and_stay_out_of_help() {
    let home = Home::new();
    home.create("alice");
    let listed = home.json(&["membership", "list", "--ledger", "alice"]);
    for alias in ["org", "member"] {
        assert_eq!(
            home.json(&[alias, "list", "--ledger", "alice"]),
            listed,
            "{alias} names the same command as membership"
        );
    }

    let (code, stdout, _) = home.run(&["--help"]);
    assert_eq!(code, 0);
    let listed: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("  "))
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    assert!(listed.contains(&"membership"), "{stdout}");
    for alias in ["org", "member"] {
        assert!(
            !listed.contains(&alias),
            "{alias} is undocumented: {stdout}"
        );
    }
}
