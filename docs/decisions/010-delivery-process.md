# 010: Delivery process

- Date: 2026-08-23
- Status: accepted
- Source: product owner

Phases, each reviewed by both a Claude Opus subagent and Codex before moving
on; the top-level agent decides which review feedback to apply:

1. Requirements and high-level proposal (docs/proposals/001-architecture.md).
2. Implementation plan as tickets under docs/issues/NNN-name.md.
3. Implementation, ticket by ticket, parallelized where possible.
4. Verify all tickets are complete.
5. User stories for end-to-end testing.
6. End-to-end tests implementing those stories.
7. Gap analysis against the original ask, code and docs cleanup.
