//! The CLI against the frozen fixtures in `contracts/cli/`.
//!
//! Every `--json` document is compared to its fixture key for key: the ids and
//! timestamps a temp home produces are its own, so the assertion is on the
//! shape (which keys exist, and what kind of value each holds), not on the
//! values. The exit codes this ticket owns are 0, 2, 20, 60 and 70.

use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

/// An id no home holds, for the unresolved-subject and missing-ledger cases.
const STRANGER: &str = "jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq";

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
        let mut command = binary();
        command.arg("--home").arg(self.path()).args(arguments);
        command
    }

    /// Runs a command, returning its exit code, stdout and stderr.
    fn run(&self, arguments: &[&str]) -> (i32, String, String) {
        output(&mut self.command(arguments))
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

    /// Creates an identity and returns its id.
    fn create(&self, alias: &str) -> String {
        text(&self.json(&["identity", "create", "--alias", alias])["identity_id"])
    }

    /// This node's endpoint id, the one real endpoint a temp home has.
    fn endpoint(&self) -> String {
        text(&self.json(&["node", "id"])["endpoint_id"])
    }
}

fn binary() -> Command {
    Command::cargo_bin("mabel").expect("the mabel binary is built")
}

/// Runs a command to completion.
fn output(command: &mut Command) -> (i32, String, String) {
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

/// One `document` from a `contracts/cli/` fixture, by case name.
fn fixture(file: &str, case: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/cli")
        .join(format!("{file}.json"));
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let fixture: Value = serde_json::from_slice(&bytes).expect("the fixture is JSON");
    let cases = fixture["cases"].as_array().expect("cases is an array");
    let found = cases
        .iter()
        .find(|entry| entry["case"] == *case)
        .unwrap_or_else(|| panic!("{file}.json has no case {case}"));
    found["document"].clone()
}

/// Asserts that `actual` has the keys of `expected`, and the same kind of
/// value under each.
///
/// A `null` on either side matches anything, which is how the fixtures spell a
/// field that does not apply. An array in `actual` that is empty is accepted,
/// since an empty array carries no shape.
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

/// Asserts that two objects carry the same top-level keys.
fn assert_keys(actual: &Value, expected: &Value, path: &str) {
    let keys = |value: &Value| {
        let mut keys: Vec<String> = value
            .as_object()
            .unwrap_or_else(|| panic!("{path} is not an object: {value}"))
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    };
    assert_eq!(keys(actual), keys(expected), "keys differ at {path}");
}

fn is_id(value: &Value) -> bool {
    value.as_str().is_some_and(|id| {
        id.len() == 52
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c))
    })
}

#[test]
fn identity_create_matches_the_fixture() {
    let home = Home::new();
    let document = home.json(&["identity", "create", "--alias", "alice"]);
    assert_shape(
        &document,
        &fixture("identity-create", "created"),
        "identity-create",
    );
    assert_eq!(document["alias"], Value::from("alice"));
    assert_eq!(document["declared_kind"], Value::from("person"));
    assert_eq!(document["head_seq"], Value::from(0));
    assert_eq!(document["identity_id"], document["inception_event"]);
    assert_eq!(document["identity_id"], document["head_event"]);
    assert!(is_id(&document["active_key"]), "{document}");
    assert!(is_id(&document["reserve_commit"]), "{document}");
    assert_ne!(document["active_key"], document["reserve_commit"]);
}

/// A name or an email on the create adds one `ProfileUpdate` at seq 1, so a
/// new identity's first two events are who it is and what it shows the world
/// (proposal 005).
#[test]
fn identity_create_with_a_name_and_an_email_publishes_a_profile_at_seq_1() {
    let home = Home::new();
    let document = home.json(&[
        "identity",
        "create",
        "--alias",
        "alice",
        "--name",
        "Alice Ashworth",
        "--email",
        "alice@alice.example",
    ]);
    assert_shape(
        &document,
        &fixture("identity-create", "created-with-a-profile"),
        "identity-create/created-with-a-profile",
    );
    assert_eq!(document["head_seq"], Value::from(1));
    assert_ne!(document["head_event"], document["inception_event"]);
    let profile = &document["profile"];
    assert_eq!(profile["display_name"], Value::from("Alice Ashworth"));
    assert_eq!(profile["email"], Value::from("alice@alice.example"));
    assert_eq!(
        profile["hostname"],
        Value::Null,
        "create claims no hostname"
    );
    assert_eq!(profile["seq"], Value::from(1));
    assert_eq!(profile["event"], document["head_event"]);
    assert_eq!(
        profile["signing_principal"]["identity"],
        document["identity_id"]
    );

    // The same document the identity routes serve reports it.
    let shown = home.json(&["identity", "show", "alice"]);
    assert_eq!(shown["profile"], *profile);
    assert_eq!(shown["event_count"], Value::from(2));
}

/// The scanner's refusal lands before the mint, so a mistyped email costs
/// neither a ledger nor the alias.
#[test]
fn identity_create_with_an_email_the_scanner_refuses_creates_nothing() {
    let home = Home::new();
    let expected = fixture("identity-create", "invalid-email");
    let (code, document) = home.failure(&[
        "identity",
        "create",
        "--alias",
        "alice",
        "--email",
        "alice.example",
    ]);
    assert_eq!(code, 10);
    assert_eq!(document["message"], expected["message"]);
    assert_eq!(document["details"], expected["details"]);

    // No identity, and the alias is still free.
    let listed = home.json(&["identity", "list"]);
    assert_eq!(listed["identities"], Value::Array(Vec::new()));
    home.json(&["identity", "create", "--alias", "alice"]);
}

/// Neither flag given, the new ledger is one event long and publishes nothing.
#[test]
fn identity_create_without_a_name_or_an_email_publishes_no_profile() {
    let home = Home::new();
    let document = home.json(&["identity", "create", "--alias", "alice"]);
    assert_eq!(document["profile"], Value::Null);
    assert_eq!(document["head_seq"], Value::from(0));
}

#[test]
fn identity_create_with_a_founder_makes_an_identity_root() {
    let home = Home::new();
    let founder = home.create("alice");
    let document = home.json(&[
        "identity",
        "create",
        "--alias",
        "acme",
        "--kind",
        "organization",
        "--founder",
        "alice",
    ]);
    let mut expected = fixture("identity-create", "created");
    let fields = expected.as_object_mut().expect("an object");
    // An identity-rooted ledger holds no key of its own (ticket 008 scope).
    fields.remove("active_key");
    fields.remove("reserve_commit");
    assert_shape(&document, &expected, "identity-create/identity-root");
    assert_eq!(document["declared_kind"], Value::from("organization"));
    assert_ne!(text(&document["identity_id"]), founder);

    // The founder's key signed it, so the founder is the ledger's controller
    // and can append to it.
    let subject = home.create("bob");
    let attested = home.json(&["trust", "add", "--issuer", "acme", "--subject", &subject]);
    assert_eq!(attested["issuer"], document["identity_id"]);
}

#[test]
fn identity_create_refuses_an_alias_this_home_already_uses() {
    let home = Home::new();
    home.create("alice");
    let (code, document) = home.failure(&["identity", "create", "--alias", "alice"]);
    assert_eq!(code, 2);
    assert_eq!(document["details"]["reason"], Value::from("alias_in_use"));
}

#[test]
fn identity_list_on_an_empty_home_matches_the_fixture() {
    let home = Home::new();
    let document = home.json(&["identity", "list"]);
    assert_shape(
        &document,
        &fixture("identity-list", "empty-home"),
        "identity-list/empty",
    );
    assert_eq!(document["identities"], Value::Array(Vec::new()));
}

#[test]
fn identity_list_matches_the_fixture_for_a_person_and_an_organization() {
    let home = Home::new();
    let alice = home.create("alice");
    let bob = home.create("bob");
    home.json(&[
        "identity",
        "create",
        "--alias",
        "acme",
        "--kind",
        "organization",
        "--founder",
        "alice",
    ]);
    let attestation = text(
        &home.json(&["trust", "add", "--issuer", "alice", "--subject", &bob])["attestation_event"],
    );
    home.json(&[
        "trust",
        "revoke",
        "--issuer",
        "alice",
        "--attestation",
        &attestation,
    ]);
    let endpoint = home.endpoint();
    home.json(&[
        "witness",
        "add",
        "--identity",
        "alice",
        "--endpoint",
        &endpoint,
    ]);

    let document = home.json(&["identity", "list"]);
    let expected = fixture("identity-list", "one-person-and-one-organization");
    // The array holds two shapes, a person and an organization, so each entry
    // is compared to the fixture entry of its kind below.
    assert_keys(&document, &expected, "identity-list");

    let entries = document["identities"].as_array().expect("an array");
    assert_eq!(entries.len(), 3, "{document}");
    let person = entries
        .iter()
        .find(|entry| entry["identity_id"] == *alice)
        .expect("alice is listed");
    let organization = entries
        .iter()
        .find(|entry| entry["declared_kind"] == *"organization")
        .expect("acme is listed");
    let expected_entries = expected["identities"].as_array().expect("an array");
    let expected_person = expected_entries
        .iter()
        .find(|entry| entry["declared_kind"] == *"person")
        .expect("the fixture lists a person");
    let expected_organization = expected_entries
        .iter()
        .find(|entry| entry["declared_kind"] == *"organization")
        .expect("the fixture lists an organization");

    assert_shape(person, expected_person, "identity-list/person");
    assert_shape(
        organization,
        expected_organization,
        "identity-list/organization",
    );
    assert_eq!(person["alias"], Value::from("alice"));
    assert_eq!(person["trust"][0]["revoked"], Value::Bool(true));
    assert_eq!(person["witnesses"][0], Value::from(endpoint));
    assert!(
        organization.get("active_key").is_none(),
        "an identity-rooted ledger reports no key of its own: {organization}"
    );
}

#[test]
fn identity_show_returns_the_identity_document() {
    let home = Home::new();
    let alice = home.create("alice");
    let document = home.json(&["identity", "show", "alice"]);
    let expected =
        fixture("identity-list", "one-person-and-one-organization")["identities"][1].clone();
    let mut expected_with_ok = expected.clone();
    expected_with_ok
        .as_object_mut()
        .expect("an object")
        .insert("ok".to_owned(), Value::Bool(true));
    assert_shape(&document, &expected_with_ok, "identity-show");
    assert_eq!(document["identity_id"], Value::from(alice.clone()));

    // The id resolves to the same document as the alias.
    assert_eq!(home.json(&["identity", "show", &alice]), document);
}

#[test]
fn identity_rotate_exits_70() {
    let home = Home::new();
    home.create("alice");
    let (code, document) = home.failure(&["identity", "rotate", "alice"]);
    assert_eq!(code, 70);
    assert_eq!(
        document["message"],
        Value::from("key rotation is not part of this POC")
    );
    assert_eq!(
        document["details"]["reason"],
        Value::from("unsupported_feature")
    );
    let (_, _, stderr) = home.run(&["identity", "rotate", "alice"]);
    assert_eq!(stderr.trim(), "key rotation is not part of this POC");
}

#[test]
fn trust_add_matches_the_fixture() {
    let home = Home::new();
    let alice = home.create("alice");
    let bob = home.create("bob");
    let document = home.json(&["trust", "add", "--issuer", "alice", "--subject", &bob]);
    assert_shape(&document, &fixture("trust-add", "attested"), "trust-add");
    assert_eq!(document["issuer"], Value::from(alice));
    assert_eq!(document["subject"], Value::from(bob));
    assert_eq!(document["attestation_seq"], Value::from(1));
    assert_eq!(document["head_seq"], document["attestation_seq"]);
    assert_eq!(document["head_event"], document["attestation_event"]);
    assert_eq!(document["pushed"], Value::Bool(false));
}

#[test]
fn a_second_attestation_for_the_same_subject_exits_20() {
    let home = Home::new();
    home.create("alice");
    let bob = home.create("bob");
    home.json(&["trust", "add", "--issuer", "alice", "--subject", &bob]);

    let (code, document) = home.failure(&["trust", "add", "--issuer", "alice", "--subject", &bob]);
    assert_eq!(code, 20);
    assert_shape(&document, &fixture("errors", "policy"), "errors/policy");
    assert_eq!(
        document["details"]["reason"],
        Value::from("duplicate_unrevoked_attestation")
    );
    assert!(
        text(&document["message"]).starts_with("Policy error: "),
        "{document}"
    );
    let (_, _, stderr) = home.run(&["trust", "add", "--issuer", "alice", "--subject", &bob]);
    assert!(stderr.starts_with("Policy error: "), "{stderr}");
}

#[test]
fn trust_revoke_records_the_revocation_and_trust_list_reports_it() {
    let home = Home::new();
    home.create("alice");
    let bob = home.create("bob");
    let attestation = text(
        &home.json(&["trust", "add", "--issuer", "alice", "--subject", &bob])["attestation_event"],
    );
    let revoked = home.json(&[
        "trust",
        "revoke",
        "--issuer",
        "alice",
        "--attestation",
        &attestation,
    ]);
    assert_eq!(
        revoked["attestation_event"],
        Value::from(attestation.clone())
    );
    assert_eq!(revoked["attestation_seq"], Value::from(1));
    assert_eq!(revoked["revocation_seq"], Value::from(2));
    assert_eq!(revoked["subject"], Value::from(bob));

    let listed = home.json(&["trust", "list", "--issuer", "alice"]);
    let entry = &listed["trust"][0];
    assert_eq!(entry["attestation_event"], Value::from(attestation.clone()));
    assert_eq!(entry["revoked"], Value::Bool(true));
    assert_eq!(entry["revocation_event"], revoked["revocation_event"]);
    assert_shape(
        entry,
        &fixture("identity-list", "one-person-and-one-organization")["identities"][1]["trust"][0],
        "trust-list/entry",
    );

    // Flag R: the text says how far it read, and never "unrevoked".
    let (code, stdout, _) = home.run(&["trust", "list", "--issuer", "alice"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("revoked at seq 2"), "{stdout}");
    assert!(!stdout.contains("unrevoked"), "{stdout}");
}

#[test]
fn witness_add_replaces_the_set_the_ledger_records() {
    let home = Home::new();
    let alice = home.create("alice");
    let endpoint = home.endpoint();
    let document = home.json(&[
        "witness",
        "add",
        "--identity",
        "alice",
        "--endpoint",
        &endpoint,
    ]);
    assert_eq!(document["identity_id"], Value::from(alice));
    assert_eq!(document["endpoint"], Value::from(endpoint.clone()));
    assert_eq!(document["witnesses"], Value::from(vec![endpoint.clone()]));
    assert_eq!(document["head_seq"], Value::from(1));
    assert_eq!(document["head_event"], document["event_id"]);

    let shown = home.json(&["identity", "show", "alice"]);
    assert_eq!(shown["witnesses"], Value::from(vec![endpoint]));
}

#[test]
fn verify_ledger_matches_the_fixture() {
    let home = Home::new();
    let alice = home.create("alice");
    let bob = home.create("bob");
    home.json(&["trust", "add", "--issuer", "alice", "--subject", &bob]);

    let document = home.json(&["verify", "ledger", &alice]);
    assert_shape(
        &document,
        &fixture("verify-ledger", "valid"),
        "verify-ledger",
    );
    assert_eq!(document["kind"], Value::from("ledger"));
    assert_eq!(document["valid"], Value::Bool(true));
    assert_eq!(document["valid_to_seq"], Value::from(1));
    assert_eq!(document["failed_at_seq"], Value::Null);
    assert_eq!(document["event_count"], Value::from(2));
    assert_eq!(document["source"], document["sources_queried"][0]);
    assert!(
        text(&document["statement"])
            .starts_with(&format!("valid as of seq 1 of {alice}, fetched from ")),
        "{document}"
    );
    assert!(
        text(&document["verified_means"]).starts_with("Verified means this identity signed"),
        "{document}"
    );

    let (_, stdout, _) = home.run(&["verify", "ledger", &alice]);
    assert!(stdout.contains("valid as of seq 1 of"), "{stdout}");
}

#[test]
fn verify_ledger_on_a_tampered_ledger_exits_20() {
    let home = Home::new();
    let alice = home.create("alice");
    let bob = home.create("bob");
    home.json(&["trust", "add", "--issuer", "alice", "--subject", &bob]);
    tamper(
        &home
            .path()
            .join("ledgers")
            .join(&alice)
            .join("000000000001.ev"),
    );

    let (code, document) = home.failure(&["verify", "ledger", &alice]);
    assert_eq!(code, 20);
    let expected = fixture("verify-ledger", "partial-validity");
    assert_shape(&document, &expected, "verify-ledger/partial");
    assert_eq!(document["details"]["valid"], Value::Bool(false));
    assert_eq!(document["details"]["valid_to_seq"], Value::from(0));
    assert_eq!(document["details"]["failed_at_seq"], Value::from(1));
    assert_eq!(
        text(&document["message"]),
        format!(
            "Ledger error: valid to seq 0, failed at seq 1: {}",
            "SignedEvent.signature does not verify under author_key"
        )
    );

    let (_, _, stderr) = home.run(&["verify", "ledger", &alice]);
    assert!(stderr.starts_with("Ledger error: "), "{stderr}");
}

#[test]
fn verify_ledger_for_a_ledger_no_source_holds_exits_30() {
    let home = Home::new();
    home.create("alice");
    let (code, document) = home.failure(&["verify", "ledger", STRANGER]);
    assert_eq!(code, 30);
    assert_shape(&document, &fixture("errors", "network"), "errors/network");
    assert_eq!(
        document["details"]["reason"],
        Value::from("no_source_available")
    );
    assert!(
        text(&document["message"]).starts_with("Network error: "),
        "{document}"
    );
}

#[test]
fn verify_trust_matches_the_trusted_fixture() {
    let home = Home::new();
    let alice = home.create("alice");
    let bob = home.create("bob");
    home.json(&["trust", "add", "--issuer", "alice", "--subject", &bob]);

    let document = home.json(&["verify", "trust", "--issuer", &alice, "--subject", &bob]);
    assert_shape(
        &document,
        &fixture("verify-trust", "trusted"),
        "verify-trust/trusted",
    );
    assert_eq!(document["trusted"], Value::Bool(true));
    assert_eq!(document["subject_resolution"], Value::from("resolved"));
    assert_eq!(document["subject_note"], Value::Null);
    assert_eq!(document["revoked_count"], Value::from(0));
    assert_eq!(document["attestation_seq"], Value::from(1));
    assert!(
        text(&document["statement"]).ends_with("; no revocation up to seq 1"),
        "{document}"
    );
}

#[test]
fn verify_trust_matches_the_revoked_fixture_and_still_exits_0() {
    let home = Home::new();
    let alice = home.create("alice");
    let bob = home.create("bob");
    let attestation = text(
        &home.json(&["trust", "add", "--issuer", "alice", "--subject", &bob])["attestation_event"],
    );
    home.json(&[
        "trust",
        "revoke",
        "--issuer",
        "alice",
        "--attestation",
        &attestation,
    ]);

    let document = home.json(&["verify", "trust", "--issuer", &alice, "--subject", &bob]);
    assert_shape(
        &document,
        &fixture("verify-trust", "not-trusted-because-revoked"),
        "verify-trust/revoked",
    );
    assert_eq!(document["trusted"], Value::Bool(false));
    assert_eq!(document["attestation_event"], Value::Null);
    assert_eq!(document["attestation_seq"], Value::Null);
    assert_eq!(document["revoked_count"], Value::from(1));
    assert_eq!(
        document["revoked_attestations"][0]["attestation_event"],
        Value::from(attestation.clone())
    );
    assert!(
        text(&document["statement"])
            .ends_with(&format!("; attestation {attestation} revoked at seq 2")),
        "{document}"
    );
}

#[test]
fn verify_trust_with_a_subject_no_source_holds_matches_the_unresolved_fixture() {
    let home = Home::new();
    let alice = home.create("alice");
    home.json(&["trust", "add", "--issuer", "alice", "--subject", STRANGER]);

    let document = home.json(&["verify", "trust", "--issuer", &alice, "--subject", STRANGER]);
    assert_shape(
        &document,
        &fixture("verify-trust", "unresolved-subject"),
        "verify-trust/unresolved",
    );
    assert_eq!(document["trusted"], Value::Bool(true));
    assert_eq!(document["subject_resolution"], Value::from("unresolved"));
    assert_eq!(
        document["subject_note"],
        Value::from("subject: unresolved (not held by any queried source)")
    );
}

#[test]
fn verify_trust_text_carries_the_flag_l_sentence_and_never_says_unrevoked() {
    let home = Home::new();
    let alice = home.create("alice");
    let bob = home.create("bob");
    home.json(&["trust", "add", "--issuer", "alice", "--subject", &bob]);

    let (code, stdout, _) = home.run(&["verify", "trust", "--issuer", &alice, "--subject", &bob]);
    assert_eq!(code, 0);
    assert!(stdout.contains("trusted: true"), "{stdout}");
    assert!(
        stdout.contains(
            "subject control was not proven to this verifier; \
             the issuer is responsible for out-of-band confirmation"
        ),
        "{stdout}"
    );
    assert!(stdout.contains("no revocation up to seq 1"), "{stdout}");
    assert!(!stdout.contains("unrevoked"), "{stdout}");
}

#[test]
fn node_id_prints_this_nodes_endpoint_id() {
    let home = Home::new();
    let document = home.json(&["node", "id"]);
    assert!(is_id(&document["endpoint_id"]), "{document}");
    let (code, stdout, _) = home.run(&["node", "id"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), text(&document["endpoint_id"]));
}

#[test]
fn node_ticket_round_trips_through_parse_peer_ticket() {
    let home = Home::new();
    let document = home.json(&["node", "ticket", "--addr", "10.0.0.5:9070"]);
    assert_shape(
        &document,
        &fixture("node-ticket", "one-address"),
        "node-ticket",
    );
    assert!(is_id(&document["endpoint_id"]), "{document}");
    assert_eq!(document["addrs"], serde_json::json!(["10.0.0.5:9070"]));

    let ticket = text(&document["ticket"]);
    let parsed = mabel_net::parse_peer_ticket(&ticket).expect("the printed ticket parses");
    assert_eq!(base32(parsed.id.as_bytes()), home.endpoint());
    assert_eq!(
        parsed.addrs.into_iter().collect::<Vec<_>>(),
        vec![iroh_base::TransportAddr::Ip(
            "10.0.0.5:9070".parse().expect("a socket address")
        )]
    );

    // Text mode prints the ticket and nothing else, so `--peer "$(mabel node
    // ticket ...)"` works, which is what docker/entrypoint.sh does.
    let (code, stdout, _) = home.run(&["node", "ticket", "--addr", "10.0.0.5:9070"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), ticket);
}

#[test]
fn node_ticket_carries_every_address_it_is_given_and_none_by_default() {
    let home = Home::new();
    let document = home.json(&[
        "node",
        "ticket",
        "--addr",
        "10.0.0.5:9070",
        "--addr",
        "10.0.0.6:9071",
    ]);
    let parsed = mabel_net::parse_peer_ticket(&text(&document["ticket"])).expect("it parses");
    assert_eq!(parsed.addrs.len(), 2, "{document}");

    let document = home.json(&["node", "ticket"]);
    assert_eq!(document["addrs"], serde_json::json!([]));
    let parsed = mabel_net::parse_peer_ticket(&text(&document["ticket"])).expect("it parses");
    assert!(parsed.addrs.is_empty(), "{document}");
}

#[test]
fn a_malformed_ticket_address_exits_2() {
    let home = Home::new();
    let (code, document) = home.failure(&["node", "ticket", "--addr", "10.0.0.5"]);
    assert_eq!(code, 2);
    assert_eq!(document["details"]["reason"], Value::from("invalid_value"));
}

#[test]
fn witness_set_default_replaces_the_node_wide_set() {
    let home = Home::new();
    let endpoint = home.endpoint();
    // A second home is the simplest source of another real endpoint id: an
    // arbitrary 32 bytes is not a valid one.
    let other = &Home::new().endpoint();

    let document = home.json(&["witness", "set-default", &endpoint, other]);
    assert_shape(
        &document,
        &fixture("witness-set-default", "set"),
        "witness-set-default",
    );
    assert_eq!(
        document["witnesses"],
        serde_json::json!([endpoint.clone(), other])
    );
    assert_eq!(config_witnesses(&home).len(), 2);

    // The set is replaced, not added to, and a repeat is dropped.
    let document = home.json(&["witness", "set-default", other, other]);
    assert_eq!(document["witnesses"], serde_json::json!([other]));
    assert_eq!(config_witnesses(&home).len(), 1);

    let (code, stdout, _) = home.run(&["witness", "set-default", other]);
    assert_eq!(code, 0);
    assert!(stdout.contains("1 default witness"), "{stdout}");
    assert!(stdout.contains(other), "{stdout}");
}

#[test]
fn a_malformed_default_witness_exits_2_and_leaves_node_json_alone() {
    let home = Home::new();
    home.json(&["witness", "set-default", &home.endpoint()]);
    let (code, document) = home.failure(&["witness", "set-default", "not-an-endpoint"]);
    assert_eq!(code, 2);
    assert_eq!(
        document["details"]["reason"],
        Value::from("malformed_endpoint_id")
    );
    assert_eq!(config_witnesses(&home).len(), 1);
}

/// 32 bytes as every document spells an id.
fn base32(value: &[u8]) -> String {
    data_encoding::BASE32_NOPAD
        .encode(value)
        .to_ascii_lowercase()
}

/// `node.json.witnesses`, which `iroh_base` spells as hex.
fn config_witnesses(home: &Home) -> Vec<String> {
    let bytes = std::fs::read(home.path().join("node.json")).expect("node.json is there");
    let config: Value = serde_json::from_slice(&bytes).expect("node.json is JSON");
    config["witnesses"]
        .as_array()
        .expect("witnesses is an array")
        .iter()
        .map(text)
        .collect()
}

#[test]
fn a_missing_argument_matches_the_usage_fixture() {
    let home = Home::new();
    let (code, document) = home.failure(&["trust", "add", "--issuer", "alice"]);
    assert_eq!(code, 2);
    assert_eq!(document, fixture("errors", "usage"));
}

#[test]
fn an_unknown_argument_exits_2() {
    let home = Home::new();
    let (code, document) = home.failure(&["identity", "list", "--everything"]);
    assert_eq!(code, 2);
    assert_shape(&document, &fixture("errors", "usage"), "errors/usage");
    assert_eq!(
        document["details"]["reason"],
        Value::from("unknown_argument")
    );
    assert_eq!(document["details"]["argument"], Value::from("--everything"));

    // Text mode keeps clap's own rendering, with the usage line.
    let (code, _, stderr) = home.run(&["identity", "list", "--everything"]);
    assert_eq!(code, 2);
    assert!(stderr.contains("--everything"), "{stderr}");
}

#[test]
fn an_unknown_alias_exits_2_and_an_alias_resolves_to_its_id() {
    let home = Home::new();
    let alice = home.create("alice");
    let bob = home.create("bob");

    let (code, document) = home.failure(&["trust", "add", "--issuer", "carol", "--subject", &bob]);
    assert_eq!(code, 2);
    assert_eq!(document["details"]["reason"], Value::from("unknown_alias"));
    assert_eq!(document["details"]["alias"], Value::from("carol"));

    // The alias and the id name the same ledger, and only the id is signed.
    let document = home.json(&["trust", "add", "--issuer", "alice", "--subject", "bob"]);
    assert_eq!(document["issuer"], Value::from(alice));
    assert_eq!(document["subject"], Value::from(bob));
}

#[cfg(unix)]
#[test]
fn a_group_readable_key_file_exits_60_unless_the_flag_is_passed() {
    use std::os::unix::fs::PermissionsExt;

    let home = Home::new();
    let alice = home.create("alice");
    let bob = home.create("bob");
    let key = home
        .path()
        .join("identities")
        .join(&alice)
        .join("active.key");
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).expect("chmod");

    let (code, document) = home.failure(&["trust", "add", "--issuer", "alice", "--subject", &bob]);
    assert_eq!(code, 60);
    assert_shape(
        &document,
        &fixture("errors", "insecure-permissions"),
        "errors/insecure-permissions",
    );
    assert_eq!(
        document["details"]["reason"],
        Value::from("insecure_key_permissions")
    );
    assert_eq!(document["details"]["mode"], Value::from("0644"));
    assert_eq!(document["details"]["expected_mode"], Value::from("0600"));
    assert_eq!(
        document["details"]["path"],
        Value::from(format!("identities/{alice}/active.key"))
    );
    assert!(
        text(&document["message"]).starts_with("key file has insecure permissions: "),
        "{document}"
    );

    let document = home.json(&[
        "trust",
        "add",
        "--issuer",
        "alice",
        "--subject",
        &bob,
        "--allow-insecure-permissions",
    ]);
    assert_eq!(document["attestation_seq"], Value::from(1));
}

#[test]
fn the_global_flags_are_accepted_before_and_after_the_subcommand() {
    let home = Home::new();
    let mut command = binary();
    command
        .arg("--json")
        .arg("--verbose")
        .arg("--home")
        .arg(home.path())
        .args(["identity", "create", "--alias", "alice"]);
    let (code, stdout, _) = output(&mut command);
    assert_eq!(code, 0);
    let document = parse(&stdout);
    assert_eq!(document["ok"], Value::Bool(true));
    assert_eq!(document["alias"], Value::from("alice"));
}

#[test]
fn witness_run_refuses_a_peer_that_is_not_a_ticket() {
    let home = Home::new();
    let (code, document) = home.failure(&["witness", "run", "--peer", "nope"]);
    assert_eq!(code, 2);
    assert_eq!(
        document["details"]["reason"],
        Value::from("malformed_peer_ticket")
    );
    assert_eq!(document["details"]["value"], Value::from("nope"));
}

#[test]
fn witness_run_takes_an_http_address_an_iroh_port_peer_tickets_and_a_ui_dir() {
    let (code, stdout, _) = output(binary().args(["witness", "run", "--help"]));
    assert_eq!(code, 0);
    for flag in ["--http", "--iroh-port", "--peer", "--ui-dir"] {
        assert!(stdout.contains(flag), "{flag} is not in the help: {stdout}");
    }
}

#[test]
fn wallet_serve_takes_an_http_address_an_iroh_port_peer_tickets_and_a_ui_dir() {
    let (code, stdout, _) = output(binary().args(["wallet", "serve", "--help"]));
    assert_eq!(code, 0);
    for flag in ["--http", "--iroh-port", "--peer", "--ui-dir"] {
        assert!(stdout.contains(flag), "{flag} is not in the help: {stdout}");
    }
}

/// Flips one bit of a stored event's signature, which the fold refuses.
fn tamper(path: &Path) {
    let mut bytes = std::fs::read(path).expect("the event file");
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    std::fs::write(path, &bytes).expect("the event file is writable");
}
