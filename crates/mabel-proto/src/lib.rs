//! Prost-generated types for the normative schemas in `proto/mabel/v0/`.
//!
//! This crate contains generated code only. The encoded bytes of signed
//! messages are authoritative (proposal 001 section 3.1): nothing outside
//! the signing path may re-encode an event.

pub use prost;

pub mod v0 {
    include!(concat!(env!("OUT_DIR"), "/mabel.v0.rs"));
}
