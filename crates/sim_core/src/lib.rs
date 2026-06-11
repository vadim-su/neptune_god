//! Headless deterministic factory simulation core.
//!
//! Owns world state, transport lines, buildings, energy networks, and per-tick
//! command application. The Bevy shell (`neptune_app`) advances ticks and renders
//! via [`view`] snapshots and [`diff`] patches; machine logic plugs in through
//! [`behavior_host`].

pub mod activation;
pub mod behavior_host;
pub mod building;
pub mod catalog;
pub mod character_inventory;
pub mod command;
pub mod diff;
pub mod digest;
pub mod energy;
pub mod ids;
pub mod inserter;
pub mod inventory;
pub mod metrics;
pub mod tick;
pub mod topology;
pub mod transport;
pub mod units;
pub mod view;
pub mod world;
pub mod worldgen;

#[cfg(test)]
mod perf_gate;
