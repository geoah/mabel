# 017: demo script over the compose topology

- Status: open
- Depends on: 015

## Goal

One script walks the compose topology through the whole product story with the
CLI, so a reader sees mabel work without reading the tests.

## Scope

- `docker/demo.sh` driving the running compose stack: two people create
  identities in two wallets, configure the witness, push, one founds an org and
  invites the other, the invitee accepts, the founder admits, the person and
  the org each attest trust in a third identity, a stranger verifies from an
  empty home, the issuer revokes, and the script prints the witness ledger list
  and heads (section 11, e2e scenario, run through the CLI).
- Each step prints the command it runs and the relevant output, including the
  verification lines with their source and head (section 6, flag R).
- Root `README.md` gains a short section on running the demo.

Out of scope: Playwright specs, which are phase 6.

## Acceptance criteria

- [ ] The script runs end to end against `docker compose up` and exits 0.
- [ ] It uses only the CLI surface of section 9 and the seeded tickets from
      ticket 015, contacting no external network.
- [ ] The stranger step runs against a wiped home with no keys and prints a
      result naming its source and head sequence (sections 3.7 and 6).
- [ ] tests: a CI-callable invocation of the script returns 0 and its output
      contains the revocation and the witness ledger list.
