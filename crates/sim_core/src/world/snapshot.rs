//! Serializable snapshot of [`SimWorld`] for save/load and tests.
//!
//! Captures topology, transport storage, activation scheduler, and optional player
//! inventories so a world can be restored without replaying history.

use std::collections::{BTreeMap, BTreeSet};

use behavior_api::BehaviorId;
use serde::{Deserialize, Serialize};

use super::footprint_offsets_for_direction;
use crate::building::{SimBuildingState, footprint_tiles};
use crate::catalog::{CoreBuildingBehavior, CoreBuildingDef, CoreBuildingDriver, CoreCatalog};
use crate::character_inventory::{LoadedContainerInstance, SimCharacterInventory};
use crate::energy::EnergyNetworkSnapshot;
use crate::ids::{
    BuildingId, DEFAULT_SURFACE_Z, IdAllocator, IdAllocatorSnapshot, InventoryId, ItemInstanceId,
    ItemKindId, SurfaceZ, TilePos,
};
use crate::inventory::SimInventory;
use crate::tick::{BehaviorEffectRejectionReason, CoreRemovalDrop, CoreSurfaceDrop, SimTick};
use crate::topology::graph::TopologyGraphSnapshot;
use crate::transport::node::{SplitterRuntime, TransportNodeRuntime};
use crate::transport::storage::TransportStorageSnapshot;
use crate::units::UnitsPerTick;
use crate::world::SimWorld;

use super::ports::building_ports;
use super::underground_corridor::UndergroundCorridors;
use super::{DayNightSettings, SimBuilding, SimInventoryRecord, TimeOfDay};
use crate::activation::scheduler::{ActivationScheduler, ActivationSnapshot};
use crate::topology::graph::TopologyGraph;
use crate::transport::storage::TransportStorage;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SimInventoryRecordSnapshot {
    pub building: BuildingId,
    pub inventory: SimInventory,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SimBehaviorQuarantineSnapshot {
    pub building: BuildingId,
    pub origin: TilePos,
    pub behavior_id: Option<BehaviorId>,
    pub reason: BehaviorEffectRejectionReason,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
/// Full world state at a point in time (serde round-trip with [`SimWorld`]).
pub struct SimWorldSnapshot {
    pub tick: u64,
    #[serde(default)]
    pub time_of_day: TimeOfDay,
    #[serde(default)]
    pub day_night_settings: DayNightSettings,
    pub ids: IdAllocatorSnapshot,
    pub terrain: BTreeMap<TilePos, String>,
    #[serde(default)]
    pub surface_z: BTreeMap<TilePos, SurfaceZ>,
    pub resources: BTreeMap<TilePos, (ItemKindId, u32)>,
    pub occupied_tiles: BTreeMap<TilePos, UnitsPerTick>,
    pub topology_graph: TopologyGraphSnapshot,
    pub topology_revision_seen: u64,
    pub transport: TransportStorageSnapshot,
    #[serde(default)]
    pub energy: EnergyNetworkSnapshot,
    pub activation: ActivationSnapshot,
    pub occupied_surface_tiles: BTreeSet<TilePos>,
    pub buildings: BTreeMap<BuildingId, SimBuilding>,
    pub building_by_origin: BTreeMap<TilePos, BuildingId>,
    pub building_occupancy: BTreeMap<TilePos, BuildingId>,
    pub inventories: BTreeMap<InventoryId, SimInventoryRecordSnapshot>,
    #[serde(default)]
    pub character_inventory: Option<SimCharacterInventory>,
    #[serde(default)]
    pub loaded_containers: BTreeMap<ItemInstanceId, LoadedContainerInstance>,
    #[serde(default)]
    pub player_inventory: Option<SimInventory>,
    #[serde(default)]
    pub cursor_inventory: Option<SimInventory>,
    #[serde(default)]
    pub removed_item_drops: Vec<CoreRemovalDrop>,
    #[serde(default)]
    pub surface_item_drops: Vec<CoreSurfaceDrop>,
    #[serde(default)]
    pub behavior_quarantine: Vec<SimBehaviorQuarantineSnapshot>,
    #[serde(default)]
    pub underground_corridors: UndergroundCorridors,
}

impl SimWorld {
    pub(super) fn canonical_energy_network(&self) -> crate::energy::EnergyNetwork {
        let mut energy = self.energy.clone();
        if energy.topology_dirty {
            crate::energy::topology::rebuild_energy_topology(
                &mut energy,
                &self.catalog,
                &self.buildings,
            );
        }
        energy
    }

    pub(super) fn behavior_quarantine_snapshot(
        &self,
        building: BuildingId,
        reason: BehaviorEffectRejectionReason,
    ) -> Option<SimBehaviorQuarantineSnapshot> {
        let building_record = self.buildings.get(&building)?;
        let behavior_id = match &building_record.state {
            SimBuildingState::Behavior(state) => Some(state.behavior_id.clone()),
            _ => None,
        };
        Some(SimBehaviorQuarantineSnapshot {
            building,
            origin: building_record.origin,
            behavior_id,
            reason,
        })
    }

    pub fn snapshot(&self) -> SimWorldSnapshot {
        let energy = self.canonical_energy_network();
        SimWorldSnapshot {
            tick: self.tick.raw(),
            time_of_day: self.time_of_day,
            day_night_settings: self.day_night_settings,
            ids: self.ids.snapshot(),
            terrain: self.terrain.clone(),
            surface_z: self.surface_z.clone(),
            resources: self.resources.clone(),
            occupied_tiles: self.occupied_tiles.clone(),
            topology_graph: self.topology_graph.snapshot(),
            topology_revision_seen: self.topology_revision_seen,
            transport: self.transport.snapshot(),
            energy: energy.snapshot(),
            activation: self.activation.snapshot(),
            occupied_surface_tiles: self.occupied_surface_tiles.clone(),
            buildings: self.buildings.clone(),
            building_by_origin: self.building_by_origin.clone(),
            building_occupancy: self.building_occupancy.clone(),
            inventories: self
                .inventories
                .iter()
                .map(|(id, record)| {
                    (
                        *id,
                        SimInventoryRecordSnapshot {
                            building: record.owner,
                            inventory: record.inventory.clone(),
                        },
                    )
                })
                .collect(),
            character_inventory: Some(self.character_inventory.clone()),
            loaded_containers: self.loaded_containers.clone(),
            player_inventory: Some(self.player_inventory.clone()),
            cursor_inventory: Some(self.cursor_inventory.clone()),
            removed_item_drops: self.removed_item_drops.clone(),
            surface_item_drops: self.surface_item_drops.clone(),
            behavior_quarantine: self.behavior_quarantine_snapshots(),
            underground_corridors: self.underground_corridors.clone(),
        }
    }

    pub fn from_snapshot(catalog: CoreCatalog, snapshot: SimWorldSnapshot) -> Result<Self, String> {
        validate_catalog_derived_snapshot_fields(&catalog, &snapshot)?;
        validate_loaded_container_snapshot_fields(&catalog, &snapshot)?;
        validate_behavior_quarantine_snapshot(&snapshot)?;
        let energy_snapshot = snapshot.energy;

        let mut world = Self::with_catalog(catalog);
        world.tick = SimTick::from_raw(snapshot.tick);
        world.day_night_settings = snapshot.day_night_settings.normalized();
        world.time_of_day = snapshot
            .time_of_day
            .with_day_length(world.day_night_settings.day_length_ticks);
        world.ids = IdAllocator::from_snapshot(snapshot.ids);
        world.terrain = snapshot.terrain;
        world.surface_z = snapshot
            .surface_z
            .into_iter()
            .filter(|(_, surface_z)| *surface_z != DEFAULT_SURFACE_Z)
            .collect();
        world.resources = snapshot.resources;
        world.occupied_tiles = snapshot.occupied_tiles;
        world.topology_graph = TopologyGraph::from_snapshot(snapshot.topology_graph);
        world.topology_revision_seen = snapshot.topology_revision_seen;
        world.transport = TransportStorage::from_snapshot(snapshot.transport)?;
        world.activation = ActivationScheduler::from_snapshot(snapshot.activation);
        world.occupied_surface_tiles = snapshot.occupied_surface_tiles;
        world.buildings = snapshot.buildings;
        world.building_by_origin = snapshot.building_by_origin;
        world.building_occupancy = snapshot.building_occupancy;
        world.normalize_splitter_building_states();
        world.inventories = snapshot
            .inventories
            .into_iter()
            .map(|(id, record)| {
                let mut inventory = record.inventory;
                inventory.normalize_slot_instances();
                (
                    id,
                    SimInventoryRecord {
                        id,
                        owner: record.building,
                        inventory,
                    },
                )
            })
            .collect();
        world.character_inventory = snapshot
            .character_inventory
            .unwrap_or_else(|| SimCharacterInventory::from_catalog(&world.catalog));
        world.loaded_containers = snapshot.loaded_containers;
        world.player_inventory = snapshot
            .player_inventory
            .unwrap_or_else(|| SimInventory::from_def(world.catalog.player_inventory_def()));
        world.player_inventory.normalize_slot_instances();
        world.cursor_inventory = snapshot
            .cursor_inventory
            .unwrap_or_else(|| SimInventory::from_def(world.catalog.cursor_inventory_def()));
        world.cursor_inventory.normalize_slot_instances();
        for container in &mut world.character_inventory.containers {
            container.inventory.normalize_slot_instances();
        }
        for container in world.loaded_containers.values_mut() {
            for section in &mut container.containers {
                section.inventory.normalize_slot_instances();
            }
        }
        world.removed_item_drops = snapshot.removed_item_drops;
        world.surface_item_drops = snapshot.surface_item_drops;
        world.behavior_quarantine = snapshot
            .behavior_quarantine
            .into_iter()
            .map(|quarantine| (quarantine.building, quarantine.reason))
            .collect();
        world.underground_corridors = snapshot.underground_corridors;
        crate::energy::topology::rebuild_energy_topology(
            &mut world.energy,
            &world.catalog,
            &world.buildings,
        );
        world.energy.apply_snapshot(energy_snapshot);
        Ok(world)
    }

    fn normalize_splitter_building_states(&mut self) {
        let node_runtimes = self
            .transport
            .nodes_sorted()
            .filter_map(|node| {
                let direction = node.direction?;
                let TransportNodeRuntime::Splitter(runtime) = &node.runtime else {
                    return None;
                };
                Some(((node.sort_tile, direction), runtime.clone()))
            })
            .collect::<BTreeMap<_, _>>();

        for building in self.buildings.values_mut() {
            let Some(def) = self.catalog.building_by_id(&building.def_id) else {
                continue;
            };
            if !matches!(def.behavior.driver, CoreBuildingDriver::Splitter { .. }) {
                continue;
            }
            if matches!(building.state, SimBuildingState::Splitter(_)) {
                continue;
            }
            let runtime = node_runtimes
                .get(&(building.origin, building.direction))
                .cloned()
                .unwrap_or_else(SplitterRuntime::default);
            building.state = SimBuildingState::Splitter(runtime);
        }
    }
}

fn validate_behavior_quarantine_snapshot(snapshot: &SimWorldSnapshot) -> Result<(), String> {
    for quarantine in &snapshot.behavior_quarantine {
        let building = snapshot
            .buildings
            .get(&quarantine.building)
            .ok_or_else(|| {
                format!(
                    "behavior quarantine references missing building {:?}",
                    quarantine.building
                )
            })?;
        if building.origin != quarantine.origin {
            return Err(format!(
                "behavior quarantine for building {:?} origin {:?} does not match building origin {:?}",
                quarantine.building, quarantine.origin, building.origin
            ));
        }
        let SimBuildingState::Behavior(state) = &building.state else {
            return Err(format!(
                "behavior quarantine references non-behavior building {:?}",
                quarantine.building
            ));
        };
        if quarantine
            .behavior_id
            .as_ref()
            .is_some_and(|behavior_id| behavior_id != &state.behavior_id)
        {
            return Err(format!(
                "behavior quarantine for building {:?} behavior does not match building state",
                quarantine.building
            ));
        }
    }
    Ok(())
}

fn validate_catalog_derived_snapshot_fields(
    catalog: &CoreCatalog,
    snapshot: &SimWorldSnapshot,
) -> Result<(), String> {
    let mut expected_inventory_ids = BTreeSet::new();

    for (building_key, building) in &snapshot.buildings {
        if *building_key != building.id {
            return Err(format!(
                "building snapshot key {building_key:?} does not match building id {:?}",
                building.id
            ));
        }
        let def = catalog.building_by_id(&building.def_id).ok_or_else(|| {
            format!(
                "building {:?} references unknown catalog definition '{}'",
                building.id, building.def_id
            )
        })?;

        validate_building_against_def(building, def)?;

        if building.inventories.len() != def.inventories.len() {
            return Err(format!(
                "building {:?} inventory definition count mismatch: snapshot has {}, catalog has {}",
                building.id,
                building.inventories.len(),
                def.inventories.len()
            ));
        }

        for (inventory_id, inventory_def) in building.inventories.iter().zip(&def.inventories) {
            expected_inventory_ids.insert(*inventory_id);
            let record = snapshot.inventories.get(inventory_id).ok_or_else(|| {
                format!(
                    "building {:?} inventory {:?} missing from snapshot inventories",
                    building.id, inventory_id
                )
            })?;
            if record.building != building.id {
                return Err(format!(
                    "inventory {:?} belongs to building {:?}, expected {:?}",
                    inventory_id, record.building, building.id
                ));
            }
            if !record.inventory.matches_def(inventory_def) {
                return Err(format!(
                    "inventory {:?} for building {:?} does not match catalog definition",
                    inventory_id, building.id
                ));
            }
        }
    }

    for inventory_id in snapshot.inventories.keys() {
        if !expected_inventory_ids.contains(inventory_id) {
            return Err(format!(
                "orphan inventory {:?} is not referenced by a known building inventory definition",
                inventory_id
            ));
        }
    }

    if let Some(inventory) = &snapshot.player_inventory
        && !inventory.matches_def(catalog.player_inventory_def())
    {
        return Err("player inventory does not match catalog definition".to_string());
    }

    if let Some(inventory) = &snapshot.character_inventory
        && !inventory.matches_catalog(catalog)
    {
        return Err("character inventory does not match catalog definition".to_string());
    }

    if let Some(inventory) = &snapshot.cursor_inventory
        && !inventory.matches_def(catalog.cursor_inventory_def())
    {
        return Err("cursor inventory does not match catalog definition".to_string());
    }

    Ok(())
}

fn validate_loaded_container_snapshot_fields(
    catalog: &CoreCatalog,
    snapshot: &SimWorldSnapshot,
) -> Result<(), String> {
    let mut referenced = BTreeSet::new();

    for (instance, container) in &snapshot.loaded_containers {
        let item = catalog.item(container.item).ok_or_else(|| {
            format!(
                "loaded container {:?} references unknown item {:?}",
                instance, container.item
            )
        })?;
        let Some(equipment) = item.equipment.as_ref() else {
            return Err(format!(
                "loaded container {:?} item {:?} does not provide containers",
                instance, container.item
            ));
        };
        for section in &container.containers {
            let Some(provided) = equipment
                .provides_containers
                .iter()
                .find(|provided| provided.id.as_str() == section.container_id.as_str())
            else {
                return Err(format!(
                    "loaded container {:?} references unknown section '{}'",
                    instance,
                    section.container_id.as_str()
                ));
            };
            if !section
                .inventory
                .matches_inventory_shape(&SimInventory::from_container_policy(&provided.policy))
            {
                return Err(format!(
                    "loaded container {:?} inventory does not match item container policy",
                    instance
                ));
            }
            validate_inventory_instance_refs(
                &format!(
                    "loaded container {:?} section '{}'",
                    instance,
                    section.container_id.as_str()
                ),
                &section.inventory,
                &snapshot.loaded_containers,
                &mut referenced,
                false,
            )?;
        }
    }

    if let Some(inventory) = &snapshot.player_inventory {
        validate_inventory_instance_refs(
            "player inventory",
            inventory,
            &snapshot.loaded_containers,
            &mut referenced,
            true,
        )?;
    }
    if let Some(inventory) = &snapshot.cursor_inventory {
        validate_inventory_instance_refs(
            "cursor inventory",
            inventory,
            &snapshot.loaded_containers,
            &mut referenced,
            true,
        )?;
    }
    for (inventory_id, record) in &snapshot.inventories {
        validate_inventory_instance_refs(
            &format!("building inventory {:?}", inventory_id),
            &record.inventory,
            &snapshot.loaded_containers,
            &mut referenced,
            true,
        )?;
    }
    if let Some(character) = &snapshot.character_inventory {
        for container in &character.containers {
            validate_inventory_instance_refs(
                &format!("character container '{}'", container.id.as_str()),
                &container.inventory,
                &snapshot.loaded_containers,
                &mut referenced,
                false,
            )?;
        }
    }
    for (index, drop) in snapshot.removed_item_drops.iter().enumerate() {
        validate_drop_instance_ref(
            &format!("removed item drop {index}"),
            drop.stack.kind,
            drop.stack.amount,
            drop.instance,
            &snapshot.loaded_containers,
            &mut referenced,
        )?;
    }
    for (index, drop) in snapshot.surface_item_drops.iter().enumerate() {
        validate_drop_instance_ref(
            &format!("surface item drop {index}"),
            drop.stack.kind,
            drop.stack.amount,
            drop.instance,
            &snapshot.loaded_containers,
            &mut referenced,
        )?;
    }

    Ok(())
}

fn validate_inventory_instance_refs(
    label: &str,
    inventory: &SimInventory,
    loaded_containers: &BTreeMap<ItemInstanceId, LoadedContainerInstance>,
    referenced: &mut BTreeSet<ItemInstanceId>,
    allow_instances: bool,
) -> Result<(), String> {
    let snapshot = inventory.snapshot();
    for (slot_index, (slot, instance)) in snapshot
        .slots
        .into_iter()
        .zip(snapshot.slot_instances)
        .enumerate()
    {
        let Some(instance) = instance else {
            continue;
        };
        if !allow_instances {
            return Err(format!(
                "{label} slot {slot_index} cannot contain loaded container instance {:?}",
                instance
            ));
        }
        let Some(stack) = slot else {
            return Err(format!(
                "{label} slot {slot_index} has loaded container instance {:?} without stack",
                instance
            ));
        };
        if stack.amount != 1 {
            return Err(format!(
                "{label} slot {slot_index} loaded container instance {:?} has stack amount {}",
                instance, stack.amount
            ));
        }
        let Some(container) = loaded_containers.get(&instance) else {
            return Err(format!(
                "{label} slot {slot_index} references missing loaded container instance {:?}",
                instance
            ));
        };
        if stack.kind != container.item {
            return Err(format!(
                "{label} slot {slot_index} loaded container instance {:?} item {:?} does not match stack {:?}",
                instance, container.item, stack.kind
            ));
        }
        if !referenced.insert(instance) {
            return Err(format!(
                "loaded container instance {:?} is referenced by more than one inventory slot",
                instance
            ));
        }
    }
    Ok(())
}

fn validate_drop_instance_ref(
    label: &str,
    stack_kind: ItemKindId,
    stack_amount: u32,
    instance: Option<ItemInstanceId>,
    loaded_containers: &BTreeMap<ItemInstanceId, LoadedContainerInstance>,
    referenced: &mut BTreeSet<ItemInstanceId>,
) -> Result<(), String> {
    let Some(instance) = instance else {
        return Ok(());
    };
    if stack_amount != 1 {
        return Err(format!(
            "{label} loaded container instance {:?} has stack amount {}",
            instance, stack_amount
        ));
    }
    let Some(container) = loaded_containers.get(&instance) else {
        return Err(format!(
            "{label} references missing loaded container instance {:?}",
            instance
        ));
    };
    if stack_kind != container.item {
        return Err(format!(
            "{label} loaded container instance {:?} item {:?} does not match stack {:?}",
            instance, container.item, stack_kind
        ));
    }
    if !referenced.insert(instance) {
        return Err(format!(
            "loaded container instance {:?} is referenced by more than one inventory slot or pending drop",
            instance
        ));
    }
    Ok(())
}

fn validate_building_against_def(
    building: &SimBuilding,
    def: &CoreBuildingDef,
) -> Result<(), String> {
    if building.kind != def.kind {
        return Err(format!(
            "building {:?} kind does not match catalog definition '{}'",
            building.id, building.def_id
        ));
    }

    let expected_footprint_offsets =
        footprint_offsets_for_direction(&def.footprint, def.rotate_footprint, building.direction);
    let expected_footprint = footprint_tiles(building.origin, &expected_footprint_offsets);
    if building.footprint != expected_footprint {
        return Err(format!(
            "building {:?} footprint does not match catalog definition '{}'",
            building.id, building.def_id
        ));
    }

    let expected_ports = building_ports(
        building.origin,
        &expected_footprint,
        building.direction,
        building.surface_z,
        def,
    );
    if building.ports != expected_ports {
        return Err(format!(
            "building {:?} ports do not match catalog definition '{}'",
            building.id, building.def_id
        ));
    }

    validate_building_state_against_behavior(building, &def.behavior)
}

fn validate_building_state_against_behavior(
    building: &SimBuilding,
    behavior: &CoreBuildingBehavior,
) -> Result<(), String> {
    match (&building.state, &behavior.driver) {
        (SimBuildingState::Passive, CoreBuildingDriver::Noop)
        | (SimBuildingState::Transport, CoreBuildingDriver::Transport { .. })
        | (SimBuildingState::Splitter(_), CoreBuildingDriver::Splitter { .. })
        | (SimBuildingState::Underground(_), CoreBuildingDriver::Underground { .. })
        | (SimBuildingState::Inserter(_), CoreBuildingDriver::Inserter { .. }) => Ok(()),
        (SimBuildingState::Behavior(state), CoreBuildingDriver::BehaviorHost) => {
            if state.behavior_id.as_str() != behavior.behavior_id {
                return Err(format!(
                    "building {:?} behavior '{}' does not match catalog behavior '{}' for definition '{}'",
                    building.id,
                    state.behavior_id.as_str(),
                    behavior.behavior_id,
                    building.def_id
                ));
            }
            Ok(())
        }
        _ => Err(format!(
            "building {:?} state does not match catalog behavior for definition '{}'",
            building.id, building.def_id
        )),
    }
}
