//! End-to-end tests against Postgres and S3-compatible object storage.
//!
//! The tests are ignored by default. Start the stack with `ephemeris-up`,
//! then run `cargo nextest run --run-ignored only` in the dev shell.

#[cfg(test)]
mod action;
#[cfg(test)]
mod forge;
#[cfg(test)]
mod harness;
#[cfg(test)]
mod hashserv;
#[cfg(test)]
mod migrations;
#[cfg(test)]
mod nix;
#[cfg(test)]
mod objects;
#[cfg(test)]
mod prune;
#[cfg(test)]
mod worker;
#[cfg(test)]
mod yocto;
