//! World digest: hash building state, energy, and transport for snapshots/tests.

use super::*;
use crate::building::{UndergroundRole, UndergroundRuntime};
use crate::transport::node::{SplitterRuntime, UndergroundTransportRuntime};
use behavior_api::BehaviorStateValue;

pub(super) fn combine_energy_consumer_state_digest(
    digest: &mut WorldDigest,
    state: crate::energy::EnergyConsumerState,
) {
    digest.combine_u64(match state {
        crate::energy::EnergyConsumerState::Powered => 0,
        crate::energy::EnergyConsumerState::Degraded => 1,
        crate::energy::EnergyConsumerState::Offline => 2,
    });
}

pub(super) fn combine_tile_digest(digest: &mut WorldDigest, pos: TilePos) {
    digest.combine_u64(pos.x as i64 as u64);
    digest.combine_u64(pos.y as i64 as u64);
}

pub(super) fn combine_string_digest(digest: &mut WorldDigest, value: &str) {
    digest.combine_u64(value.len() as u64);
    for byte in value.bytes() {
        digest.combine_u64(byte as u64);
    }
}

pub(super) fn combine_building_state_digest(digest: &mut WorldDigest, state: &SimBuildingState) {
    match state {
        SimBuildingState::Passive => digest.combine_u64(0),
        SimBuildingState::Transport => digest.combine_u64(1),
        SimBuildingState::Behavior(state) => {
            digest.combine_u64(2);
            combine_string_digest(digest, state.behavior_id.as_str());
            combine_string_digest(digest, state.status.as_str());
            digest.combine_u64(state.data.len() as u64);
            for (key, value) in &state.data {
                combine_string_digest(digest, key);
                combine_behavior_state_value_digest(digest, value);
            }
        }
        SimBuildingState::Inserter(runtime) => {
            digest.combine_u64(3);
            combine_inserter_runtime_digest(digest, runtime);
        }
        SimBuildingState::Underground(runtime) => {
            digest.combine_u64(5);
            combine_underground_runtime_digest(digest, runtime);
        }
        SimBuildingState::Splitter(runtime) => {
            digest.combine_u64(6);
            combine_splitter_runtime_digest(digest, runtime);
        }
    }
}

pub(super) fn combine_splitter_runtime_digest(digest: &mut WorldDigest, runtime: &SplitterRuntime) {
    digest.combine_u64(runtime.next_output as u64);
    for lane in 0..2 {
        digest.combine_u64(runtime.next_output_for_lane(lane) as u64);
    }
    digest.combine_u64(runtime.ingress_items.len() as u64);
    for item in &runtime.ingress_items {
        digest.combine_u64(core_item_kind_digest(item.item));
        digest.combine_u64(item.input_channel as u64);
        digest.combine_u64(item.lane as u64);
        digest.combine_u64(item.progress.raw() as i64 as u64);
    }
    digest.combine_u64(runtime.buffered_items.len() as u64);
    for item in &runtime.buffered_items {
        digest.combine_u64(core_item_kind_digest(item.item));
        digest.combine_u64(item.source_channel as u64);
        digest.combine_u64(item.lane as u64);
    }
    digest.combine_u64(runtime.egress_items.len() as u64);
    for item in &runtime.egress_items {
        digest.combine_u64(core_item_kind_digest(item.item));
        digest.combine_u64(item.source_channel as u64);
        digest.combine_u64(item.output_channel as u64);
        digest.combine_u64(item.lane as u64);
        digest.combine_u64(item.progress.raw() as i64 as u64);
    }
}

pub(super) fn combine_underground_transport_runtime_digest(
    digest: &mut WorldDigest,
    runtime: &UndergroundTransportRuntime,
) {
    digest.combine_u64(runtime.distance.raw() as i64 as u64);
    digest.combine_u64(runtime.items.len() as u64);
    for item in &runtime.items {
        digest.combine_u64(core_item_kind_digest(item.item));
        digest.combine_u64(item.lane as u64);
        digest.combine_u64(item.progress.raw() as i64 as u64);
    }
}

pub(super) fn combine_underground_runtime_digest(
    digest: &mut WorldDigest,
    runtime: &UndergroundRuntime,
) {
    digest.combine_u64(match runtime.role {
        UndergroundRole::Entrance => 0,
        UndergroundRole::Exit => 1,
    });
    digest.combine_u64(runtime.partner.0 as u64);
    digest.combine_u64(direction_digest(runtime.direction));
}

pub(super) fn combine_behavior_state_value_digest(
    digest: &mut WorldDigest,
    value: &BehaviorStateValue,
) {
    match value {
        BehaviorStateValue::ItemStacks(stacks) => {
            digest.combine_u64(0);
            digest.combine_u64(stacks.len() as u64);
            for stack in stacks {
                digest.combine_u64(stack.kind as u64);
                digest.combine_u64(stack.amount as u64);
            }
        }
        BehaviorStateValue::String(value) => {
            digest.combine_u64(1);
            combine_string_digest(digest, value);
        }
        BehaviorStateValue::U32(value) => {
            digest.combine_u64(2);
            digest.combine_u64(*value as u64);
        }
    }
}

pub(super) fn combine_inserter_runtime_digest(digest: &mut WorldDigest, runtime: &InserterRuntime) {
    digest.combine_u64(direction_digest(runtime.pickup_direction));
    digest.combine_u64(direction_digest(runtime.drop_direction));
    digest.combine_u64(runtime.cooldown_remaining_ticks as u64);
    match runtime.carried {
        Some(stack) => {
            digest.combine_u64(1);
            digest.combine_u64(core_item_kind_digest(stack.kind));
            digest.combine_u64(stack.amount as u64);
        }
        None => digest.combine_u64(0),
    }
}

pub(super) fn core_building_kind_digest(kind: CoreBuildingKind) -> u64 {
    match kind {
        CoreBuildingKind::Machine => 0,
        CoreBuildingKind::Transport => 1,
        CoreBuildingKind::Passive => 2,
        CoreBuildingKind::Inserter => 3,
    }
}

pub(super) fn core_inventory_role_digest(role: CoreInventoryRole) -> u64 {
    match role {
        CoreInventoryRole::Input => 0,
        CoreInventoryRole::Output => 1,
        CoreInventoryRole::Fuel => 2,
        CoreInventoryRole::Storage => 3,
        CoreInventoryRole::InserterHand => 4,
    }
}

pub(super) fn core_item_kind_digest(kind: ItemKindId) -> u64 {
    kind.0 as u64
}

pub(super) fn direction_digest(direction: Direction) -> u64 {
    match direction {
        Direction::North => 0,
        Direction::East => 1,
        Direction::South => 2,
        Direction::West => 3,
    }
}
