# 015: DNS verification

- Date: 2026-08-24
- Status: accepted
- Source: product owner

- An identity can link a hostname to itself, in the style of atproto
  handles: a TXT record under a well-known label carries the identity id;
  the identity claims the hostname on its ledger.
- Verifiers (the wallet node) check the TXT record, cache the result, and
  re-verify roughly daily. The UI shows a verified icon with the hostname.
- Verification is advisory metadata like declared kind: it never gates
  ledger validity.
