//! The `Resolver` trait, its `hickory-resolver` implementation and the stub
//! the tests query (proposal 003 section 2).
//!
//! The trait is dyn-compatible: methods return boxed futures, the shape
//! `api::service` already uses, so the verifier holds `&dyn Resolver` and no
//! test reaches the public internet.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use hickory_resolver::TokioResolver;
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::net::{DnsError, NetError};
use hickory_resolver::proto::rr::{RData, RecordType};

/// What a resolver method returns: a boxed future, so [`Resolver`] stays
/// dyn-compatible.
pub type ResolveFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ResolveError>> + Send + 'a>>;

/// Why a lookup produced no answer.
///
/// Every variant is `unreachable` to the verifier (proposal 003 section 2): a
/// name that exists and carries no TXT record is `Ok(vec![])`, not an error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ResolveError {
    /// The query did not answer in time.
    #[error("the query for {name} timed out")]
    Timeout {
        /// The name that was queried.
        name: String,
    },

    /// The resolver refused the query or failed it.
    #[error("the query for {name} failed: {message}")]
    Failed {
        /// The name that was queried.
        name: String,
        /// What the resolver reported.
        message: String,
    },

    /// No resolver could be built from the system configuration.
    #[error("no system resolver: {message}")]
    Unavailable {
        /// What the resolver library reported.
        message: String,
    },
}

/// The character-strings of one TXT resource record.
///
/// Strings are concatenated within one record and never across records
/// (proposal 003 section 2), which is why the record, not the flattened
/// value, is what a lookup returns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TxtRecord {
    strings: Vec<Vec<u8>>,
}

impl TxtRecord {
    /// A record holding these character-strings, in wire order.
    #[must_use]
    pub fn new(strings: Vec<Vec<u8>>) -> Self {
        Self { strings }
    }

    /// A record holding these character-strings.
    #[must_use]
    pub fn from_strings<I, S>(strings: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<[u8]>,
    {
        Self::new(
            strings
                .into_iter()
                .map(|string| string.as_ref().to_vec())
                .collect(),
        )
    }

    /// The character-strings, in wire order.
    #[must_use]
    pub fn strings(&self) -> &[Vec<u8>] {
        &self.strings
    }

    /// The character-strings concatenated with no separator.
    #[must_use]
    pub fn value(&self) -> Vec<u8> {
        self.strings.concat()
    }
}

/// Resolves the records the hostname check reads.
///
/// The verifier calls [`Resolver::lookup_txt`] with an absolute name and only
/// falls back to [`Resolver::lookup_cname`] when the name holds no TXT
/// record, since DNS forbids a CNAME beside other data.
pub trait Resolver: Send + Sync {
    /// TXT records at `name`, one entry per resource record.
    ///
    /// `name` is absolute, root label included. A name that exists and holds
    /// no TXT record answers with an empty vector.
    fn lookup_txt<'a>(&'a self, name: &'a str) -> ResolveFuture<'a, Vec<TxtRecord>>;

    /// The CNAME target at `name`, absolute, or `None` when the name is not
    /// an alias.
    ///
    /// The default answers `None`, which is right for any resolver that
    /// follows the chain itself.
    fn lookup_cname<'a>(&'a self, _name: &'a str) -> ResolveFuture<'a, Option<String>> {
        Box::pin(std::future::ready(Ok(None)))
    }
}

/// A resolver over `hickory-resolver`, configured from the system but with
/// the search list cleared (proposal 003 section 2).
///
/// Clearing `domain` and `search` is what keeps a local suffix off a claimed
/// hostname; the verifier passing an absolute name is the second guard.
pub struct HickoryResolver {
    inner: TokioResolver,
}

impl HickoryResolver {
    /// Builds a resolver from `/etc/resolv.conf` (or the Windows registry)
    /// with no search list.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError::Unavailable`] when the system configuration
    /// cannot be read or the resolver cannot be built.
    pub fn system() -> Result<Self, ResolveError> {
        let (mut config, options) = hickory_resolver::system_conf::read_system_conf()
            .map_err(|error| unavailable(&error))?;
        config.domain = None;
        config.search.clear();
        let inner = TokioResolver::builder_with_config(config, TokioRuntimeProvider::default())
            .with_options(options)
            .build()
            .map_err(|error| unavailable(&error))?;
        Ok(Self { inner })
    }

    /// Wraps a resolver the caller configured.
    #[must_use]
    pub fn with_resolver(inner: TokioResolver) -> Self {
        Self { inner }
    }
}

// Generic because `read_system_conf` returns a different error type per
// platform (a `ProtoError` on macOS, a `NetError` on Linux).
fn unavailable(error: &impl std::fmt::Display) -> ResolveError {
    ResolveError::Unavailable {
        message: error.to_string(),
    }
}

/// Maps a lookup failure, reading "no records" as an empty answer.
fn lookup_result(name: &str, error: NetError) -> Result<(), ResolveError> {
    match error {
        NetError::Dns(DnsError::NoRecordsFound(_)) => Ok(()),
        NetError::Timeout => Err(ResolveError::Timeout {
            name: name.to_owned(),
        }),
        other => Err(ResolveError::Failed {
            name: name.to_owned(),
            message: other.to_string(),
        }),
    }
}

impl Resolver for HickoryResolver {
    fn lookup_txt<'a>(&'a self, name: &'a str) -> ResolveFuture<'a, Vec<TxtRecord>> {
        Box::pin(async move {
            let lookup = match self.inner.txt_lookup(name).await {
                Ok(lookup) => lookup,
                Err(error) => {
                    lookup_result(name, error)?;
                    return Ok(Vec::new());
                }
            };
            Ok(lookup
                .answers()
                .iter()
                .filter_map(|record| match &record.data {
                    RData::TXT(txt) => Some(TxtRecord::from_strings(txt.txt_data.iter())),
                    _ => None,
                })
                .collect())
        })
    }

    fn lookup_cname<'a>(&'a self, name: &'a str) -> ResolveFuture<'a, Option<String>> {
        Box::pin(async move {
            let lookup = match self.inner.lookup(name, RecordType::CNAME).await {
                Ok(lookup) => lookup,
                Err(error) => {
                    lookup_result(name, error)?;
                    return Ok(None);
                }
            };
            Ok(lookup
                .answers()
                .iter()
                .find_map(|record| match &record.data {
                    RData::CNAME(cname) => Some(cname.0.to_string()),
                    _ => None,
                }))
        })
    }
}

/// What a [`StubResolver`] answers for one name.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StubAnswer {
    Txt(Vec<TxtRecord>),
    Cname(String),
    Timeout,
    Failure(String),
}

/// A resolver answering from a table, for tests and for `mabel-cli` dry runs.
///
/// Names are matched exactly, so a test asserting on [`StubResolver::queries`]
/// also asserts that the verifier queried the absolute name.
#[derive(Debug, Clone, Default)]
pub struct StubResolver {
    answers: BTreeMap<String, StubAnswer>,
    queries: Arc<Mutex<Vec<String>>>,
}

impl StubResolver {
    /// A resolver that answers every name with no records.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Answers `name` with these TXT records.
    #[must_use]
    pub fn with_records(mut self, name: &str, records: Vec<TxtRecord>) -> Self {
        self.answers
            .insert(name.to_owned(), StubAnswer::Txt(records));
        self
    }

    /// Answers `name` with one TXT record holding one character-string.
    #[must_use]
    pub fn with_text(self, name: &str, value: &str) -> Self {
        self.with_records(name, vec![TxtRecord::from_strings([value])])
    }

    /// Answers `name` with a CNAME to `target` and no TXT record.
    #[must_use]
    pub fn with_cname(mut self, name: &str, target: &str) -> Self {
        self.answers
            .insert(name.to_owned(), StubAnswer::Cname(target.to_owned()));
        self
    }

    /// Times out every query for `name`.
    #[must_use]
    pub fn with_timeout(mut self, name: &str) -> Self {
        self.answers.insert(name.to_owned(), StubAnswer::Timeout);
        self
    }

    /// Fails every query for `name` with `message`.
    #[must_use]
    pub fn with_failure(mut self, name: &str, message: &str) -> Self {
        self.answers
            .insert(name.to_owned(), StubAnswer::Failure(message.to_owned()));
        self
    }

    /// Every name queried so far, in order, TXT and CNAME alike.
    ///
    /// # Panics
    ///
    /// Panics if another thread panicked while holding the query log.
    #[must_use]
    pub fn queries(&self) -> Vec<String> {
        self.queries.lock().expect("stub query log").clone()
    }

    fn record_query(&self, name: &str) {
        self.queries
            .lock()
            .expect("stub query log")
            .push(name.to_owned());
    }
}

impl Resolver for StubResolver {
    fn lookup_txt<'a>(&'a self, name: &'a str) -> ResolveFuture<'a, Vec<TxtRecord>> {
        self.record_query(name);
        let answer = match self.answers.get(name) {
            Some(StubAnswer::Txt(records)) => Ok(records.clone()),
            Some(StubAnswer::Timeout) => Err(ResolveError::Timeout {
                name: name.to_owned(),
            }),
            Some(StubAnswer::Failure(message)) => Err(ResolveError::Failed {
                name: name.to_owned(),
                message: message.clone(),
            }),
            Some(StubAnswer::Cname(_)) | None => Ok(Vec::new()),
        };
        Box::pin(std::future::ready(answer))
    }

    fn lookup_cname<'a>(&'a self, name: &'a str) -> ResolveFuture<'a, Option<String>> {
        self.record_query(name);
        let answer = match self.answers.get(name) {
            Some(StubAnswer::Cname(target)) => Ok(Some(target.clone())),
            Some(StubAnswer::Timeout) => Err(ResolveError::Timeout {
                name: name.to_owned(),
            }),
            Some(StubAnswer::Failure(message)) => Err(ResolveError::Failed {
                name: name.to_owned(),
                message: message.clone(),
            }),
            Some(StubAnswer::Txt(_)) | None => Ok(None),
        };
        Box::pin(std::future::ready(answer))
    }
}

#[cfg(test)]
mod tests {
    use super::{Resolver, StubResolver, TxtRecord};

    #[test]
    fn a_record_concatenates_its_character_strings_with_no_separator() {
        let record = TxtRecord::from_strings(["mabel=", "abc", "def"]);
        assert_eq!(record.strings().len(), 3);
        assert_eq!(record.value(), b"mabel=abcdef");
        assert_eq!(TxtRecord::default().value(), Vec::<u8>::new());
    }

    #[tokio::test]
    async fn the_stub_answers_from_its_table_and_logs_every_query() {
        let resolver = StubResolver::new()
            .with_text("_mabel.alice.example.", "mabel=one")
            .with_cname("_mabel.bob.example.", "_mabel.carol.example.");

        assert_eq!(
            resolver
                .lookup_txt("_mabel.alice.example.")
                .await
                .expect("txt"),
            vec![TxtRecord::from_strings(["mabel=one"])]
        );
        assert!(
            resolver
                .lookup_txt("_mabel.bob.example.")
                .await
                .expect("txt")
                .is_empty()
        );
        assert_eq!(
            resolver
                .lookup_cname("_mabel.bob.example.")
                .await
                .expect("cname"),
            Some("_mabel.carol.example.".to_owned())
        );
        assert!(
            resolver
                .lookup_cname("_mabel.alice.example.")
                .await
                .expect("cname")
                .is_none()
        );
        assert_eq!(
            resolver.queries(),
            vec![
                "_mabel.alice.example.".to_owned(),
                "_mabel.bob.example.".to_owned(),
                "_mabel.bob.example.".to_owned(),
                "_mabel.alice.example.".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn the_stub_reports_timeouts_and_failures_for_both_record_types() {
        let resolver = StubResolver::new()
            .with_timeout("_mabel.slow.example.")
            .with_failure("_mabel.broken.example.", "SERVFAIL");

        let timeout = resolver
            .lookup_txt("_mabel.slow.example.")
            .await
            .expect_err("timeout");
        assert_eq!(
            timeout.to_string(),
            "the query for _mabel.slow.example. timed out"
        );
        let failure = resolver
            .lookup_cname("_mabel.broken.example.")
            .await
            .expect_err("failure");
        assert_eq!(
            failure.to_string(),
            "the query for _mabel.broken.example. failed: SERVFAIL"
        );
    }
}
