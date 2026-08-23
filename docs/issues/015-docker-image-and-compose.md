# 015: container image and compose topology

- Status: open
- Depends on: 014

## Goal

One image runs both node roles, and `docker/compose.yaml` brings up the two
wallets and one witness of proposal 001 section 11 with the witness ticket
already seeded, needing no internet.

## Scope

- `docker/Dockerfile`, multi-stage: a Rust build stage, a Node 22 LTS stage
  building `ui/`, and a slim runtime stage producing one image used by both
  `wallet serve` and `witness run` (sections 11 and 12).
- `docker/compose.yaml`: two wallet services and one witness service, keys on
  named volumes, fixed UDP ports, wallet HTTP ports exposed, witness HTTP port
  exposed (section 11, containers bullet).
- Seeding: the witness `EndpointTicket` written into each wallet's
  `peers.json` at startup, and each wallet's `node.json` listing the witness,
  so no discovery service is contacted (sections 8 and 11).
- Endpoint configuration that keeps the topology working with no outbound
  network, stating in `docker/README.md` which iroh discovery or relay setting
  achieves it.

## Acceptance criteria

- [ ] One image, selected role by command, for all three services (section 11).
- [ ] The UI bundle is embedded in the image, so no host `ui/` mount is needed
      (section 10).
- [ ] Each wallet's `peers.json` contains the witness ticket before its first
      command runs (section 11).
- [ ] tests: `docker compose up` reaches all three services healthy on a host
      with no outbound network, and a scripted check pushes a ledger from one
      wallet and reads its head from the other through the witness.
