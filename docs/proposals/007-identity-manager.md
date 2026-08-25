# 007: one manager owns the background work

- Date: 2026-08-25
- Status: proposed (revised after dual review by Codex and an independent Opus
  reviewer; 55 findings across the two sets applied in this revision)
- Decisions amended: **018** (no route does network work as a *side effect*,
  and one named component gains scheduled network work; the loopback and
  `--allow-host` half is untouched), **015** (the daily re-verification it asks
  for becomes a schedule the manager owns rather than a side effect of a
  read), **016** (the "periodic background sync can come later" clause is
  taken up for ledger refresh and left alone for the trust graph crawl),
  **017** (the queryable state this adds is an HTTP and CLI surface; eight
  sentences reach the UI and no status screen does)
- Also: supersedes the ticket-042 ruling that no read starts a lookup, by
  moving the lookup out of the read rather than back into it; changes
  `NetLedgerFetcher::read`'s result type so a source's answer is distinguished
  rather than collapsed; adds `status/<identity_id>.json` to the node home
  beside `verification/`, `bindings/` and `contacts/`
- On acceptance this writes **decision 021, "background work is scheduled,
  budgeted and observable"**, holding the four rules that outlive the
  proposal: no route does network work as a side effect, the manager is the
  only component that schedules network work with no person present, every
  automatic contact is over something the operator named, and every attempt is
  recorded as data a person can read back. Decisions 015, 016 and 018 each
  gain a line pointing at 021.

## Context

Everything this node does over the network is started by a person, in the
foreground, once. A node that has been running for a week holds exactly what
it held when somebody last clicked something. A ledger it stores is never
refreshed. A handle it verified a month ago stays a month old until a person
opens the handle screen. A lookup that fails leaves a spinner and then says
nothing about when it might work. And almost nothing records that the attempt
happened: `FetchOutcome.sources_tried` is the only per-attempt record in the
system, it carries no outcome per source, and it is dropped when the call
returns (`crates/mabel-node/src/graph/fetcher.rs:149`).

The owner's ruling: there is one manager component, it is the thing a
developer using this library talks to, it owns the business logic and the
background work for every identity, witness, endpoint and handle this node
knows about, and what it has tried and what it is about to try is data anyone
can read back.

## Proposal

### 1. What exists today

An honest inventory, because the manager orchestrates all of it and replaces
almost none of it. Two earlier drafts of this section got the fetch path and
the handler count wrong; both are corrected here against the tree.

| Piece | Where | What it does now |
|---|---|---|
| `NodeService` | `crates/mabel-node/src/api/service.rs:216` | **29 methods**, and `routes.rs` has **29 handlers**, one per method. Handlers validate, call one method, render. |
| `NodeApiService` | `crates/mabel-node/src/wallet/service.rs:63` | Implements all 29 over `core`, `storage`, `sync`. Holds one piece of scheduling state, `refreshing: Arc<StdMutex<HashSet<IdentityId>>>`. |
| **The fetch route** | `wallet/service.rs:355` | `fetch_identity` plans endpoints, then calls `WalletSync::fetch` (`wallet/sync.rs:449`) **sequentially, one endpoint at a time, and returns the first success**. It does not run `NetLedgerFetcher` and it does not detect equivocation. |
| **The crawl reader** | `graph/fetcher.rs:422` | `NetLedgerFetcher` is a different path: it asks every applicable source, detects equivocation, and keeps candidates in memory. `read` returns `Ok(None)` for an unreachable source and `Err` for one that served a chain that does not verify (`fetcher.rs:593`), so a timeout, a refusal and a `NotFoundResp` are one value today. |
| `Resolution` | `graph/resolve.rs:126` | One per top-level operation: `MAX_DIALS` 16, a deadline of `RESOLVE_BUDGET` (20 s) or `RUN_BUDGET` (60 s), the visited-identity set, per-class caps through `SourceClass::cap` and `reserved`. |
| `FetchSource` | `graph/model.rs:78` | The eight source classes of proposal 006 section 5. Source 8 runs only when sources 1 to 7 produced no reachable copy (`fetcher.rs:546`), taking a hostname from a stale local copy or the crawl generation (`fetcher.rs:574`). |
| `NodeStatus` | `graph/model.rs:225` | `Ok`, `Unreachable`, `Invalid`, `Equivocation`: the outcome of resolving one *ledger* during a crawl. Not per endpoint, which is why section 5.3 adds a different enum rather than reusing this one. |
| `Peers` | `peers.rs:153` | `peers.json`, `BTreeMap<LedgerId, Vec<PeerHint>>`. `MAX_HINTS` 8, `HINT_MAX_AGE_MS` 30 days, `MAX_FAILURES` 3. **An undated legacy hint never ages out until a success stamps it** (`peers.rs:74`). `PeerHint.failures` is written by graph fetching and by push result handling; **a failed HTTP fetch through the route above does not touch it.** |
| `Bindings` | `bindings.rs:82` | `bindings/<identity_id>.json`, the proposal 006 section 4.2 predicate. |
| `VerificationStore` | `verification/cache.rs:148` | `verification/<identity_id>.json`. `FRESH_FOR_MS` 24 h, `is_stale`, `merge` keeping a decisive verdict under an unreachable re-check. `VerificationEntry` already carries `checked_at_ms`, the hostname it is bound to, and the unreachable re-check. |
| `Contacts` | `contacts.rs:25` | `contacts/<identity_id>.json`, the private nickname and note. Named here because it is the fourth per-identity store and the precedent section 5.4 follows. |
| `LedgerStorage` | `storage.rs:308` | One store over one home with an in-memory index. `adopt` (`storage.rs:484`) re-reads a ledger the index has never seen; **a ledger the index knows and another process has extended is not re-read.** |
| `now_ms` | `time.rs:10` | `SystemTime::now()`, called at **30 sites**. There is no clock seam, so `start_paused` cannot move it. |
| `NodeConfig` | `config.rs:98` | `deny_unknown_fields`. Eight keys, no interval, TTL or cap among them. |
| The crawl | `graph/crawl.rs` | Depth 2, `MAX_NODES` 500, `MAX_FETCHES` 300, `IN_FLIGHT` 8, `RUN_BUDGET` 60 s. Three call sites, all started by a person. |
| The UI | `ui/src/hooks/useResource.ts` | `{data, error, loading, reload}`. No react-query, no polling; the only timer in `ui/src` is the copied-flash in `Identifier.tsx:84`. |

**Two GET routes do network work today, by design.** `GET
/api/resolve?input=<hostname>` calls `resolver.lookup_txt` on the request path
(`wallet/service.rs:721`), which is what the wallet search box and story 008
step 5 depend on. `GET /api/witnesses/{identity_id}/holdings` resolves the
identity and calls `sync.list` per endpoint (`wallet/service.rs:820`), which
proposal 006 section 8 specified. Section 4.1 is written around these two
rather than pretending they are not there.

**One shape the manager must not inherit.** `refresh_in_background` removes
its dedupe entry at the end of the spawned task, outside any guard
(`wallet/service.rs:926`); a panic before that line leaks the identity from
the set forever, and no later read can re-check it. Every dedupe and in-flight
set in the manager is cleared by a guard whose `Drop` runs on the panic path.

Two absences worth naming. There is no scheduler, interval or background task
in the crate beyond that one `tokio::spawn`. And there is no record anywhere
of an attempt that failed.

### 2. The gap, in the owner's words

Condensed from the ask; every clause is a requirement this proposal is
measured against.

- On node start, walk the local identities and schedule background work: fetch
  and refresh their ledgers, resolve and cache DNS for their handles.
- Lookup becomes stateful. Check locally; if it is absent, start a fetch and
  answer "I do not know this identity yet, but I am looking it up", so the UI
  can poll every few seconds until the state resolves. On failure, answer
  "could not fetch it, next retry in N seconds", with a manual retry control.
- DNS verification per handle has three user-visible states: not yet
  attempted; attempted and failed with retries remaining; failed with a
  next-retry-at.
- Track per identity: the last sync attempt and the last successful sync of
  its ledger, how many lookup attempts have run, and where we looked.
- Track per endpoint: the last successful and the last failed contact, the
  consecutive failure count, and the attempt counts.
- Minimum and maximum attempt budgets, and backoff, everywhere.
- All of it queryable, so the UI and a developer can see what state everything
  is in.
- An in-memory work queue feeding background workers inside `mabel serve`. The
  manager is what the CLI, the API routes and future embedders call instead of
  poking storage and resolution directly.

### 3. The manager's shape

Decision: **the component is `IdentityManager`, it lives in
`crates/mabel-node/src/manager/`, and it owns the network operations and the
status.** Everything it schedules work about is an identity: a witness is an
identity (proposal 006 section 1), an endpoint is an endpoint an identity
advertises (decision 020), a handle is a hostname an identity claims.
`NodeManager` loses on decision 020, which gives "node" one meaning.

#### 3.1 What the manager owns, and what it does not

The first draft said `NodeApiService` "becomes a renderer". That was not
costed and it was wrong. The manager has 14 methods; `NodeService` has 29.
Making every append route and every pure read pass through the manager means
about 20 pass-through methods that add nothing, or an exposed `core()` that
collapses the encapsulation argument anyway.

Decision: **the manager owns network operations and status; it does not own
appends or pure reads.** Concretely:

| Work | Reaches it through |
|---|---|
| Every fetch, refresh, push, crawl, DNS check, witness resolution | the manager, always |
| Every status read | the manager, always |
| Appends: profile, trust, witnesses, endpoints, memberships, identity create | `NodeApiService` -> `WalletCore`, as today |
| Pure reads: identities, ledger pages, keys, contacts, forks, graph | `NodeApiService` -> `WalletCore` and `LedgerStorage`, as today |

`NodeApiService` keeps its `core` and `storage` handles and gains a `manager`.
The manager holds the same `Arc<WalletCore>` and `Arc<LedgerStorage>`, so
there is one core and one store, not two. **The encapsulation claim is
narrowed to exactly this: network work has one owner.** Appends keep the
authority they already have, which is `WalletCore` and its `AppendLock`.

That narrowing is what makes ticket 044 a real size rather than a mechanical
one, and section 11 scopes it accordingly.

#### 3.2 The public API

Two families, and the split is the whole design: `request_*` returns
immediately with the current state and may enqueue, `*_now` runs inline on the
caller's task under the interactive budget.

```rust
pub struct IdentityManager { /* core, storage, sync, resolver, clock, queue, status */ }

/// The seam that makes section 9's backoff tests writable. `now_ms()` is
/// `SystemTime::now()` at 30 sites and `start_paused` cannot move it.
pub trait Clock: Send + Sync {
    /// Wall clock, for display and for the persisted timestamps.
    fn now_ms(&self) -> u64;
    /// Monotonic, for every scheduling decision.
    fn now_monotonic(&self) -> Instant;
}

pub struct ManagerOptions {
    pub background: bool,
    pub workers: usize,
    pub refresh: Duration,
    pub startup_spread: Duration,
}

impl IdentityManager {
    pub fn new(
        core: Arc<WalletCore>,
        storage: Arc<LedgerStorage>,
        sync: WalletSync,
        clock: Arc<dyn Clock>,
        options: ManagerOptions,
    ) -> Arc<Self>;

    pub fn with_resolver(self: Arc<Self>, resolver: Arc<dyn Resolver>) -> Arc<Self>;
    pub fn with_fetcher(self: Arc<Self>, fetcher: Arc<dyn LedgerFetcher>) -> Arc<Self>;

    /// Loads the status store, then runs the boot walk, then spawns the
    /// workers. Order matters: section 4.2.
    pub fn start(self: &Arc<Self>) -> ManagerHandle;

    // Reads. None of these opens a socket.
    pub fn status(&self, identity: IdentityId) -> IdentityStatus;
    pub fn statuses(&self, page: PageRequest) -> Page<IdentityStatus>;
    pub fn endpoint(&self, endpoint: EndpointId) -> Option<EndpointContact>;
    pub fn endpoints(&self, page: PageRequest) -> Page<EndpointContact>;
    pub fn overview(&self) -> ManagerOverview;

    // Requests. Enqueue and return the state after enqueuing.
    pub fn request_lookup(&self, identity: IdentityId, request: LookupRequestKind) -> IdentityStatus;
    pub fn request_refresh(&self, identity: IdentityId, origin: Origin) -> IdentityStatus;
    pub fn request_check(&self, identity: IdentityId, origin: Origin) -> CheckStatus;

    // Inline interactive operations, on the caller's task.
    pub async fn fetch_now(&self, request: FetchIdentity) -> Result<FetchedLedger, ServiceError>;
    pub async fn check_now(&self, identity: IdentityId) -> Result<VerificationChecked, ServiceError>;
    pub async fn push_now(&self, request: PushRequest) -> Result<Pushed, ServiceError>;
    pub async fn crawl_now(&self) -> Result<GraphSynced, ServiceError>;
    pub async fn resolve_now(&self, input: ResolveInput) -> Result<Resolved, ServiceError>;
    pub async fn holdings_now(&self, identity: IdentityId, page: PageRequest)
        -> Result<WitnessHoldings, ServiceError>;
}
```

`resolve_now` and `holdings_now` are the two GETs of section 1 moved behind
the manager unchanged in behavior, so they draw the same budget and record the
same endpoint contacts as everything else.

Decision: **the manager holds no lock a route can block on.** Status is an
`Arc<RwLock<Status>>`; every read takes the guard, copies and drops. The queue
is a `Mutex<VecDeque>` plus a `Notify`. Nothing does IO under either lock: a
flush clones under the read guard and writes outside it. Every in-flight set
entry is held by a guard whose `Drop` clears it, so the
`refresh_in_background` leak of section 1 cannot recur.

Decision: **the manager does not re-implement resolution, folding or
admission.** It decides when; the existing code decides what.

#### 3.3 One fetch path, and what that costs

Section 1 established that the route and the crawl use different readers
today. The manager cannot have two.

Decision: **`fetch_now` and the fetch worker both use `NetLedgerFetcher`, and
the behavior change that follows is specified rather than absorbed.** Today
the route stores the first chain that verifies and never notices a second,
divergent one. `NetLedgerFetcher` asks every applicable source and reports
equivocation.

The conflict policy, normative:

- when sources serve chains that diverge, the attempt **reports equivocation,
  stores nothing**, and writes the fork record the way every other
  equivocation is recorded, so it stays readable at `GET /api/forks`;
- the status row goes to `exhausted` with `reason: "equivocation"` and
  **schedules no automatic retry**, because retrying cannot resolve a fork;
- `fetch_now` answers code 50, the spelling `WalletSync::fetch` already uses
  for a divergent copy, so the CLI and the route keep their exit code.

This is a real behavior change on the fetch route: a fetch that would have
silently stored one branch now refuses and names the conflict. That is the
correct direction (proposal 001 section 3.7 exists to make equivocation
visible) and it is called out here so nobody discovers it at ticket time.

Decision: **refresh success is defined, and `Local` never counts.** A refresh
attempt succeeds when a **remote** source served a chain that verifies and
either extends the head or confirms it at the same head. A same-head remote
answer is a success and stores zero events: the question a refresh asks is
"is my copy current", and "yes" is an answer. `Local` is still queried, because
it is what the fold compares against, but it can never set
`last_success_ms`: a held copy must not make a refresh look successful after
every remote endpoint has gone dark.

### 4. Lifecycle, scheduling and budgets

#### 4.1 The amendment to decision 018

Decision 018 is about the network boundary of the HTTP surface: a node answers
loopback and nothing else unless an operator names a host. **That half is not
touched.** No rule about `Host`, `Origin`, `--allow-host` or `allowed_hosts`
changes, no route gains authentication, and the node keeps stating what it
accepts at startup.

What decision 018 also came to mean, through ticket 042, is that a read never
causes a dial. Stated that broadly it was already false: `GET /api/resolve`
and `GET /api/witnesses/{id}/holdings` both do network work today. The rule is
therefore restated, not repealed.

**Rule 1, the side-effect rule.** No route does network work *as a side
effect of answering something else*. A route either does no network work at
all, or its entire purpose is one named remote read, and then it is an
**inline interactive operation** under the manager's budget. There are exactly
two of the latter, both pre-existing:

| Route | Why it is inline, not a side effect |
|---|---|
| `GET /api/resolve?input=` | Its whole purpose is one TXT lookup the caller asked for. It still writes nothing and still touches no verification cache. |
| `GET /api/witnesses/{id}/holdings` | Its whole purpose is a live `List` against that witness, as proposal 006 section 8 specified. |

What this forbids, and what changes: `GET /api/identities/{id}` may no longer
spawn a DNS re-check. That is the last side effect in the router and ticket
044 removes it.

**Rule 2, the scheduler.** The manager is the one component that may do
network work with no person present, and only over things the operator named:
identities under `identities/`, witness identities in `node.json.witnesses`
and `node.json.witness_for`, and the endpoints those resolve to.

**Rule 3, source 8 is off for automatic work.** Decision: **a non-interactive
resolution never queries DNS, full stop.** `Resolution` carries an
`interactive: bool` and `dns_sources` is skipped when it is false. Without
this, a background refresh of a witness whose endpoints have all died queries
that witness's zone unprompted, and an automatic retry for a stranger id that
a crawl gave a hostname queries a stranger's zone on a timer, which is
precisely the shape ticket 042 closed. An automatic lookup that runs out of
reachable endpoints fails and says so; finding the identity through DNS is a
thing a person asks for.

**Rule 4, an operator can turn it off.** `node.json` gains `background` and
`mabel serve` gains `--no-background`. With it off the manager still exists,
still answers every read, and still runs the `*_now` operations.

What runs automatically and what stays a click:

| Work | Automatic | Why |
|---|---|---|
| Refresh a ledger this home signs for | yes | The operator holds its keys; the dial goes to witnesses the operator configured. |
| Refresh a witness identity in `node.json.witnesses` or `witness_for` | yes | The operator named it. |
| Check a handle claimed by an identity this home signs for | yes | The operator's own zone; decision 015 asks for a daily check. |
| Re-check a handle whose existing verdict has gone stale, for the same hostname | yes | The zone was already told once. Section 5.2 binds this to the hostname that produced the verdict. |
| Retry a lookup a person started, sources 1, 3 and 4 only | yes | The person asked for that identity. |
| Fetch a ledger this home merely stores | no | Nobody asked, and the set grows with what a person once looked at. |
| Check a handle with no verdict | no | Ticket 042's ruling, kept exactly. |
| Any DNS inside a resolution (source 8) | no | Rule 3. |
| Crawl the trust graph | no | Decision 016. Section 8 keeps it. |

Decision: **the default is on, and the argument is re-stated with rules 2 and
3 in force.** With source 8 suppressed and caller hints consumed (section
4.3), every automatic contact goes to an endpoint reached through `Local`,
`peers.json` or `node.json.witnesses`: endpoints this node already dials on
every push, for identities the operator holds keys for or configured by hand.
The only automatic DNS is `_mabel.<hostname>` for a hostname the operator's
own identity claims, or one that already produced a verdict here. Nothing
automatic reaches a party the operator has not already chosen. What
default-on genuinely discloses is timing and co-residency, and section 10
states that rather than waving at it.

#### 4.2 Start-up

Order is load-first, and it is the fix for a crashloop defeating every
backoff.

1. **Load the status store** (`status/<identity_id>.json`), age out what
   section 5.4 says to, and adopt the rest. A `looking` row from a dead
   process is handled by the rule in 4.5.
2. **Walk.** List `identities/`, `node.json.witnesses` and
   `node.json.witness_for`. For each, compute the deterministic phase below
   and enqueue `RefreshLedger`, plus `CheckHandle` when the folded profile
   claims a hostname.
3. **Requeue persisted work**: every row in `failed`, and every foreign
   identity with a stale verdict that section 4.1 allows re-checking.
   `exhausted` rows are not requeued, because 4.5 makes exhausted terminal.
4. **Due time** for every item is `max(phase offset, persisted
   next_attempt_at_ms)`. A restart cannot pull a six-hour backoff forward to
   sixty seconds, which is what a crashlooping pod did in the first draft.
5. Log one line: identities walked, items queued, the window, and whether
   background work is on.

Decision: **an identity's refresh phase is deterministic, from
`hash(identity_id, node_key_public)`, not a fresh jitter per boot.** The phase
is taken modulo `startup_spread` for the boot offset and modulo `refresh` for
the steady-state slot. Two consequences, both wanted: two pods with different
node keys spread against each other, and one pod restarting twenty times
keeps asking at the same offset instead of re-broadcasting its whole signing
set at a new random moment each time.

Decision: **the walk enqueues and never dials**, so `start` returns before any
network work and a readiness probe stays honest.

Decision: **the boot walk is not the only producer.** Enqueuing also happens
when the thing being scheduled comes into existence:

| Append or change | Enqueues |
|---|---|
| `POST /api/identities` creates an identity | `RefreshLedger` for it |
| A `ProfileUpdate` changes the folded hostname | `CheckHandle`, subject to section 5.2's changed-hostname rule |
| `node.json.witnesses` changes (`mabel witness set-default`) | `RefreshLedger` for the added witness identity |

Without this, creating an identity or claiming a hostname schedules nothing
until the next restart, which on a long-running home is never.

#### 4.3 The queue

Decision: **the queue is in memory and is never written to disk.** Everything
needed to rebuild it is in the status store and `node.json`, and step 3 above
rebuilds it. A queue on disk is a second source of truth for a schedule.

```rust
pub enum Work {
    Lookup { identity: IdentityId },
    RefreshLedger { identity: IdentityId },
    CheckHandle { identity: IdentityId, hostname: String },
}

/// Who asked. This decides both dispatch order and which permits are drawn.
pub enum Origin { Person, Schedule }

pub struct WorkItem {
    pub work: Work,
    pub origin: Origin,
    pub due_at: Instant,
    pub attempt: u32,
    /// Set when an enqueue arrives for a key already in flight.
    pub dirty: bool,
}
```

Decision: **a person-requested item is dispatched before every scheduled item
and draws the interactive permits.** This is the fix for the click that waits
behind fifty due refreshes. The reserved-headroom split of 4.4 is about
background versus interactive; origin ordering is about which queued item goes
first, and the two compose: a pasted link is `Origin::Person`, jumps the due
order, and takes an interactive permit, so it never queues behind scheduled
work at all.

Decision: **coalescing is keyed by `(coalescing kind, identity)`, and the
ledger-writing kinds share one key.** `Lookup` and `RefreshLedger` both fold
and store the same chain, so they coalesce onto one key; `CheckHandle` has its
own. A second enqueue for a key already queued upgrades its origin to `Person`
if the new one is, clears its backoff if so, and does not add an item.

Decision: **an enqueue against an *in-flight* key sets the dirty bit, and a
dirty key re-enqueues once on completion, due now.** Without this, a person
clicking refresh during an in-flight attempt that already read the witness
before their new event existed gets silence and a `held` row that looks
checked.

Decision: **the real guarantee against two workers folding one chain is
`WalletCore`'s `AppendLock`, and the coalescing key is a scheduling
convenience.** The first draft claimed coalescing made concurrent folds
impossible; that was false while `Lookup` and `RefreshLedger` had different
keys, and it would be false again for any inline operation running beside a
worker. `AppendLock` is what actually serializes writers, it exists, and it is
what this proposal relies on.

Decision: **caller hints are consumed by the attempt that receives them and
are never retried.** `Work::Lookup` carries no hints. A `mabel://` link's
endpoints go to that one attempt as `CallerHint` and are then gone; an
automatic retry runs **sources 1, 3 and 4 only**: `Local`, `peers.json` and
`node.json.witnesses`. Otherwise one pasted link naming an attacker's endpoint
buys that attacker roughly 120 liveness confirmations over 30 days, which is
the durable dial target proposal 006 section 5.3 refuses to write to
`peers.json` in the first place. A person who wants the hints tried again
pastes the link again.

#### 4.4 Ticks, workers and shared budgets

The manager ticks every `TICK` of 5 seconds, takes every item whose `due_at`
has passed in origin-then-due order, and starts as many as the permits and the
dial bucket allow. `request_*` also fires the queue's `Notify` when the item
is due now, so a person-requested lookup starts immediately instead of waiting
up to one tick.

| Constant | Value | Why |
|---|---|---|
| `TICK` | 5 s | The finest schedule anything here needs. |
| `WORKERS` | 2, configurable to `BACKGROUND_IN_FLIGHT` | |
| `BACKGROUND_IN_FLIGHT` | 4 | Half the crawl's existing `IN_FLIGHT` of 8. |
| `DIAL_BUCKET` | capacity 64, refill 32 per tick | 6.4 dials per second sustained. |
| `MAX_PENDING` | 256 non-terminal rows | Section 4.5. |
| Per item | `MAX_DIALS` 16, `RESOLVE_BUDGET` 20 s | Unchanged. |

Decision: **four of the eight in-flight slots are reserved for interactive
work.** One `Semaphore` of 8 is shared. Background work acquires a
`BACKGROUND_IN_FLIGHT` permit first and a shared permit second; interactive
work acquires the shared permit only. At most 4 shared permits are ever held
by background work. This is headroom, not priority, and it is the same shape
as proposal 006 section 5.2's `NodeWitness` reservation.

Decision: **`TICK_DIALS` becomes a real token bucket, `DIAL_BUCKET`, drawn by
`Resolution::admit`.** The first draft's per-tick ceiling was unimplementable,
because an item's endpoint set is discovered during resolution, not before.
Moving the draw into `admit`, which is the one function that decides an
endpoint gets dialled, makes it enforceable at the only point where the number
is known. The rules:

- every `admit`, background or interactive, draws one token;
- a **background** resolution that finds the bucket empty stops admitting,
  finishes with what it has, and the item requeues one tick later with no
  attempt charged;
- an **interactive** resolution draws but is never refused; it may take the
  bucket negative to a floor of -32, which background work then pays back by
  waiting.

This is the node's **first aggregate outbound rate limit**. Nothing today
bounds total dials across concurrent operations, and it is worth having on its
own account: it is what caps the drive-by of section 10.

Three costs of the permit model, stated rather than hidden:

- it adds a bound on interactive work that does not exist today. A fifth
  concurrent click waits for a shared permit, up to `RESOLVE_BUDGET`. Four
  concurrent interactive operations is already more than any screen produces.
- `crawl_now` holds one permit and starts up to 8 fetches of its own inside
  `RUN_BUDGET`, so the permit count is a count of operations, not of sockets.
  The dial bucket is what bounds sockets, which is the other reason it is not
  optional.
- a background item that requeues for want of a permit or a token slides its
  published `next_attempt_at_ms` by one tick, so a client countdown can sit at
  the same number for a tick. Section 6.1's `retry_after_seconds` includes the
  tick for exactly this reason.

#### 4.5 Backoff and attempts

Every delay is multiplied by a jitter factor drawn uniformly from 0.75 to
1.25 when it is scheduled. **All scheduling is on monotonic time**; wall clock
is persisted for display only, so an overnight suspend or an NTP step cannot
fire everything in one tick or silence the manager.

A lookup or refresh that fails:

| Attempt | Delay before it | Cumulative |
|---|---|---|
| 1 | 0 | 0 |
| 2 | 15 s | 15 s |
| 3 | 1 min | 1 min 15 s |
| 4 | 5 min | 6 min 15 s |
| 5 | 30 min | 36 min 15 s |
| after 5 | **stop** | |

A handle check that answers `unreachable`:

| Attempt | Delay before it | Cumulative |
|---|---|---|
| 1 | 0 | 0 |
| 2 | 2 min | 2 min |
| 3 | 10 min | 12 min |
| 4 | 1 h | 1 h 12 min |
| 5 | 6 h | 7 h 12 min |
| after 5 | **stop**, until the 24 h refresh of a handle this home signs for | |

Decision: **exhausted is terminal. There is no perpetual backstop.** The first
draft retried every 6 hours forever, which meant a row could never satisfy its
own age-out condition (age-out requires no queued work) and 400 mistyped ids
would dial forever. An exhausted lookup that nobody re-asks for stops, and its
row ages out on schedule. A handle this home signs for is different: it is
config, so its 24-hour refresh resumes, and that is a bounded set.

Decision: **`MAX_PENDING` is 256 non-terminal rows**, counting `looking` and
`failed`. Over the cap, `POST .../fetch` answers 503 with reason
`lookup_queue_full` and enqueues nothing. 256 is `MAX_LIST_LIMIT` in
`mabel-net` and the maximum page on `known`, so every pending row fits in one
page a person can actually read. Rows this home signs for or configures are
exempt and never counted against it.

Decision: **a manual request never resets the attempt count without causing an
attempt.** The reset and the attempt are one transaction: the row goes to
`looking` with `attempts` at 0 and the attempt runs. Otherwise repeated
clicking keeps `exhausted` permanently unreachable.

Decision: **repeated `POST .../fetch` is idempotent.** It never resets
attempts and never clears backoff. A client that times out and re-posts every
few seconds gets the same document and changes nothing. The manual retry is a
distinct act and carries its own field, `{"retry": true}`, which is what the
UI button sends and what resets the count under the rule above.

Decision: **an interrupted `looking` row does not charge an attempt unless it
recorded one.** The attempt is charged at the start today, so a rolling deploy
that stops a node mid-attempt would exhaust identities with zero failed dials.
On load, a `looking` row reverts to its previous state; the attempt counts
only if at least one `Attempted` row was recorded before the stop.

Steady state, when nothing is failing:

| What | Interval |
|---|---|
| A held ledger this home signs for or configures | 15 min, `refresh` in `node.json`, at the deterministic phase of 4.2 |
| A handle with a decisive verdict | 24 h, `FRESH_FOR_MS` |
| A handle whose verdict is `unverified` or `mismatched` | 24 h; a wrong record is worth re-reading and it is the operator's own zone |

#### 4.6 Shutdown

`serve_until` gains one step between axum stopping and the Iroh router
shutting down, because a worker dials through the endpoint that router owns.

1. axum's graceful shutdown completes.
2. `ManagerHandle::shutdown()` closes the queue, cancels the tick task, and
   **cooperatively cancels the workers**: each holds a `CancellationToken`
   checked between sources, and the handle awaits them with a 10 second
   deadline. Past the deadline the tasks are **aborted**, not dropped:
   dropping a join handle detaches a Tokio task, it does not stop it, and a
   detached worker would write a status change after the final flush.
3. The status store is flushed once, synchronously, after cancellation has
   completed.
4. The Iroh router and endpoint shut down as they do today.

`cli/serve.json`'s shutdown document gains the queued and in-flight counts at
the moment the stop began.

#### 4.7 A CLI process beside `mabel serve`

This is real today and this proposal does not fix the underlying cause.
`LedgerStorage`'s index is a cache over directories a second process may write
(`storage.rs:499`), and `adopt` re-reads only a ledger the index has never
seen.

Decision: **two modes, and only serve mode writes the status store.**

- **Serve mode**: the walk runs, the workers run, this process is the single
  writer of `status/`.
- **One-shot mode**, used by every CLI command that touches the network:
  `background: false`, no walk, no workers, no tick. The `*_now` operations
  run inline with the same budgets, documents and exit codes as today. It
  reads `status/` and never writes it.

Decision: **there is one home-wide `peers.json` writer.** `record_hints` is a
whole-file read-modify-write, and handing it to four concurrent workers loses
failure counters, which silently breaks proposal 006 section 5.3's eviction.
In serve mode every write goes through one owning task with a channel;
one-shot mode keeps today's direct write, because it is the only writer in
its process.

The consequence, stated plainly: a `mabel sync fetch` beside a running serve
updates ledger files and `peers.json` and **does not** update the status
store, and the serving node's index may not see the extension either. So the
serving manager can report `failed` for a ledger the CLI just fetched, and can
schedule another fetch for it. Two mitigations and one honest gap:

- every status document carries `as_of_ms`;
- a status read for an identity whose ledger is on disk reports `held: true`
  regardless of the row, because section 5.1 derives `held` from the storage
  index rather than persisting it, and `adopt` covers the case where the index
  had never seen the ledger at all;
- the gap that remains: a ledger the index already knew and the CLI extended
  reads its old head until the serving node restarts. That is the pre-existing
  index limitation, this proposal does not close it, and `mabel status`
  prefers HTTP so that at least the answer comes from the process that owns
  the schedule.

### 5. The state model

#### 5.1 Per identity

Decision: **`held` is derived from the storage index at read time and is never
persisted.** Otherwise `rm -rf status/` makes every held ledger read
`unknown` while the `known` list proves them held. With `held` derived,
`unknown` means only "no attempt recorded here", which is a fact about the
manager rather than a claim about storage.

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LookupState {
    /// No attempt is recorded. Says nothing about whether the ledger is held.
    Unknown,
    /// An attempt is queued or running.
    Looking,
    /// The last attempt succeeded. See 3.3 for what success means.
    Succeeded,
    /// The last attempt failed and another is scheduled.
    Failed,
    /// Attempts are spent, or the attempt hit equivocation. Terminal.
    Exhausted,
}
```

The first draft's `Held` variant is gone: it was the state and the boolean at
once, which is what made the transition table skip the `held: true`
combinations. `held` is now a separate derived boolean on every document, and
`state` is only ever about attempts.

```rust
pub struct IdentityStatus {
    pub identity: IdentityId,
    pub state: LookupState,
    /// Derived from the storage index, never persisted.
    pub held: bool,
    pub attempts: u32,
    pub max_attempts: u32,
    pub total_attempts: u64,
    /// Wall clock, for display. 0 means never, and the document renders null.
    pub first_requested_ms: u64,
    pub last_attempt_ms: u64,
    pub last_success_ms: u64,
    pub next_attempt_at_ms: u64,
    pub reason: Option<String>,
    pub tried: Vec<Attempted>,
}

pub struct Attempted {
    pub source: FetchSource,
    pub outcome: ContactOutcome,
    pub at_ms: u64,
}
```

Decision: **the struct and the document agree, and the document follows
`contracts/README.md`'s nullability rule.** Every key is present; a timestamp
that has not happened is `null` in JSON and `0` in Rust, converted at the
document boundary the way every other document already does it; `tried` is an
empty array, never null. An `unknown` row invented for an identity with no
stored row serializes with the same keys, all nulls and zeros.

Transitions, now total over `held`:

| From | `held` | Event | To |
|---|---|---|---|
| `unknown` | either | a lookup or refresh is enqueued | `looking` |
| `looking` | false | a remote source served a chain that verifies | `succeeded`, `held` becomes true |
| `looking` | true | a remote source confirmed or extended the head | `succeeded` |
| `looking` | either | no remote source answered, `attempts < 5` | `failed` |
| `looking` | either | no remote source answered, `attempts == 5` | `exhausted` |
| `looking` | either | sources served divergent chains | `exhausted`, `reason: "equivocation"`, nothing stored |
| `failed` | either | backoff elapses, or a person asks | `looking` |
| `exhausted` | either | a person asks | `looking`, attempts reset |
| `succeeded` | true | the refresh interval elapses | `looking` |
| any | false | no queued work and no attempt for 30 days | the row is dropped |

Decision: **a success resets `attempts` to 0** and leaves `total_attempts`
climbing. `total_attempts` is the wear counter; `attempts` is the budget.

Decision: **`ContactOutcome` is per endpoint contact and is a different
question from `NodeStatus`.** `NodeStatus` (`graph/model.rs:225`) reports how
resolving one *ledger* ended: `Ok`, `Unreachable`, `Invalid`, `Equivocation`.
This reports what one *endpoint* said.

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactOutcome {
    Served,
    /// It answered `NotFoundResp`. A transport success.
    NotFound,
    /// It served something that does not verify.
    Invalid,
    TimedOut,
    Unreachable,
    /// Planned, and the budget or the dial bucket ran out first.
    NotDialled,
}
```

Decision: **`NetLedgerFetcher::read` changes its result type.** Today it
collapses a timeout, a refusal and a `NotFoundResp` into `Ok(None)`
(`fetcher.rs:593`), so these outcomes cannot be produced without the change.
It returns an enum carrying the same three cases distinctly. `sources_tried`
stays as it is and is not retyped: four assertions in
`tests/resolution.rs` and the caller-hint filter in `record_hints` read it,
and "what was asked" is a different question from "what each said".

Decision: **`MAX_TRIED` is 17, and `NotDialled` rows are recorded
explicitly.** 16 is `MAX_DIALS`, which counts endpoints, and `Local` is a free
source that also appears, so one attempt can produce 17 rows. And because the
existing loop records a source only when it is actually asked, a planned
source skipped after the deadline never reaches `sources_tried` at all; the
manager records those rows itself from the plan, which is the only way
`NotDialled` can ever appear.

#### 5.2 Per handle

Decision: **`CheckState` is derived from `VerificationEntry`, and the manager
persists only `attempts` and `next_check_at_ms`.** `VerificationEntry`
already carries `checked_at_ms`, the bound hostname and the unreachable
re-check, which is three of the four values. Deriving removes the overlap the
first draft had between "never checked" and "unreachable", and it keeps the
`never_checked`/`unchecked` pairing true after `rm -rf status/`.

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    /// No `VerificationEntry` bound to the claimed hostname. Always paired
    /// with the `unchecked` verdict of ticket 042.
    NeverChecked,
    /// An entry exists and the last check produced a verdict.
    Checked,
    /// An entry exists, the last check was unreachable, attempts remain.
    Retrying,
    /// Attempts are spent. Terminal for a foreign handle; a handle this home
    /// signs for resumes at the 24 h refresh.
    Exhausted,
}
```

The owner named three user-visible states; `Checked` is the fourth because a
handle that verified an hour ago has to be something. The three that get copy
are `NeverChecked`, `Retrying` and `Exhausted`.

| From | Event | To |
|---|---|---|
| `never_checked` | a check answers anything but unreachable | `checked` |
| `never_checked` | a check answers unreachable | `retrying` |
| `checked` | `FRESH_FOR_MS` elapses and 4.1 allows a refresh | a check runs |
| `checked` | a check answers unreachable | `retrying`; `merge` keeps the decisive verdict |
| `retrying` | a check answers anything but unreachable | `checked`, attempts reset |
| `retrying` | unreachable, `attempts == 5` | `exhausted` |
| `exhausted` | a person clicks the check control | `checked` or `retrying`, attempts reset |
| any | the profile stops claiming a hostname | the row is dropped; the verdict reads `unclaimed` |
| any | the profile claims a **different** hostname | reset to `never_checked`; no automatic check |

Decision: **an automatic re-check runs only for the hostname that produced the
existing verdict; a changed hostname resets to `never_checked` and needs a
click.** Without this the stale-verdict refresh is a DNS steering channel: a
ledger pushed to a witness appends a `ProfileUpdate` claiming
`<victim-nonce>.attacker.example`, and the automatic re-check tells the
attacker's nameserver which node holds that ledger, repeatable once per
rotation. Section 9 pins this with a test.

#### 5.3 Per endpoint

```rust
pub struct EndpointContact {
    pub endpoint: EndpointId,
    pub first_seen_ms: u64,
    pub last_success_ms: u64,
    pub last_failure_ms: u64,
    /// Incremented only by TimedOut and Unreachable.
    pub consecutive_failures: u32,
    pub attempts: u64,
    pub successes: u64,
}
```

Decision: **this table is keyed by endpoint and is not part of
`peers.json`.** They answer different questions. `peers.json` answers "which
endpoints should I dial for ledger L", is per ledger, and keeps the cap,
age-out and three-failure eviction proposal 006 section 5.3 gave it. This
answers "how has this endpoint behaved", is per endpoint across every ledger,
and an endpoint answering for a witness that serves forty ledgers has one row
here and forty hints there.

Decision: **only `TimedOut` and `Unreachable` increment
`consecutive_failures`.** A `NotFound` is a transport success: the endpoint
answered honestly that it holds nothing. Counting it would demote an honest
witness that simply does not keep the ledger being asked for.

Decision: **the ranking is: at least one success ahead of none, then fewer
consecutive failures, then more recent success.** Ranking on failures alone
puts a never-tried endpoint ahead of a proven one that failed once, which is
backwards.

Decision: **two failure counters exist and they will diverge; say which is
which.** `PeerHint.failures` is the one proposal 006 section 5.3's eviction
reads, it is per ledger, and this proposal does not touch it.
`EndpointContact.consecutive_failures` is per endpoint, orders dials, and
evicts nothing. They diverge because they count different events over
different scopes, and that is intended.

Decision: **ordering is a selection rule, so section 8's "changes no cap and
no rule" is narrowed.** Within a source class, under proposal 006's unchanged
per-class caps, this decides *which* endpoints get the slots. It changes no
cap, no class and no source order; it changes the order inside a class. That
is a real change to resolution behavior and it gets its own ticket.

Decision: **every network operation updates this table, not only fetches.**
Fetch, refresh, push, `holdings_now`, and witness resolution all record a
contact. Decision 021 promises every attempt is recorded; recording only
fetches would leave an endpoint that accepts ten pushes and serves no fetch
reading zero successes.

#### 5.4 On disk

Decision: **status is per-identity files, `status/<identity_id>.json`,
matching `verification/`, `bindings/` and `contacts/`.** One
`identities.json` meant a witness holding 10000 ledgers rewrote a
multi-megabyte file every 10 seconds. Endpoint contacts stay in one file,
`status/endpoints.json`, because they are keyed by endpoint, are capped at
512, and are read as a set.

Decision: **a ledger that arrives by push creates no status row.** A witness
admits ledgers it never asked for; a row per admitted ledger is bookkeeping
about work that never happened. Rows exist for identities this home signs
for, configures, or has attempted a lookup or refresh on.

| Rule | Value |
|---|---|
| Flush | at most every 10 s while dirty, and once after cancellation at shutdown |
| Identity row age-out | 30 days with no attempt, no queued work, and not signed for or configured |
| Non-terminal rows | `MAX_PENDING` 256, section 4.5 |
| Endpoint row age-out | 30 days with no contact, reusing `HINT_MAX_AGE_MS` |
| Endpoint row cap | 512, evicting on `max(first_seen_ms, last_success_ms, last_failure_ms)`, tie-broken by ascending endpoint id |
| Endpoint eviction exemption | endpoints in `node.json.witnesses` and in this home's own advertisements |
| `tried` rows | `MAX_TRIED` 17, most recent attempt only |

Retention includes `first_seen_ms` because a newly planned endpoint has zero
in both other fields and would otherwise look oldest and be evicted
immediately. The exemption exists because adversarial endpoint churn would
otherwise evict exactly the ordering memory that matters.

Decision: **recovery is per file and never cross-file.** Each file is written
through the existing `write_atomic`, so no file is ever partial. A malformed
or missing file loads as empty and logs one line. Because status rows are
per identity, a crash can leave one identity's row older than another's, and
that is harmless: each row is independently valid and each is a cache. The
one cross-file relationship, an identity row citing an endpoint with no
contact row, is read as "no contact recorded" and repairs on the next
contact. The dirty bit is cleared **after** a successful write, never before,
so a failed flush retries on the next cycle.

Deliberately in memory only, rebuilt every start: the queue, the in-flight
set, the permits, the dial bucket, the jitter draws, and the `tried` rows of a
running attempt.

### 6. API and CLI

#### 6.1 The async lookup contract

Decision: **`POST /api/identities/{identity_id}/fetch` answers 202 and a
status document.** The body keeps `from` and `from_witness` unchanged and
gains one optional key, `retry`, default false.

```json
{
  "ok": true,
  "identity_id": "<id>",
  "state": "looking",
  "held": false,
  "attempts": 1,
  "max_attempts": 5,
  "total_attempts": 1,
  "automatic": true,
  "retry_after_seconds": 8,
  "first_requested_ms": 1756000000000,
  "last_attempt_ms": 1756000000000,
  "last_success_ms": null,
  "next_attempt_at_ms": null,
  "reason": null,
  "tried": []
}
```

Decision: **`retry_after_seconds` says when this document may change on its
own, and it includes the dispatch tick.** It is
`ceil((next_attempt_at - now) / 1000) + TICK` seconds. Omitting the tick let a
client sleep exactly that long, see no change and stop.

Decision: **the table is total, and `automatic` covers the case
`retry_after_seconds` cannot express.** `null` already means "nothing more
will happen on its own", and that is true both when the work is done and when
background work is off. `automatic: false` distinguishes them: it is false
when the manager is in one-shot mode or background work is off, and false for
a terminal state.

| `state` | `automatic` | `retry_after_seconds` | Client |
|---|---|---|---|
| `unknown` | false | `null` | Nothing is scheduled. Offer the fetch control. |
| `looking` | true | 3 | Poll. |
| `succeeded` | true | seconds to the next refresh, or `null` if none | Read the identity. Keep polling only if `held` is false. |
| `failed` | true | seconds to the next attempt, plus `TICK` | Keep polling at that interval. |
| `failed` | false | `null` | Background is off. Show the retry control. |
| `exhausted` | false | `null` | Terminal. Show the retry control. |

Decision: **the read route is `GET /api/identities/{identity_id}/status`, and
the word is `status`, not `lookup`.** `GET /api/lookup/{identity_id}` already
answers the trust graph question, and two routes named lookup is what decision
012 forbids.

`GET /api/identities/{identity_id}` gains `status`, the same object, and three
keys in the existing `verification` object: `check_state`, `check_attempts`
and `next_check_at_ms`. It still answers **404** for a ledger this home does
not hold: a status document is not an identity document.

`GET /api/identities/known` rows gain `lookup_state`, one string.

#### 6.2 The manager's own state

```
GET /api/status
{"ok": true, "as_of_ms": 0, "background": true, "workers": 2,
 "queued": 3, "in_flight": 1, "next_due_at_ms": 1756000000000,
 "identities": {"unknown": 0, "looking": 1, "succeeded": 12, "failed": 2, "exhausted": 0},
 "handles": {"never_checked": 3, "checked": 8, "retrying": 1, "exhausted": 0},
 "endpoints_tracked": 19, "last_flush_ms": 1756000000000}

GET /api/status/identities?offset&limit
{"ok": true, "identities": [ ... ], "offset": 0, "limit": 100, "more": false}

GET /api/status/endpoints?offset&limit
{"ok": true, "endpoints": [ ... ], "offset": 0, "limit": 100, "more": false}
```

The top-level key is the plural noun and the value is always an array, which
is the convention every existing list document follows. Paging matches
`known`: default 100, maximum 256, sorted by ascending id.

Decision: **`GET /api/status/identities` enumerates only identities this home
signs for, configures, or holds.** Enumerating every id anyone ever typed
would re-expose exactly the set proposal 006 section 8 narrowed `List` to
hide, unauthenticated, for 30 days. A stranger's failed-lookup row stays
readable by id at `GET /api/identities/{id}/status`, because a caller who can
name the id already knows it.

Every `/api` answer already carries `Cache-Control: no-store` from the
middleware ticket 043 added, so these inherit the right cache rule with no
new work.

#### 6.3 CLI

`mabel lookup` does not change: it is the trust graph question.

`mabel sync fetch` keeps its document, its cases and its exit codes. A CLI
process is a one-shot manager and runs `fetch_now`. What does change is the
equivocation case of 3.3, which now answers code 50 instead of silently
storing one branch.

`mabel serve` gains `--no-background`.

`mabel status` is new:

```
mabel status                         the overview, plus a line per identity not in succeeded
mabel status --identity <alias|id>   one identity: state, attempts, where we looked
mabel status --endpoints             the endpoint contact table
mabel status --json                  the documents above
```

It reads `GET /api/status` when `node.json`'s `http_bind` answers, and
`status/` off disk otherwise, saying which in its document. Outside `--json`
every identity id reads `mabel://<id>` and every endpoint id stays bare under
its own label (decision 019).

#### 6.4 `node.json`

```json
"background": {"enabled": true, "workers": 2, "refresh_seconds": 900,
               "startup_spread_seconds": 60}
```

Decision: **the forward and downgrade rule is stated the way proposal 006
stated it for `witness_for`.** `NodeConfig` sets `deny_unknown_fields`, so the
key is `#[serde(default)]` and an existing `node.json` without it loads
unchanged. A file **with** it fails to load on a binary from before this
change, which is the same downgrade cost every added key has had; it is
recorded here so an operator who rolls back knows to delete the key.
Validation refuses `workers` of 0 or above `BACKGROUND_IN_FLIGHT`,
`refresh_seconds` below 60, and `startup_spread_seconds` above 3600, the way
every other bad value in that file is a load error. `docker/entrypoint.sh`
and the seeded homes write the key explicitly so the compose topology
exercises it.

#### 6.5 Fixtures

Each route ticket carries its own fixture rows, because `api/tests.rs`
compares fixtures key for key and a route landing without its fixture breaks
CI immediately.

| Fixture | Class | Lands in |
|---|---|---|
| `http/wallet-get-identity-status.json` | new | 046 |
| `http/wallet-post-identity-fetch.json` | changed | 046 |
| `http/wallet-get-identity.json` | changed | 046 and 047 |
| `http/wallet-get-known-identities.json` | changed | 046 |
| `http/node-get-status.json` | new | 048 |
| `http/node-get-status-identities.json` | new | 048 |
| `http/node-get-status-endpoints.json` | new | 048 |
| `cli/status.json` | new | 048 |
| `cli/serve.json` | changed | 044 |
| `cli/sync-fetch.json` | changed | 046, the equivocation case |
| `contracts/README.md` | changed | 049 |

### 7. The UI

Three changes and no fourth. Decision 017 forbids a developer mode, so the
manager's tables stay on the HTTP and CLI surfaces: **there is no status
screen and no nav entry.** Eight sentences reach a person, across two screens.

#### 7.1 Polling

```ts
export function usePolledResource<T>(
  load: () => Promise<T>,
  deps: unknown[],
  nextPollSeconds: (data: T) => number | null,
): Resource<T>
```

Decision: **polling continues while the record is not held, at the interval
the document names.** `nextPollSeconds` returns
`data.held ? null : data.retry_after_seconds`. The first draft stopped at
`failed`, so an automatic retry that succeeded fifteen seconds later left the
page showing a stale failure until reload. Polling at
`retry_after_seconds` follows the manager's own schedule, so a page waiting
through a 30-minute backoff asks twice, not six hundred times.

It stops when `document.visibilityState` is not `visible` and resumes when it
is, and it clears its timer on unmount. There is no separate five-minute stop:
`null` from `nextPollSeconds` is the one termination rule.

#### 7.2 The identity page

`identity-fetch` keeps its testid, title and `identity-fetch-link-note`.
After the click the section is replaced by `identity-status`, polled.

| Testid | Copy |
|---|---|
| `identity-status-looking` | `Looking for this record now. It will appear here as soon as it arrives.` |
| `identity-status-failed` | `We could not find this record. We will try again in 4 minutes.` |
| `identity-status-exhausted` | `We could not find this record after 5 tries. Use the button to try again.` |
| `identity-status-conflict` | `Two endpoints gave us different versions of this record, so we kept neither.` |
| `identity-status-tried` | `We asked 3 endpoints and none of them had it.` |
| `identity-status-retry` | button, `Try again now` |

A refresh of a record already held never hides it: the record draws and one
line sits above the ledger, `identity-status-refreshing`, reading `Checking
for newer entries.` A failed refresh of a held record reads
`identity-status-refresh-failed`, `We could not check for newer entries. We
will try again in 4 minutes.`, which is why the `held: true` transitions of
5.1 need their own copy rather than borrowing the not-found sentences.

The countdown is rounded to whole minutes above 90 seconds and whole seconds
below, and it is a sentence rather than a ticking timer.

#### 7.3 The handle screen

`verification-panel` keeps its five testids and gains
`verification-next-check`:

| `check_state` | Copy |
|---|---|
| `never_checked` | `This handle has not been checked from this wallet yet.` (ticket 042's sentence, unchanged) |
| `retrying` | `The last check could not reach this handle. We will try again in 10 minutes.` |
| `exhausted` | `The last 5 checks could not reach this handle. Use the button to check again.` |

`checked` draws nothing new.

The mock store gains one exported function per new route, one `http.get` per
route in `handlers.ts`, and a case in `ui/src/test/mock-routes.test.ts`.

### 8. What does not change

- **The fold.** No payload tag, descriptor, field rule or accessor. Every test
  vector keeps its bytes.
- **Admission.** Proposal 006 section 4's four clauses, the `witness_for`
  gate, the legacy clause and the advertisement invariant. A refresh is a
  `Get`, and a `Get` admits nothing.
- **Resolution structure.** The eight `FetchSource` classes, their order,
  `SourceClass::cap` and `reserved`, `MAX_DIALS` 16, `RESOLVE_BUDGET` 20 s,
  `PER_FETCH_TIMEOUT` 5 s, the visited-identity set, the applicability matrix.
  Three things do change and are named rather than buried: source 8 is
  suppressed for non-interactive resolutions (4.1), `admit` draws a dial token
  (4.4), and endpoint order within a class follows 5.3.
- **`peers.json`.** Shape, `MAX_HINTS` 8, the 30-day age-out with its undated
  exception, and the three-failure eviction on `PeerHint.failures`.
- **Bindings.** The 4.2 predicate and its head-seq rules.
- **The security invariants.** Decision 018's loopback rules, the `Host` and
  `Origin` sets, `--allow-host`, and the absence of authentication. Nothing
  binds a new socket or widens a host set.
- **`List` narrowing**, and with it what a dialler can enumerate.
- **The trust graph crawl.** Decision 016 keeps it manual and `graph-sync` on
  `/witnesses` stays the one control.
- **`sources_tried`** and the four order-sensitive assertions that read it.

### 9. Testing

**Unit, `crates/mabel-node/src/manager/tests.rs`**, over an injected `Clock`
and `StubFetcher`/`StubResolver`, so nothing reaches a socket and the tables
are actually assertable.

- The two backoff tables attempt by attempt, jitter inside 0.75 to 1.25.
- Exhausted is terminal: no item is queued after attempt 5.
- Two enqueues of one key make one item; an in-flight enqueue sets dirty and
  re-enqueues once on completion.
- `Lookup` and `RefreshLedger` for one identity share a key and never run
  together.
- Caller hints are absent from a retry: the second attempt plans sources 1, 3
  and 4 only.
- A non-interactive resolution plans no source 8.
- A manual request resets attempts only together with an attempt.
- Repeated `POST fetch` without `retry` changes nothing.
- The reservation: with 4 background permits held, an inline `fetch_now`
  acquires a shared permit without waiting.
- The dial bucket: a background resolution stops admitting at empty and
  requeues with no attempt charged; an interactive one is never refused.
- `held` derives from the index: with `status/` deleted, a held ledger reads
  `held: true` and `state: unknown`.
- Boot: status loads first, and `due_at` is `max(phase, persisted)`.
- The deterministic phase is stable across restarts and differs across node
  keys.
- An interrupted `looking` row reverts and charges no attempt with no
  `Attempted` row recorded.
- Equivocation goes to `exhausted` with `reason: "equivocation"` and stores
  nothing.
- Refresh success: a same-head remote answer sets `last_success_ms`; a
  `Local`-only outcome does not.
- Age-out, both caps, the endpoint exemption, and `first_seen_ms` in the
  retention key.
- Only `TimedOut` and `Unreachable` increment `consecutive_failures`.
- **The changed-hostname rule**: a profile that changes its hostname resets to
  `never_checked` and no automatic check runs for the new name.

**Route level, `crates/mabel-node/tests/node_routes.rs`**, against the frozen
fixtures.

- The new routes match their fixtures, key for key.
- `POST .../fetch` answers 202; a repeat is idempotent; `{"retry": true}`
  resets.
- **No GET does network work as a side effect.** A counting resolver and
  counting fetch client, then every GET called once against an identity with a
  claimed hostname and no verdict. The two inline routes of 4.1 are excluded
  by name and asserted separately to *do* their one operation; every other GET
  leaves both counters at zero. This pins the side-effect claim rather than
  the false absolute the first draft asserted.
- `background: false` runs no walk and leaves the counters at zero for three
  ticks.
- `GET /api/status/identities` omits a stranger's row while
  `GET /api/identities/{stranger}/status` answers it.

**End to end.** `docs/stories/010-a-lookup-that-waits.md` and its spec, on the
compose topology plus one hand-started home. Bring the topology up, push bob's
identity, stop every witness, start a home that knows nobody, paste bob's link
with no endpoints, click `identity-fetch-button`, assert
`identity-status-looking` then `identity-status-failed` with a `tried` array
whose rows carry a source and a non-`served` outcome, restart the witness,
click `identity-status-retry`, and assert the record arrives with `attempts`
back at 1.

Story 008 gains one wait and keeps its assertions. Story 007 gains one
assertion, that a handle nobody checked reads `never_checked`. Story 009 and
`docker/smoke.sh` both post to the fetch route and read the synchronous body
(`docker/smoke.sh:125` reads `{ledger_id, source, event_count, stored,
head_seq}`; `tests/e2e/specs/009-endpoint-rotation.spec.ts` posts at lines 201
and 268 and asserts `status === 200` at line 204). Both move to a
poll-until-held helper in ticket 046, which is where the 202 lands.

### 10. Impact and risks

**A container fleet on boot.** A home with N local identities and W configured
witnesses queues up to N + W refreshes **and** up to N handle checks, so 50
identities is up to 100 items, not 50. The default is **2 workers**, not 4. If
every fetch ran the full 20 second budget, 50 refreshes at 2 workers is about
8 minutes, plus DNS checks that are far shorter. In practice witnesses answer
in well under a second and the pass finishes in seconds. The HTTP listener
answers throughout, because the walk enqueues and does not dial. The first
draft's "about 4 minutes" used the wrong worker count and the wrong item
count.

**Thundering herd.** Four defences. The deterministic phase from
`hash(identity_id, node_key)` spreads identities within a node and spreads
nodes against each other, and unlike fresh jitter it survives a restart. The
0.75 to 1.25 jitter on retries diverges them further. The dial bucket caps one
node at 6.4 dials per second whatever its identity count. And steady-state
refreshes use the same deterministic phase, so the 15-minute pass is spread
rather than clustered, which the first draft left unstated. What is not
defended: 200 pods sharing one witness send it 200 refreshes per refresh
interval, which at 15 minutes is 0.22 requests per second. It is linear in
pods and `refresh_seconds` is the knob.

**A drive-by, with dials counted.** `POST .../fetch` is unauthenticated on an
exposed node and now returns immediately, so enqueuing is cheap. The first
draft counted attempts as dials and understated the cost about sixteenfold.
With the bounds of this revision: `MAX_PENDING` caps non-terminal rows at 256,
so 10000 posted ids produce 256 rows and 9744 immediate `lookup_queue_full`
refusals. Those 256 rows cost at most 256 x 5 attempts x 16 dials = 20480
dials, and the dial bucket meters them at 6.4 per second, so the burst is
about 53 minutes of metered traffic and then it **stops**, because exhausted
is terminal. Legitimate refreshes are delayed, not starved: they are
`Origin::Schedule` like the drive-by, but rows for identities this home signs
for or configures are exempt from `MAX_PENDING`, so they always enqueue.

**Co-residency, stated honestly.** Every automatic refresh tells the witnesses
it asks that this home is up and which identity it wants. A boot burst
therefore shows a witness the **set of identities co-resident on one node**,
plus a restart timestamp, and a home with no published endpoint now emits that
set every refresh interval rather than only when somebody clicks. The
deterministic phase spreads the set over the interval instead of delivering it
in one burst, which blurs the co-residency signal without removing it. The
parties who learn it are witnesses the operator configured. An operator who
does not want it runs `--no-background`, and the flag is documented beside
`--allow-host` for that reason.

**The resolver leak narrows.** With source 8 suppressed for non-interactive
work and automatic checks bound to the hostname that produced the verdict, the
only automatic DNS is `_mabel.<hostname>` for a handle this home signs for or
one already checked here. That is fewer queries than the tree emitted before
ticket 042, not more.

**The 202 and the fetch behavior change.** Two breakages, both scoped: every
client of the synchronous body (the UI, the e2e suite, `docker/smoke.sh`,
story 009) moves in ticket 046, and the equivocation policy of 3.3 makes the
route refuse where it used to store one branch. There is no external consumer
and the POC has no compatibility promise on HTTP documents.

**Two failure counters.** `PeerHint.failures` and
`EndpointContact.consecutive_failures` will diverge, by design, and a reader
comparing them will find different numbers. Section 5.3 says which one each
rule reads.

**What this proposal does not fix.** The `LedgerStorage` index staleness when
a CLI process extends a ledger the serving node already indexed. Section 4.7
states the residual gap rather than claiming the status store closes it.

### 11. Ticket cut

Nine tickets. Every one goes to a branch and opens a pull request for owner
review; none of this is pushed to `main` directly.

| Ticket | Scope | Depends on |
|---|---|---|
| 044 the manager, its state and the clock seam | `manager/` with the queue, origin ordering, coalescing and dirty bit, workers, tick, both backoff tables, the shared semaphore and the dial bucket in `Resolution::admit`; the injected `Clock` and the monotonic scheduling; the status types of section 5 and `status/<identity_id>.json`; the boot walk with status-first load and the deterministic phase; **removal of the identity route's DNS side effect**; the no-GET-side-effect test; `node.json.background` with its validation and the entrypoint and seeded homes; `--no-background`; cooperative worker cancellation; the one home-wide `peers.json` writer; `cli/serve.json`. | nothing |
| 045 one fetch path and the equivocation policy | `fetch_now` and the worker both on `NetLedgerFetcher`; the `read` result-type change so outcomes are distinct; the conflict policy of 3.3 and the code 50 answer; the refresh success rule; `cli/sync-fetch.json`'s equivocation case. | 044 |
| 046 endpoint contacts and dial ordering | `EndpointContact`, `status/endpoints.json`, all-operation accounting including push, holdings and witness resolution, retention with `first_seen_ms` and the exemptions, the ranking rule, and the churn it causes in the four order-sensitive assertions in `tests/resolution.rs`. | 045 |
| 047 the async lookup route | `POST .../fetch` answers 202 with `retry`; `GET /api/identities/{id}/status`; `status` on the identity document; `lookup_state` on `known` rows; `MAX_PENDING` and `lookup_queue_full`; **its own fixture rows**; `docker/smoke.sh` and story 009 moved to a poll-until-held helper. | 046 |
| 048 the handle schedule | `CheckState` derived from `VerificationEntry`; the retry table; the changed-hostname rule and its test; the append paths that enqueue; its own fixture rows. **Shares the identity-document handler and fixture with 047**, so it lands after it and rebases on it. | 047 |
| 049 the status routes and `mabel status` | `GET /api/status`, `/api/status/identities` with the narrowed enumeration, `/api/status/endpoints`; the CLI command with its HTTP-first rule; the one-shot manager migration for every CLI network command; status-store recovery; its own fixture rows. | 048 |
| 050 contracts | `contracts/README.md`: the index rows, the five lookup states, the four check states, the side-effect rule, and the two inline GETs named. Only the genuinely new status fixtures that no earlier ticket needed. | 049 |
| 051 the UI | `usePolledResource`, the `identity-status` section and its six strings, the refreshing and refresh-failed lines, `verification-next-check` and its three sentences, the mock store and the UI tests. | 050 |
| 052 the story | `docs/stories/010-a-lookup-that-waits.md` and its spec, the assertions added to stories 007 and 008, and the `docs/stories/README.md` paragraph for the new testids. | 051 |

Why this shape. 044 and the old 045 are merged because backoff without an
attempt counter is not a scheduler, and because the clock seam has to exist
before any of it is testable. The identity route's DNS side effect is removed
in the same ticket as the test that forbids it, so 044 can pass its own
acceptance. Each route ticket carries its own fixture rows, because
`api/tests.rs` compares key for key and a deferred fixture ticket breaks CI on
the route ticket. 050 keeps only `contracts/README.md` and what nothing else
needed. 047 and 048 both edit the identity document and the same handler, so
the dependency is stated rather than discovered in a conflict. 046 is its own
ticket because endpoint ordering is a selection rule that churns
order-sensitive assertions, and mixing that into a persistence ticket hides
it.

## Alternatives considered

- **Putting the schedule inside `NodeApiService`.** The smallest diff, and it
  makes the schedule unreachable to an embedder who does not want 29 HTTP
  methods.
- **Making the manager own all 29 methods.** About 20 pass-throughs that add
  nothing, or an exposed `core()` that gives up the encapsulation the
  pass-throughs were for. Section 3.1 narrows the claim to network work
  instead.
- **A `Manager` trait with one implementation.** `with_fetcher`,
  `with_resolver` and the injected `Clock` already give tests their seams.
- **Persisting the work queue.** A second source of truth for a schedule, and
  a crash leaves items whose backoff clock stopped. The boot walk rebuilds it.
- **Keeping caller hints on retries.** It is what a naive "remember what the
  user gave us" does, and it turns one pasted link into a durable dial target
  the endpoint's owner never earned.
- **Leaving source 8 available to background work.** It is the recovery path a
  rotation needs, and on a timer it queries strangers' zones unprompted, which
  is the leak ticket 042 closed. It stays available to interactive work, which
  is where recovery actually happens.
- **A perpetual 6-hour backstop for exhausted lookups.** It sounds like
  persistence and it means a row can never satisfy its own age-out, so 400
  mistyped ids dial forever.
- **Folding the per-endpoint counters into `PeerHint`.** Forty copies of one
  endpoint's history, and an eviction rule reading counters that mean
  something different from the ones it evicts on.
- **One `status/identities.json`.** A witness with 10000 ledgers rewriting
  megabytes every 10 seconds, against four neighbouring stores that are all
  per identity.
- **Persisting `held`.** `rm -rf status/` then makes every held ledger read
  `unknown` while `known` proves otherwise.
- **A `Held` state instead of a derived boolean.** It is the state and the
  fact at once, which is what made the first draft's transition table skip
  every `held: true` row.
- **Storing `CheckState` outright.** `VerificationEntry` already holds three
  of its four values, and storing it separately let "never checked" and
  "unreachable" both be true.
- **Naming the route `/api/identities/{id}/lookup`.** It collides with `GET
  /api/lookup/{id}`, the trust graph route.
- **Keeping `POST .../fetch` synchronous and adding a second async route.**
  Two ways to fetch, and the synchronous one still has no state to report when
  it fails.
- **A long poll or a websocket.** A long poll ties a connection to a work item
  for the whole budget and makes shutdown wait on readers; a websocket is a
  second transport under decision 018's no-authentication reality.
- **Priority queues instead of reserved headroom.** Headroom answers "does a
  click wait at all" with one number. Origin ordering inside the queue is a
  separate, smaller thing and both are needed: headroom for permits, origin
  for dispatch.
- **Dropping the per-tick dial ceiling as unimplementable.** It was
  unimplementable where the first draft put it, and the answer is to move the
  draw into `Resolution::admit` rather than to give up the node's only
  aggregate outbound limit.
- **Refreshing every stored ledger.** Unbounded at 10000, and it tells a
  stranger's witnesses forever that this home is still interested.
- **Background crawling of the trust graph.** Decision 016 defers it and the
  exposure question is different in kind.
- **Making background work opt-in.** It would be off on every existing home
  and in the compose topology, so nothing would exercise it. The narrowing of
  4.1 is what makes default-on defensible.
- **A status screen in the UI.** Decision 017 bans a developer mode and names
  the CLI and the HTTP API as the diagnostic surfaces.
- **Three states for the handle check.** The fourth has to exist or a handle
  that verified an hour ago is nothing. Three get copy.

## Consequences

Easier: a node keeps itself current. A ledger this home signs for picks up
events appended elsewhere within 15 minutes rather than never, and a handle
re-verifies daily as decision 015 always said it should. A lookup that fails
says when it will try again, and a person can ask again without waiting. A
developer embedding this crate gets one type with a short API for the network
work. Every attempt is a document rather than a log line. The router's last
side effect goes away, so the rule about reads becomes true instead of nearly
true. And the node gains its first aggregate outbound rate limit, which it has
never had.

Harder: there is a component with a clock, and a clock seam now threads
through 30 call sites. The fetch route changes behavior on equivocation, which
is correct and is still a change. `mabel serve` gains a task tree, a
cancellation protocol and a shutdown ordering constraint. There are two more
kinds of file in the node home, both caches. The 202 breaks every client of
the synchronous fetch body at once, including `docker/smoke.sh` and story 009.
Two failure counters now exist for endpoints and they diverge on purpose. And
the CLI-beside-serve split is visible rather than merely present: `mabel
status` has to say where it got its answer, and one staleness case stays open.

Deferred, and named: a scheduled trust graph crawl, which is decision 016's
own "can come later"; per-identity refresh intervals, where today `refresh` is
one number for the whole home; a manager event stream, so a UI could stop
polling; closing the `LedgerStorage` index staleness that section 4.7 leaves
open; and pushing on a schedule, which this proposal deliberately does not do,
because "automatically" and "writes to a stranger's node" is a sentence that
needs its own decision record.
