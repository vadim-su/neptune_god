//! Tick counter, per-tick output, and behavior effect application reports.

use crate::catalog::CoreItemStack;
use crate::diff::SimDiff;
use crate::ids::{BuildingId, ItemInstanceId, TilePos};
use crate::metrics::SimMetricsSnapshot;
use behavior_api::{BehaviorEffect, BehaviorHostError, BehaviorId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimTick(u64);

impl SimTick {
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    pub fn advance(&mut self) {
        self.0 += 1;
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CoreRemovalDrop {
    pub origin: TilePos,
    pub stack: CoreItemStack,
    pub instance: Option<ItemInstanceId>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CoreSurfaceDrop {
    pub origin: TilePos,
    pub stack: CoreItemStack,
    pub instance: Option<ItemInstanceId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreResourceDepletion {
    pub pos: TilePos,
    pub remaining: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedBehaviorEffect {
    pub effect: BehaviorEffect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RejectedBehaviorEffect {
    pub effect: BehaviorEffect,
    pub reason: BehaviorEffectRejectionReason,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum BehaviorEffectRejectionReason {
    InventoryRejected,
    PowerOutputRejected,
    MissingResource {
        pos: TilePos,
    },
    HostError {
        phase: BehaviorHostFailurePhase,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum BehaviorTickSkipReason {
    Quarantined,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum BehaviorHostFailurePhase {
    Init,
    Command,
    Tick,
    Remove,
    AcceptsInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BehaviorEffectApplication {
    Applied {
        effects: Vec<AppliedBehaviorEffect>,
    },
    Rejected {
        effects: Vec<RejectedBehaviorEffect>,
    },
    Quarantined {
        effects: Vec<RejectedBehaviorEffect>,
    },
    HostFailed {
        phase: BehaviorHostFailurePhase,
        error: BehaviorHostError,
    },
    Skipped {
        reason: BehaviorTickSkipReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BehaviorEffectReport {
    pub building: BuildingId,
    pub origin: TilePos,
    pub behavior_id: Option<BehaviorId>,
    pub application: BehaviorEffectApplication,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SimTickOutput {
    pub tick: SimTick,
    pub diff: SimDiff,
    pub metrics: SimMetricsSnapshot,
    pub removal_drops: Vec<CoreRemovalDrop>,
    pub surface_drops: Vec<CoreSurfaceDrop>,
    pub resource_depletions: Vec<CoreResourceDepletion>,
    pub behavior_effect_reports: Vec<BehaviorEffectReport>,
}
