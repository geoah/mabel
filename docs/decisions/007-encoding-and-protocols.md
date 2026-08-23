# 007: Encoding and protocols

- Date: 2026-08-23
- Status: accepted
- Source: product owner

- Protobuf is allowed and even suggested for defining ledger events, ideally
  a single `Event` message with a `oneof` over the payload types. The
  architect decides whether to use it; if signing protobuf, the stored
  encoded bytes are authoritative (digest and sign the bytes, never
  re-serialize for verification).
- gRPC is allowed if useful. It is not required and layering HTTP/2 over
  Iroh streams is probably not worth it; the architect decides.
