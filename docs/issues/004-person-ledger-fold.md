# 004: person ledger fold and the verification pipeline

- Status: open
- Depends on: 003

## Goal

`mabel-core` exposes the one fold function of proposal 001 section 3.6: it
takes an event sequence, applies steps 1 to 6 at each position, and returns
`(state, Option<Violation>)`. Person ledgers verify end to end from nothing.

## Scope

- The fold entry point and `State` covering the fields section 3.6 lists that a
  person ledger uses: kind, active key, reserve commitment, witness set, trust
  map from attestation event id to subject and revocation status, head.
- Steps 1 to 6 of section 3.6 in that order, with the exclusive state boundary:
  event `i` is checked against the fold of `0..=i-1` and applied only after
  every check passes (pitfall 3).
- Envelope rules of section 3.2: `seq == i`, `ledger == ledger_id`, `prev ==
  event_id(events[i-1])`, non-decreasing `timestamp_ms` with the
  `4102444800000` upper bound, seq-0 self-authorization.
- Person payload rules of section 3.4: `PersonInception` at seq 0 then
  `WitnessConfig`, `TrustAttestation`, `TrustRevocation`, all under the same
  active key; witness config replaces the whole set; an attestation is rejected
  when an unrevoked attestation for the same subject exists; a revocation must
  name an unrevoked attestation earlier in the same ledger.
- `Violation` carrying the failing sequence and reason, with the returned state
  being the fold of the valid prefix (partial validity, section 3.6).

Out of scope: org payloads (ticket 005), any IO (section 7).

## Acceptance criteria

- [ ] The fold reads no local state and touches no disk; `mabel-core` has no
      filesystem or tokio dependency (sections 3.6 and 7, pitfall 5).
- [ ] The signature is `(State, Option<Violation>)` and a violation reports the
      failing sequence and reason (section 3.6).
- [ ] Nothing is checked against the verifier's clock (section 3.2).
- [ ] tests: one negative test each for broken prev link, duplicate sequence,
      gap, wrong ledger id, backwards timestamp, timestamp past the year-2100
      bound, unauthorized signer and payload wrong for the ledger kind
      (section 11, chain bullet).
- [ ] tests: attestation duplicating an unrevoked subject, revocation of an
      unknown attestation and revocation of an already revoked attestation are
      each rejected (section 11, policy bullet).
- [ ] tests: a ledger valid to seq N with a bad event at M folds to the state
      at N and reports the violation at M.
