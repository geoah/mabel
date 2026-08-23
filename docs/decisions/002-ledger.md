# 002: Ledger

- Date: 2026-08-23
- Status: accepted
- Source: product owner

- Our own minimal append-only ledger: hash-chained, ed25519-signed events.
  No KERI, no keriox.
- Each identity (person or org) is one ledger. The identity id derives from
  the inception event.
- Inception creates two keys: an active signing key and a reserve key. The
  inception commits to the reserve key. Key rotation is OUT of scope for
  this POC; only the commitment exists.
- Event types for the POC: inception, witness config (list of witness
  peers), trust attestation, trust revocation, org membership events
  (create org, invite, acceptance, removal).

Amendment (2026-08-23, arbitrated during proposal 001 review, pending owner
ratification): the two-key rule applies to person identities only.
Organizations have no keys of their own; org events are signed by a current
controller's personal key per decision 004, and the change-of-authority path
for an org is its membership events, not a reserve key.
