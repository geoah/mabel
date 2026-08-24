//! `mabel profile`, `mabel contact`, `mabel graph` and `mabel lookup` against
//! the frozen fixtures in `contracts/cli/` (ticket 026, proposal 003).
//!
//! Every home here sets `relay: "disabled"` in `node.json` and names no
//! witness, so a crawl reads the local copies under `ledgers/` and nothing
//! touches DNS, a relay or the internet (proposal 001 section 11).
//!
//! Documents are compared to their fixture key for key: the ids and timestamps
//! a temp home produces are its own, so the assertion is on the shape, not the
//! values.

use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

/// An id no home here holds, for the lookup that finds nothing.
const STRANGER: &str = "jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq";

/// A temp node home and the binary that runs against it.
struct Home {
    directory: TempDir,
}

impl Home {
    /// A wallet home with `relay: "disabled"`.
    fn new() -> Self {
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

    fn run(&self, arguments: &[&str]) -> (i32, String, String) {
        output(self.command(arguments))
    }

    /// Runs a command with `stdin`, which is what the confirmation reads.
    fn answering(&self, arguments: &[&str], stdin: &str) -> (i32, String, String) {
        let mut command = self.command(arguments);
        command.write_stdin(stdin.to_owned());
        output(command)
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

    /// Runs a `--json` command that must fail.
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

    fn create(&self, alias: &str) -> String {
        text(&self.json(&["identity", "create", "--alias", alias])["identity_id"])
    }
}

fn output(mut command: Command) -> (i32, String, String) {
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

// ---------------------------------------------------------------- profile ----

#[test]
fn profile_replace_matches_the_fixture_and_lands_on_the_identity_document() {
    let home = Home::new();
    let alice = home.create("alice");

    let document = home.json(&[
        "profile",
        "replace",
        "--identity",
        "alice",
        "--display-name",
        "Alice Ashworth",
        "--hostname",
        "alice.example",
        "--yes",
    ]);
    assert_shape(
        &document,
        &fixture("profile-replace", "replaced"),
        "profile-replace",
    );
    assert_eq!(document["identity_id"], Value::from(alice.clone()));
    assert_eq!(document["display_name"], Value::from("Alice Ashworth"));
    assert_eq!(document["previous"]["display_name"], Value::Null);

    let shown = home.json(&["identity", "show", "alice"]);
    assert_eq!(shown["profile"]["hostname"], Value::from("alice.example"));
    assert_eq!(
        shown["profile"]["signing_principal"]["identity"],
        Value::from(alice)
    );
    // Cache-only: nothing has checked the claim, and the document says so
    // rather than reporting a lookup that never ran.
    assert_eq!(shown["verification"]["status"], Value::from("unverified"));
    assert_eq!(shown["verification"]["checked_at_ms"], Value::Null);
    assert_eq!(shown["verification"]["stale"], Value::Bool(true));
    assert_eq!(shown["contact"], Value::Null);
}

/// The email is one of the three fields one update replaces, and the scanner
/// owns what a valid one looks like (proposal 005).
#[test]
fn profile_replace_publishes_an_email_and_the_identity_document_reports_it() {
    let home = Home::new();
    home.create("alice");

    let document = home.json(&[
        "profile",
        "replace",
        "--identity",
        "alice",
        "--display-name",
        "Alice Ashworth",
        "--hostname",
        "alice.example",
        "--email",
        "alice@alice.example",
        "--yes",
    ]);
    assert_shape(
        &document,
        &fixture("profile-replace", "replaced"),
        "profile-replace",
    );
    assert_eq!(document["email"], Value::from("alice@alice.example"));
    assert_eq!(document["previous"]["email"], Value::Null);
    assert_eq!(
        home.json(&["identity", "show", "alice"])["profile"]["email"],
        Value::from("alice@alice.example")
    );

    // Omitting the flag clears it, like the other two fields.
    let cleared = home.json(&[
        "profile",
        "replace",
        "--identity",
        "alice",
        "--display-name",
        "Alice Ashworth",
        "--hostname",
        "alice.example",
        "--yes",
    ]);
    assert_eq!(cleared["email"], Value::Null);
    assert_eq!(
        cleared["previous"]["email"],
        Value::from("alice@alice.example")
    );
}

/// The scanner refuses the event before it is stored, and the reason it pins
/// is what the person reads.
#[test]
fn an_email_the_scanner_refuses_matches_the_fixture_and_exits_10() {
    let home = Home::new();
    let alice = home.create("alice");
    let expected = fixture("profile-replace", "invalid-email");

    let (code, document) = home.failure(&[
        "profile",
        "replace",
        "--identity",
        "alice",
        "--email",
        "alice.example",
        "--yes",
    ]);
    assert_eq!(code, 10);
    assert_eq!(document["message"], expected["message"]);
    assert_shape(&document, &expected, "profile-replace/invalid-email");
    assert_eq!(document["details"]["reason"], Value::from("invalid_email"));
    assert_eq!(document["details"]["ledger_id"], Value::from(alice));

    // Nothing was signed: the refused event never reached the chain.
    assert_eq!(
        home.json(&["identity", "show", "alice"])["profile"],
        Value::Null
    );
}

#[test]
fn profile_replace_clears_the_field_it_omits() {
    let home = Home::new();
    home.create("alice");
    home.json(&[
        "profile",
        "replace",
        "--identity",
        "alice",
        "--display-name",
        "Alice Ashworth",
        "--hostname",
        "alice.example",
        "--yes",
    ]);

    let document = home.json(&[
        "profile",
        "replace",
        "--identity",
        "alice",
        "--hostname",
        "alice.example",
        "--yes",
    ]);
    assert_shape(
        &document,
        &fixture("profile-replace", "cleared"),
        "profile-replace/cleared",
    );
    assert_eq!(document["display_name"], Value::Null);
    assert_eq!(
        document["previous"]["display_name"],
        Value::from("Alice Ashworth")
    );
    assert_eq!(
        home.json(&["identity", "show", "alice"])["profile"]["display_name"],
        Value::Null
    );
}

#[test]
fn profile_replace_prints_the_diff_and_asks_for_confirmation() {
    let home = Home::new();
    home.create("alice");
    let arguments = [
        "profile",
        "replace",
        "--identity",
        "alice",
        "--display-name",
        "Alice Ashworth",
        "--hostname",
        "alice.example",
    ];

    let (code, stdout, _) = home.answering(&arguments, "no\n");
    assert_eq!(code, 2, "{stdout}");
    assert!(
        stdout.contains("display name: (unset) -> Alice Ashworth"),
        "{stdout}"
    );
    assert!(
        stdout.contains("hostname:     (unset) -> alice.example"),
        "{stdout}"
    );
    assert!(stdout.contains("type yes to sign"), "{stdout}");
    // A hostname is a public claim, and the diff says so before anything is
    // signed (proposal 003 consequences).
    assert!(stdout.contains("readable forever"), "{stdout}");
    assert_eq!(
        home.json(&["identity", "show", "alice"])["profile"],
        Value::Null,
        "nothing was signed"
    );

    let (code, stdout, stderr) = home.answering(&arguments, "yes\n");
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert_eq!(
        home.json(&["identity", "show", "alice"])["profile"]["display_name"],
        Value::from("Alice Ashworth")
    );
}

#[test]
fn profile_replace_with_json_and_no_yes_matches_the_fixture() {
    let home = Home::new();
    home.create("alice");
    let (code, document) = home.failure(&[
        "profile",
        "replace",
        "--identity",
        "alice",
        "--display-name",
        "Alice Ashworth",
    ]);
    assert_eq!(code, 2);
    assert_eq!(
        document["details"]["reason"],
        Value::from("confirmation_required")
    );
    assert_shape(
        &document,
        &fixture("profile-replace", "confirmation-required-with-json"),
        "profile-replace/confirmation-required-with-json",
    );
}

#[test]
fn a_profile_replace_that_changes_nothing_matches_the_no_op_fixture() {
    let home = Home::new();
    let alice = home.create("alice");
    let arguments = [
        "profile",
        "replace",
        "--identity",
        "alice",
        "--display-name",
        "Alice Ashworth",
        "--hostname",
        "alice.example",
        "--yes",
    ];
    let first = home.json(&arguments);

    let (code, document) = home.failure(&arguments);
    assert_eq!(code, 20);
    assert_shape(
        &document,
        &fixture("profile-replace", "no-op"),
        "profile-replace/no-op",
    );
    assert_eq!(
        document["details"]["reason"],
        Value::from("no_op_profile_update")
    );
    assert_eq!(document["details"]["ledger_id"], Value::from(alice));
    assert_eq!(
        home.json(&["identity", "show", "alice"])["head_seq"],
        first["head_seq"],
        "the refused update never reached the chain"
    );
}

// --------------------------------------------------------------- contacts ----

#[test]
fn contact_set_and_show_match_the_fixture_for_a_foreign_identity() {
    let home = Home::new();
    home.create("alice");

    let set = home.json(&[
        "contact",
        "set",
        STRANGER,
        "--nickname",
        "bob at the print shop",
        "--note",
        "met at the 2023 zine fair; verifies his own hostname",
    ]);
    assert_shape(&set, &fixture("contact-set", "set"), "contact-set");
    assert_eq!(set["identity_id"], Value::from(STRANGER));
    assert_eq!(
        set["contact"]["nickname"],
        Value::from("bob at the print shop")
    );

    let shown = home.json(&["contact", "show", STRANGER]);
    assert_shape(
        &shown,
        &fixture("contact-set", "shown"),
        "contact-set/shown",
    );
    assert_eq!(shown["contact"], set["contact"]);

    // The note is never signed and never part of the ledger: a stranger has no
    // identity directory here at all.
    assert!(!home.path().join("identities").join(STRANGER).is_dir());
    assert!(
        home.path()
            .join("contacts")
            .join(format!("{STRANGER}.json"))
            .is_file()
    );

    let cleared = home.json(&["contact", "set", STRANGER]);
    assert_eq!(cleared["contact"], Value::Null);
    let empty = home.json(&["contact", "show", STRANGER]);
    assert_shape(
        &empty,
        &fixture("contact-set", "no-contact-recorded"),
        "contact-set/no-contact-recorded",
    );
}

#[test]
fn a_contact_shows_up_on_the_identity_document() {
    let home = Home::new();
    home.create("alice");
    home.json(&["contact", "set", "alice", "--nickname", "me"]);
    let shown = home.json(&["identity", "show", "alice"]);
    assert_eq!(shown["contact"]["nickname"], Value::from("me"));
    assert_eq!(shown["contact"]["note"], Value::Null);
}

#[test]
fn a_nickname_over_the_cap_matches_the_fixture_and_exits_10() {
    let home = Home::new();
    let expected = fixture("contact-set", "nickname-too-long");
    let len = expected["details"]["len"].as_u64().expect("a length") as usize;
    let long = "n".repeat(len);
    let (code, document) = home.failure(&["contact", "set", STRANGER, "--nickname", &long]);
    assert_eq!(code, 10);
    assert_eq!(document["message"], expected["message"]);
    assert_eq!(document["details"], expected["details"]);
}

// ------------------------------------------------------------ graph, lookup --

#[test]
fn graph_status_before_any_crawl_matches_the_fixture() {
    let home = Home::new();
    home.create("alice");
    let document = home.json(&["graph", "status"]);
    assert_shape(
        &document,
        &fixture("graph-sync", "never-synchronized"),
        "graph-sync/never-synchronized",
    );
    assert_eq!(document["graph"], Value::Null);

    let (_, stdout, _) = home.run(&["graph", "status"]);
    assert!(stdout.contains("no crawl has run in this home"), "{stdout}");
}

#[test]
fn a_home_with_no_identity_cannot_be_crawled() {
    let home = Home::new();
    let (code, document) = home.failure(&["graph", "sync"]);
    assert_eq!(code, 2);
    assert_shape(
        &document,
        &fixture("graph-sync", "no-local-identity"),
        "graph-sync/no-local-identity",
    );
    assert_eq!(
        document["details"]["reason"],
        Value::from("no_local_identity")
    );
}

#[test]
fn graph_sync_then_status_match_the_fixture_and_lookup_answers_from_the_crawl() {
    let home = Home::new();
    let alice = home.create("alice");
    let bob = home.create("bob");
    home.json(&["trust", "add", "--issuer", "alice", "--subject", &bob]);

    let synced = home.json(&["graph", "sync"]);
    assert_shape(
        &synced,
        &fixture("graph-sync", "synchronized"),
        "graph-sync/synchronized",
    );
    let graph = &synced["graph"];
    assert_eq!(graph["node_count"], Value::from(2));
    assert_eq!(graph["edge_count"], Value::from(1));
    assert_eq!(graph["stale"], Value::Bool(false));
    assert_eq!(graph["roots"].as_array().expect("roots").len(), 2);

    let status = home.json(&["graph", "status"]);
    assert_shape(
        &status,
        &fixture("graph-sync", "status"),
        "graph-sync/status",
    );
    assert_eq!(status["graph"]["sync_id"], graph["sync_id"]);

    let lookup = home.json(&["lookup", &bob, "--from", "alice"]);
    assert_shape(&lookup, &fixture("lookup", "two-degrees"), "lookup");
    assert_eq!(lookup["degrees"], Value::from(1));
    assert_eq!(lookup["identity"]["identity_id"], Value::from(bob.clone()));
    assert_eq!(lookup["from"]["identity_id"], Value::from(alice));
    assert_eq!(lookup["reverse"]["best_effort"], Value::Bool(true));
    assert_eq!(
        lookup["reverse"]["entries"]
            .as_array()
            .expect("entries")
            .len(),
        1
    );
    assert_eq!(lookup["graph_stale"], Value::Bool(false));

    let (_, stdout, _) = home.run(&["lookup", &bob, "--from", "alice"]);
    assert!(stdout.contains("1 degrees in this crawl"), "{stdout}");
}

#[test]
fn a_lookup_for_an_identity_absent_from_the_crawl_exits_0_with_null_degrees() {
    let home = Home::new();
    home.create("alice");
    home.json(&["graph", "sync"]);

    let document = home.json(&["lookup", STRANGER, "--from", "alice"]);
    assert_shape(
        &document,
        &fixture("lookup", "not-in-this-crawl"),
        "lookup/not-in-this-crawl",
    );
    assert_eq!(document["degrees"], Value::Null);
    assert_eq!(document["paths"], Value::Array(Vec::new()));
    assert_eq!(document["stale"], Value::Bool(true));

    let (code, stdout, _) = home.run(&["lookup", STRANGER, "--from", "alice"]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("no path in this crawl, which is not the same as no relationship"),
        "{stdout}"
    );
}

#[test]
fn a_lookup_from_an_identity_this_home_does_not_hold_exits_2() {
    let home = Home::new();
    home.create("alice");
    let (code, document) = home.failure(&["lookup", "alice", "--from", STRANGER]);
    assert_eq!(code, 2);
    assert_eq!(
        document["details"]["reason"],
        Value::from("unknown_from_identity")
    );
}

#[test]
fn a_lookup_with_no_from_uses_the_lowest_local_identity() {
    let home = Home::new();
    home.create("one");
    let second = home.create("two");
    home.json(&["graph", "sync"]);
    // The same ascending order `mabel identity list` and `GET /api/identities`
    // sort by, so the default root is the first row a person sees.
    let lowest = home.json(&["identity", "list"])["identities"][0]["identity_id"].clone();

    let document = home.json(&["lookup", &second]);
    assert_eq!(document["from"]["identity_id"], lowest);
}
