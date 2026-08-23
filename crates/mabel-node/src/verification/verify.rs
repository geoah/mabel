//! The hostname check: query construction, matching and the four statuses a
//! lookup can produce (proposal 003 section 2).
//!
//! The check is advisory. It never gates ledger validity (decision 015) and
//! runs on the wallet node only; a witness reports a hostname as claimed.

use mabel_core::{IdentityId, MAX_HOSTNAME_BYTES, MAX_HOSTNAME_LABEL_BYTES};
use serde::{Deserialize, Serialize};

use super::resolver::{Resolver, TxtRecord};

/// The label the TXT record sits under.
pub const TXT_LABEL: &str = "_mabel";

/// The prefix a matching TXT record carries, compared case-insensitively.
pub const TXT_PREFIX: &str = "mabel=";

/// How many CNAME links the check follows before giving up.
pub const MAX_CNAME_LINKS: usize = 4;

/// What the wallet knows about a claimed hostname (proposal 003 section 2).
///
/// Every status is advisory. `Unclaimed` never comes out of a lookup: it is
/// what the identity document reports when the profile names no hostname, and
/// it lives here so one enum spells the vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    /// A TXT record at the label carries this identity id.
    Verified,
    /// The label carries no `mabel=` record.
    Unverified,
    /// The label carries `mabel=` records, none of them this identity id.
    Mismatched,
    /// The lookup did not answer: a timeout, a resolver error, or a CNAME
    /// chain that loops or runs past four links.
    Unreachable,
    /// The profile names no hostname, so nothing was queried.
    Unclaimed,
}

impl VerificationStatus {
    /// The wire spelling, the one `contracts/` freezes.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Mismatched => "mismatched",
            Self::Unreachable => "unreachable",
            Self::Unclaimed => "unclaimed",
        }
    }

    /// True for `verified` and `mismatched`, the results a failed re-check
    /// never overwrites (proposal 003 section 2).
    #[must_use]
    pub fn is_decisive(self) -> bool {
        matches!(self, Self::Verified | Self::Mismatched)
    }
}

impl std::fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One run of the check against one claimed hostname.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationOutcome {
    /// The hostname the check ran against, as the profile spells it. The
    /// cache entry is bound to it.
    pub hostname: String,
    /// What the lookup found.
    pub status: VerificationStatus,
    /// One sentence naming what was queried and what came back.
    pub detail: String,
}

impl VerificationOutcome {
    fn new(hostname: &str, status: VerificationStatus, detail: String) -> Self {
        Self {
            hostname: hostname.to_owned(),
            status,
            detail,
        }
    }
}

/// The absolute name the check queries: `_mabel.<hostname>.`, root label
/// included.
///
/// Absolute is half of the rule; the other half is [`HickoryResolver`] with
/// its search list cleared, so no local suffix can be appended to a claim.
///
/// [`HickoryResolver`]: super::HickoryResolver
#[must_use]
pub fn query_name(hostname: &str) -> String {
    let trimmed = hostname.strip_suffix('.').unwrap_or(hostname);
    format!("{TXT_LABEL}.{trimmed}.")
}

/// Checks `hostname` against the TXT record at `_mabel.<hostname>.`.
///
/// Follows a CNAME at the label to at most [`MAX_CNAME_LINKS`] links; a
/// longer chain, a loop, a timeout and any resolver error are all
/// `unreachable`, never a refusal of the claim.
pub async fn verify_hostname(
    resolver: &dyn Resolver,
    hostname: &str,
    identity: IdentityId,
) -> VerificationOutcome {
    if let Err(reason) = check_hostname(hostname) {
        return VerificationOutcome::new(
            hostname,
            VerificationStatus::Unreachable,
            format!("{hostname} was not queried: {reason}"),
        );
    }

    let start = query_name(hostname);
    let mut name = start.clone();
    let mut visited = vec![name.clone()];
    let mut links = 0usize;

    loop {
        let records = match resolver.lookup_txt(&name).await {
            Ok(records) => records,
            Err(error) => {
                return VerificationOutcome::new(
                    hostname,
                    VerificationStatus::Unreachable,
                    error.to_string(),
                );
            }
        };
        if !records.is_empty() {
            return classify(hostname, &name, &records, identity);
        }

        // DNS forbids a CNAME beside other data, so an alias is only worth
        // asking about when the name carried no TXT record.
        let target = match resolver.lookup_cname(&name).await {
            Ok(Some(target)) => absolute(&target),
            Ok(None) => {
                return VerificationOutcome::new(
                    hostname,
                    VerificationStatus::Unverified,
                    format!("{name} holds no {TXT_PREFIX} TXT record"),
                );
            }
            Err(error) => {
                return VerificationOutcome::new(
                    hostname,
                    VerificationStatus::Unreachable,
                    error.to_string(),
                );
            }
        };

        links += 1;
        if links > MAX_CNAME_LINKS {
            return VerificationOutcome::new(
                hostname,
                VerificationStatus::Unreachable,
                format!("the CNAME chain from {start} runs past {MAX_CNAME_LINKS} links"),
            );
        }
        if visited.contains(&target) {
            return VerificationOutcome::new(
                hostname,
                VerificationStatus::Unreachable,
                format!("the CNAME chain from {start} returns to {target}"),
            );
        }
        visited.push(target.clone());
        name = target;
    }
}

/// Reads the records at one name: `verified` on the first match, else
/// `mismatched` when any `mabel=` record is there, else `unverified`.
fn classify(
    hostname: &str,
    name: &str,
    records: &[TxtRecord],
    identity: IdentityId,
) -> VerificationOutcome {
    let mut claims = 0usize;
    for record in records {
        let value = record.value();
        let Some(claimed) = mabel_claim(&value) else {
            continue;
        };
        claims += 1;
        if claimed
            .parse::<IdentityId>()
            .is_ok_and(|parsed| parsed == identity)
        {
            return VerificationOutcome::new(
                hostname,
                VerificationStatus::Verified,
                format!("a TXT record at {name} carries {TXT_PREFIX}{identity}"),
            );
        }
    }
    match claims {
        0 => VerificationOutcome::new(
            hostname,
            VerificationStatus::Unverified,
            format!("{name} holds no {TXT_PREFIX} TXT record"),
        ),
        1 => VerificationOutcome::new(
            hostname,
            VerificationStatus::Mismatched,
            format!("the {TXT_PREFIX} record at {name} names another identity"),
        ),
        many => VerificationOutcome::new(
            hostname,
            VerificationStatus::Mismatched,
            format!("the {many} {TXT_PREFIX} records at {name} name other identities"),
        ),
    }
}

/// The text after a case-insensitive `mabel=` prefix, or `None` when the
/// record is something else.
///
/// The remainder must be UTF-8; the id codec, which is case-insensitive,
/// judges the rest.
#[must_use]
pub fn mabel_claim(value: &[u8]) -> Option<&str> {
    let prefix = TXT_PREFIX.len();
    let (head, rest) = value.split_at_checked(prefix)?;
    if !head.eq_ignore_ascii_case(TXT_PREFIX.as_bytes()) {
        return None;
    }
    Some(std::str::from_utf8(rest).unwrap_or(""))
}

/// A CNAME target as an absolute lowercase name.
fn absolute(target: &str) -> String {
    let lowered = target.to_ascii_lowercase();
    if lowered.ends_with('.') {
        lowered
    } else {
        format!("{lowered}.")
    }
}

/// The hostname syntax of proposal 003 section 2, checked again here because
/// a cached or hand-edited claim never passed the wire validator.
///
/// The same reasons `mabel-core` gives, so one claim reads the same wherever
/// it is refused. `GET /api/resolve/{hostname}` validates its path segment
/// with this, so the route accepts exactly the names a profile may claim.
///
/// # Errors
///
/// Returns the clause naming which rule the name broke.
pub fn check_hostname(text: &str) -> Result<(), &'static str> {
    if text.is_empty() {
        return Err("it is empty");
    }
    if text.len() > MAX_HOSTNAME_BYTES {
        return Err("it is over 246 bytes");
    }
    if !text.is_ascii() {
        return Err("it holds a character outside ASCII");
    }
    if text.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err("it holds an uppercase letter");
    }
    if text.ends_with('.') {
        return Err("it ends with a dot");
    }
    if !text.contains('.') {
        return Err("it holds no dot");
    }
    for label in text.split('.') {
        let bytes = label.as_bytes();
        let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
            return Err("it holds an empty label");
        };
        if bytes.len() > MAX_HOSTNAME_LABEL_BYTES {
            return Err("it holds a label over 63 bytes");
        }
        if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
            return Err("a label does not start and end with a letter or digit");
        }
        if !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        {
            return Err("a label holds a character outside [a-z0-9-]");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use mabel_core::IdentityId;

    use super::super::resolver::{StubResolver, TxtRecord};
    use super::{VerificationStatus, query_name, verify_hostname};

    const HOSTNAME: &str = "alice.example";
    const NAME: &str = "_mabel.alice.example.";

    fn identity() -> IdentityId {
        IdentityId::from_bytes([7u8; 32])
    }

    fn other() -> IdentityId {
        IdentityId::from_bytes([9u8; 32])
    }

    fn claim(id: IdentityId) -> String {
        format!("mabel={id}")
    }

    #[test]
    fn the_query_name_is_absolute_and_labelled() {
        assert_eq!(query_name(HOSTNAME), NAME);
        assert_eq!(query_name("a.b.c.example"), "_mabel.a.b.c.example.");
    }

    #[tokio::test]
    async fn a_matching_record_verifies_and_only_the_absolute_name_is_queried() {
        let resolver = StubResolver::new().with_text(NAME, &claim(identity()));
        let outcome = verify_hostname(&resolver, HOSTNAME, identity()).await;

        assert_eq!(outcome.status, VerificationStatus::Verified);
        assert_eq!(outcome.hostname, HOSTNAME);
        assert!(outcome.detail.contains(NAME), "{}", outcome.detail);
        assert_eq!(resolver.queries(), vec![NAME.to_owned()]);
    }

    #[tokio::test]
    async fn a_mabel_record_naming_another_identity_mismatches() {
        let resolver = StubResolver::new().with_text(NAME, &claim(other()));
        let outcome = verify_hostname(&resolver, HOSTNAME, identity()).await;

        assert_eq!(outcome.status, VerificationStatus::Mismatched);
        assert_eq!(
            outcome.detail,
            format!("the mabel= record at {NAME} names another identity")
        );
    }

    #[tokio::test]
    async fn records_that_are_not_mabel_records_leave_the_claim_unverified() {
        let resolver = StubResolver::new().with_records(
            NAME,
            vec![
                TxtRecord::from_strings(["v=spf1 -all"]),
                TxtRecord::from_strings(["mabelish=nope"]),
            ],
        );
        let outcome = verify_hostname(&resolver, HOSTNAME, identity()).await;

        assert_eq!(outcome.status, VerificationStatus::Unverified);
        assert_eq!(outcome.detail, format!("{NAME} holds no mabel= TXT record"));
    }

    #[tokio::test]
    async fn a_label_with_no_records_at_all_is_unverified() {
        let resolver = StubResolver::new();
        let outcome = verify_hostname(&resolver, HOSTNAME, identity()).await;

        assert_eq!(outcome.status, VerificationStatus::Unverified);
        assert_eq!(resolver.queries(), vec![NAME.to_owned(), NAME.to_owned()]);
    }

    #[tokio::test]
    async fn one_matching_record_among_several_mabel_records_verifies() {
        let resolver = StubResolver::new().with_records(
            NAME,
            vec![
                TxtRecord::from_strings([claim(other())]),
                TxtRecord::from_strings(["v=spf1 -all"]),
                TxtRecord::from_strings([claim(identity())]),
            ],
        );
        assert_eq!(
            verify_hostname(&resolver, HOSTNAME, identity())
                .await
                .status,
            VerificationStatus::Verified
        );
    }

    #[tokio::test]
    async fn several_mabel_records_and_no_match_is_mismatched() {
        let resolver = StubResolver::new().with_records(
            NAME,
            vec![
                TxtRecord::from_strings([claim(other())]),
                TxtRecord::from_strings([claim(IdentityId::from_bytes([3u8; 32]))]),
            ],
        );
        let outcome = verify_hostname(&resolver, HOSTNAME, identity()).await;

        assert_eq!(outcome.status, VerificationStatus::Mismatched);
        assert_eq!(
            outcome.detail,
            format!("the 2 mabel= records at {NAME} name other identities")
        );
    }

    #[tokio::test]
    async fn character_strings_are_concatenated_within_one_record() {
        let claim = claim(identity());
        let (head, tail) = claim.split_at(10);
        let resolver =
            StubResolver::new().with_records(NAME, vec![TxtRecord::from_strings([head, tail])]);

        assert_eq!(
            verify_hostname(&resolver, HOSTNAME, identity())
                .await
                .status,
            VerificationStatus::Verified
        );
    }

    #[tokio::test]
    async fn character_strings_are_never_concatenated_across_records() {
        let claim = claim(identity());
        let (head, tail) = claim.split_at(10);
        let resolver = StubResolver::new().with_records(
            NAME,
            vec![
                TxtRecord::from_strings([head]),
                TxtRecord::from_strings([tail]),
            ],
        );
        let outcome = verify_hostname(&resolver, HOSTNAME, identity()).await;

        // The first record is `mabel=` with a truncated id, so the label does
        // carry a claim; it just is not this one.
        assert_eq!(outcome.status, VerificationStatus::Mismatched);
    }

    #[tokio::test]
    async fn the_prefix_and_the_id_are_both_matched_case_insensitively() {
        let value = format!("MaBeL={}", identity().to_string().to_ascii_uppercase());
        let resolver = StubResolver::new().with_text(NAME, &value);

        assert_eq!(
            verify_hostname(&resolver, HOSTNAME, identity())
                .await
                .status,
            VerificationStatus::Verified
        );
    }

    #[tokio::test]
    async fn a_prefix_with_junk_after_it_is_a_claim_that_does_not_match() {
        let resolver = StubResolver::new().with_text(NAME, "mabel=not-an-id");
        let outcome = verify_hostname(&resolver, HOSTNAME, identity()).await;

        assert_eq!(outcome.status, VerificationStatus::Mismatched);
    }

    #[tokio::test]
    async fn a_cname_chain_of_four_links_is_followed_to_the_record() {
        let resolver = StubResolver::new()
            .with_cname(NAME, "one.example.")
            .with_cname("one.example.", "two.example.")
            .with_cname("two.example.", "three.example.")
            .with_cname("three.example.", "four.example.")
            .with_text("four.example.", &claim(identity()));

        let outcome = verify_hostname(&resolver, HOSTNAME, identity()).await;
        assert_eq!(outcome.status, VerificationStatus::Verified);
        assert!(
            outcome.detail.contains("four.example."),
            "{}",
            outcome.detail
        );
    }

    #[tokio::test]
    async fn a_cname_chain_of_five_links_is_unreachable() {
        let resolver = StubResolver::new()
            .with_cname(NAME, "one.example.")
            .with_cname("one.example.", "two.example.")
            .with_cname("two.example.", "three.example.")
            .with_cname("three.example.", "four.example.")
            .with_cname("four.example.", "five.example.")
            .with_text("five.example.", &claim(identity()));

        let outcome = verify_hostname(&resolver, HOSTNAME, identity()).await;
        assert_eq!(outcome.status, VerificationStatus::Unreachable);
        assert_eq!(
            outcome.detail,
            format!("the CNAME chain from {NAME} runs past 4 links")
        );
    }

    #[tokio::test]
    async fn a_cname_loop_is_unreachable() {
        let resolver = StubResolver::new()
            .with_cname(NAME, "one.example.")
            .with_cname("one.example.", NAME);

        let outcome = verify_hostname(&resolver, HOSTNAME, identity()).await;
        assert_eq!(outcome.status, VerificationStatus::Unreachable);
        assert_eq!(
            outcome.detail,
            format!("the CNAME chain from {NAME} returns to {NAME}")
        );
    }

    #[tokio::test]
    async fn a_relative_cname_target_is_made_absolute_and_lowercased() {
        let resolver = StubResolver::new()
            .with_cname(NAME, "Records.Example")
            .with_text("records.example.", &claim(identity()));

        assert_eq!(
            verify_hostname(&resolver, HOSTNAME, identity())
                .await
                .status,
            VerificationStatus::Verified
        );
    }

    #[tokio::test]
    async fn a_timeout_and_a_resolver_error_are_both_unreachable() {
        let timed_out = StubResolver::new().with_timeout(NAME);
        let outcome = verify_hostname(&timed_out, HOSTNAME, identity()).await;
        assert_eq!(outcome.status, VerificationStatus::Unreachable);
        assert_eq!(outcome.detail, format!("the query for {NAME} timed out"));

        let failed = StubResolver::new().with_failure(NAME, "SERVFAIL");
        let outcome = verify_hostname(&failed, HOSTNAME, identity()).await;
        assert_eq!(outcome.status, VerificationStatus::Unreachable);
        assert_eq!(
            outcome.detail,
            format!("the query for {NAME} failed: SERVFAIL")
        );
    }

    #[tokio::test]
    async fn a_hostname_that_breaks_the_syntax_is_never_queried() {
        for (hostname, reason) in [
            ("nodot", "it holds no dot"),
            ("Alice.example", "it holds an uppercase letter"),
            ("alice.example.", "it ends with a dot"),
            ("alice..example", "it holds an empty label"),
            (
                "-alice.example",
                "a label does not start and end with a letter or digit",
            ),
            (
                "ali_ce.example",
                "a label holds a character outside [a-z0-9-]",
            ),
            ("", "it is empty"),
        ] {
            let resolver = StubResolver::new();
            let outcome = verify_hostname(&resolver, hostname, identity()).await;
            assert_eq!(
                outcome.status,
                VerificationStatus::Unreachable,
                "{hostname}"
            );
            assert_eq!(
                outcome.detail,
                format!("{hostname} was not queried: {reason}")
            );
            assert!(resolver.queries().is_empty(), "{hostname}");
        }
    }

    #[test]
    fn the_statuses_spell_themselves_the_way_the_contract_does() {
        for (status, spelling) in [
            (VerificationStatus::Verified, "verified"),
            (VerificationStatus::Unverified, "unverified"),
            (VerificationStatus::Mismatched, "mismatched"),
            (VerificationStatus::Unreachable, "unreachable"),
            (VerificationStatus::Unclaimed, "unclaimed"),
        ] {
            assert_eq!(status.as_str(), spelling);
            assert_eq!(status.to_string(), spelling);
            assert_eq!(
                serde_json::to_string(&status).unwrap(),
                format!("\"{spelling}\"")
            );
        }
        assert!(VerificationStatus::Verified.is_decisive());
        assert!(VerificationStatus::Mismatched.is_decisive());
        assert!(!VerificationStatus::Unverified.is_decisive());
        assert!(!VerificationStatus::Unreachable.is_decisive());
    }
}
