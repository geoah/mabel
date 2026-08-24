//! The hostname check: query construction, matching and the four statuses a
//! lookup can produce (proposal 003 section 2).
//!
//! The check is advisory. It never gates ledger validity (decision 015) and
//! runs on the wallet node only; a witness reports a hostname as claimed.

use data_encoding::BASE32_NOPAD;
use iroh_base::EndpointId;
use mabel_core::id::ID_STR_LEN;
use mabel_core::{
    ID_BYTES, IdentityId, MAX_ENDPOINTS, MAX_HOSTNAME_BYTES, MAX_HOSTNAME_LABEL_BYTES, render_id,
};
use serde::{Deserialize, Serialize};

use super::resolver::{Resolver, TxtRecord};

/// The label the TXT record sits under.
pub const TXT_LABEL: &str = "_mabel";

/// The prefix a matching TXT record carries, compared case-insensitively.
pub const TXT_PREFIX: &str = "mabel=";

/// The second recognised prefix at the same label, compared the same way
/// (proposal 006 section 6).
pub const TXT_ENDPOINTS_PREFIX: &str = "mabel-endpoints=";

/// Most endpoints one label may name, between all its records.
///
/// The same cap the payload carries: one number for "how many machines answer
/// for one identity", wherever the list is read. A TXT character-string holds
/// 255 bytes and `mabel-endpoints=` plus four ids is 227, so a zone publishing
/// 5 to 8 splits them across two character-strings in one record, which the
/// concatenation rule joins back with no separator.
pub const MAX_LABEL_ENDPOINTS: usize = MAX_ENDPOINTS;

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

/// The text after a case-insensitive `mabel-endpoints=` prefix, or `None` when
/// the record is something else (proposal 006 section 6).
#[must_use]
pub fn endpoints_claim(value: &[u8]) -> Option<&str> {
    let prefix = TXT_ENDPOINTS_PREFIX.len();
    let (head, rest) = value.split_at_checked(prefix)?;
    if !head.eq_ignore_ascii_case(TXT_ENDPOINTS_PREFIX.as_bytes()) {
        return None;
    }
    Some(std::str::from_utf8(rest).unwrap_or(""))
}

/// The endpoints the records at one label name, unioned and sorted ascending
/// by their rendered base32 (proposal 006 section 6).
///
/// One overflow rule, discard whole, at both levels. A record with an
/// unparseable element, an empty element, a duplicate element, more than eight
/// elements, or a byte outside the codec's alphabet and the comma is discarded
/// whole; if the surviving records name more than eight distinct endpoints
/// between them, the label's set is discarded whole and reads as absent.
/// Nothing is trimmed to fit: choosing which eight of nine an operator meant is
/// a guess.
///
/// This is row 1 of the applicability matrix, the hostname the caller supplied:
/// what comes back belongs to the identity this same response resolved to.
#[must_use]
pub fn endpoints_at_label(records: &[TxtRecord]) -> Vec<EndpointId> {
    let mut endpoints: Vec<EndpointId> = Vec::new();
    for record in records {
        let value = record.value();
        let Some(listed) = endpoints_claim(&value) else {
            continue;
        };
        // A record that breaks a rule is discarded whole and leaves the
        // records beside it standing.
        let Some(parsed) = record_endpoints(listed) else {
            continue;
        };
        for endpoint in parsed {
            if !endpoints.contains(&endpoint) {
                endpoints.push(endpoint);
            }
        }
    }
    if endpoints.len() > MAX_LABEL_ENDPOINTS {
        return Vec::new();
    }
    // Two wallets derive the same set from the same zone, whatever order a
    // resolver returned the records in. The rendered form orders it, not the
    // bytes: base32 spells values 26 to 31 as the digits 2 to 7, which sort
    // before the letters in ASCII and after them in the codec.
    endpoints.sort_by_key(|endpoint| render_id(endpoint.as_bytes()));
    endpoints
}

/// What the records at one label say about a hostname the caller typed: row 1
/// of the applicability matrix (proposal 006 section 6).
///
/// A caller named this hostname for this operation, so the response may yield
/// both an identity and the endpoints beside it. The endpoints belong to the
/// identity this same response resolved to, which is why a label that resolved
/// to none reports none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallerZone {
    /// The identity the first parseable `mabel=` record names.
    pub identity: Option<IdentityId>,
    /// How many `mabel=` records the response carried, parseable or not.
    pub claims: usize,
    /// The endpoints at the label, empty when no `mabel=` record resolved.
    pub endpoints: Vec<EndpointId>,
}

/// Reads the records at a label a caller typed, under row 1 of the
/// applicability matrix (proposal 006 section 6).
///
/// `GET /api/resolve?input=<hostname>` and `mabel sync fetch --from-host` both
/// read a response this way, so one zone answers both the same.
#[must_use]
pub fn caller_zone(records: &[TxtRecord]) -> CallerZone {
    let mut zone = CallerZone::default();
    for record in records {
        let value = record.value();
        let Some(claimed) = mabel_claim(&value) else {
            continue;
        };
        zone.claims += 1;
        if zone.identity.is_none()
            && let Ok(identity) = claimed.parse::<IdentityId>()
        {
            zone.identity = Some(identity);
        }
    }
    // A label that resolved to no identity has no identity to offer endpoints
    // for, so its endpoints records are not read out.
    if zone.identity.is_some() {
        zone.endpoints = endpoints_at_label(records);
    }
    zone
}

/// The endpoints at one label, read for an identity that merely claimed the
/// hostname: only when a `mabel=` record at the same label names that identity
/// (row 2 of the applicability matrix, proposal 006 section 6).
///
/// The hostname came from the ledger's own `ProfileUpdate`, a stale local copy
/// or the stored crawl generation, so the zone has to say this identity is one
/// of the names it answers for. A zone that names other endpoints and not this
/// identity offers this identity nothing.
///
/// Nothing here is verification: the five verification statuses are about
/// `mabel=` alone, and no endpoints record is ever read, written or cached by
/// `verification/<identity_id>.json`.
#[must_use]
pub fn endpoints_for_claim(records: &[TxtRecord], identity: IdentityId) -> Vec<EndpointId> {
    let names_identity = records.iter().any(|record| {
        mabel_claim(&record.value())
            .and_then(|claimed| claimed.parse::<IdentityId>().ok())
            .is_some_and(|claimed| claimed == identity)
    });
    if names_identity {
        endpoints_at_label(records)
    } else {
        Vec::new()
    }
}

/// One record's list, or `None` when the record is discarded whole.
fn record_endpoints(listed: &str) -> Option<Vec<EndpointId>> {
    let mut endpoints: Vec<EndpointId> = Vec::new();
    for element in listed.split(',') {
        // An empty element and an element of the wrong length are both refused
        // here, before anything decodes.
        if element.len() != ID_STR_LEN {
            return None;
        }
        let decoded = BASE32_NOPAD
            .decode(element.to_ascii_uppercase().as_bytes())
            .ok()?;
        let bytes: [u8; ID_BYTES] = decoded.try_into().ok()?;
        let endpoint = EndpointId::from_bytes(&bytes).ok()?;
        if endpoints.contains(&endpoint) || endpoints.len() == MAX_LABEL_ENDPOINTS {
            return None;
        }
        endpoints.push(endpoint);
    }
    Some(endpoints)
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
    use iroh_base::{EndpointId, SecretKey};
    use mabel_core::{IdentityId, render_id};

    use super::super::resolver::{StubResolver, TxtRecord};
    use super::{
        TXT_ENDPOINTS_PREFIX, VerificationStatus, endpoints_at_label, endpoints_for_claim,
        query_name, verify_hostname,
    };

    const HOSTNAME: &str = "alice.example";
    const NAME: &str = "_mabel.alice.example.";

    /// One machine that answers, by seed. Every one is a real curve point,
    /// which the parser checks.
    fn endpoint(seed: u8) -> EndpointId {
        SecretKey::from_bytes(&[seed; 32]).public()
    }

    fn spelled(seed: u8) -> String {
        render_id(endpoint(seed).as_bytes())
    }

    /// One `mabel-endpoints=` record over these seeds.
    fn hint(seeds: &[u8]) -> TxtRecord {
        let listed: Vec<String> = seeds.iter().copied().map(spelled).collect();
        TxtRecord::from_strings([format!("{TXT_ENDPOINTS_PREFIX}{}", listed.join(","))])
    }

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

    // ------------------------------------------- mabel-endpoints= records ----

    /// Two records at one label produce the same sorted set whatever order the
    /// resolver returns them in, and one of the ids is split across two
    /// character-strings inside its record (proposal 006 section 6).
    #[test]
    fn records_at_one_label_are_unioned_and_sorted_whatever_order_they_arrive_in() {
        let split = {
            let listed = format!("{TXT_ENDPOINTS_PREFIX}{}", spelled(0x22));
            let (head, tail) = listed.split_at(listed.len() - 20);
            TxtRecord::from_strings([head, tail])
        };
        let first = hint(&[0x11, 0x33]);
        let one_way = endpoints_at_label(&[first.clone(), split.clone()]);
        let other_way = endpoints_at_label(&[split, first]);

        assert_eq!(one_way, other_way);
        assert_eq!(one_way.len(), 3);
        let rendered: Vec<String> = one_way
            .iter()
            .map(|endpoint| render_id(endpoint.as_bytes()))
            .collect();
        let mut sorted = rendered.clone();
        sorted.sort();
        assert_eq!(rendered, sorted, "sorted ascending by rendered base32");
    }

    /// A record with nine endpoints, a duplicate, an empty element or an
    /// element that does not parse is discarded whole, and the record beside it
    /// still reads.
    #[test]
    fn a_record_that_breaks_a_rule_is_discarded_whole() {
        let good = hint(&[0x11]);
        let nine = hint(&[0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29]);
        let duplicate = hint(&[0x31, 0x31]);
        let empty_element = TxtRecord::from_strings([format!(
            "{TXT_ENDPOINTS_PREFIX}{},,{}",
            spelled(0x41),
            spelled(0x42)
        )]);
        let unparseable =
            TxtRecord::from_strings([format!("{TXT_ENDPOINTS_PREFIX}{},nope", spelled(0x51))]);
        let not_a_point =
            TxtRecord::from_strings([format!("{TXT_ENDPOINTS_PREFIX}{}", render_id(&[0x02; 32]))]);
        let whitespace = TxtRecord::from_strings([format!(
            "{TXT_ENDPOINTS_PREFIX}{}, {}",
            spelled(0x61),
            spelled(0x62)
        )]);

        for bad in [
            nine,
            duplicate,
            empty_element,
            unparseable,
            not_a_point,
            whitespace,
        ] {
            assert_eq!(
                endpoints_at_label(std::slice::from_ref(&bad)),
                Vec::new(),
                "the record is discarded whole"
            );
            assert_eq!(
                endpoints_at_label(&[bad, good.clone()]),
                vec![endpoint(0x11)],
                "the record beside it still reads"
            );
        }
    }

    /// Eight distinct endpoints across two records read; nine read as absent,
    /// and nothing is trimmed to fit.
    #[test]
    fn a_label_naming_more_than_eight_endpoints_reads_as_absent() {
        let four = hint(&[0x11, 0x12, 0x13, 0x14]);
        let another_four = hint(&[0x15, 0x16, 0x17, 0x18]);
        assert_eq!(
            endpoints_at_label(&[four.clone(), another_four.clone()]).len(),
            8
        );

        let ninth = hint(&[0x19]);
        assert_eq!(
            endpoints_at_label(&[four.clone(), another_four.clone(), ninth]),
            Vec::new()
        );

        // A repeat across records is one endpoint, not two, so eight distinct
        // ids in nine slots still read.
        assert_eq!(
            endpoints_at_label(&[four.clone(), another_four, four]).len(),
            8
        );
    }

    #[test]
    fn the_endpoints_prefix_is_matched_case_insensitively_and_other_records_are_ignored() {
        let shouted = TxtRecord::from_strings([format!("MaBeL-EnDpOiNtS={}", spelled(0x11))]);
        assert_eq!(endpoints_at_label(&[shouted]), vec![endpoint(0x11)]);

        let other = TxtRecord::from_strings(["v=spf1 -all"]);
        let claim = TxtRecord::from_strings([format!("mabel={}", identity())]);
        assert_eq!(endpoints_at_label(&[other, claim]), Vec::new());
    }

    /// Row 2 of the applicability matrix: a hostname taken from a ledger's own
    /// claim yields endpoints only when the same response names that identity.
    #[test]
    fn a_claimed_hostname_yields_endpoints_only_when_the_zone_names_that_identity() {
        let hints = hint(&[0x11, 0x12]);
        let claims_alice = TxtRecord::from_strings([format!("mabel={}", identity())]);
        let claims_someone_else = TxtRecord::from_strings([format!("mabel={}", other())]);

        assert_eq!(
            endpoints_for_claim(&[claims_alice.clone(), hints.clone()], identity()).len(),
            2
        );
        // A zone naming other endpoints and not this identity offers this
        // identity nothing, even though the caller row would read them.
        assert_eq!(
            endpoints_for_claim(&[claims_someone_else.clone(), hints.clone()], identity()),
            Vec::new()
        );
        assert_eq!(
            endpoints_for_claim(std::slice::from_ref(&hints), identity()),
            Vec::new(),
            "an endpoints record alone says nothing about who it answers for"
        );
        assert_eq!(
            endpoints_at_label(&[claims_someone_else, hints]).len(),
            2,
            "row 1 reads the same records for the identity the response resolved to"
        );
    }

    /// A zone with an endpoints record and no `mabel=` record is still
    /// `unverified`: the endpoints record never touches the five statuses.
    #[tokio::test]
    async fn an_endpoints_record_alone_leaves_the_claim_unverified() {
        let resolver = StubResolver::new().with_records(NAME, vec![hint(&[0x11, 0x12])]);
        let outcome = verify_hostname(&resolver, HOSTNAME, identity()).await;

        assert_eq!(outcome.status, VerificationStatus::Unverified);
        assert_eq!(outcome.detail, format!("{NAME} holds no mabel= TXT record"));
    }
}
