# 003: Trust

- Date: 2026-08-23
- Status: accepted
- Source: product owner

- Trust is one-way. If Alice trusts Bob, a signed trust event goes into
  Alice's ledger only. Mutual trust is both parties each recording their
  own event. No handshake protocol.
- The ledger is the full history: everything ever trusted and every trust
  revocation stays in the chain.
