//! End-to-end tests against Postgres and S3-compatible object storage.
//!
//! The tests are ignored by default. With `nix run .#dev` running, run them
//! with `source .dev/env && cargo nextest run --run-ignored only`.

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
