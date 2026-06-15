use behavior_api::{BehaviorConfigValue, BehaviorStateValue};
use godot::prelude::*;
use sim_core::building::{SimBuildingSnapshot, SimBuildingState};
use sim_core::catalog::CoreBuildingBehavior;
use sim_core::catalog::CoreBuildingDef;
use sim_core::catalog::CoreBuildingDriver;
use sim_core::catalog::CoreBuildingKind;
use sim_core::catalog::CoreCatalog;
use sim_core::catalog::CoreContainerPolicy;
use sim_core::catalog::CoreEquipmentDef;
use sim_core::catalog::CoreInserterDepositLimit;
use sim_core::catalog::CoreInventoryDef;
use sim_core::catalog::CoreInventoryRole;
use sim_core::catalog::CoreItemDef;
use sim_core::catalog::CoreItemSizeClass;
use sim_core::catalog::CoreItemStack;
use sim_core::catalog::CoreItemStackLimit;
use sim_core::catalog::CorePersonalInventoryDefs;
use sim_core::catalog::CorePortDef;
use sim_core::catalog::CorePortRole;
use sim_core::catalog::CorePortSide;
use sim_core::catalog::CoreProvidedContainerDef;
use sim_core::catalog::CoreStartingEquipment;
use sim_core::catalog::CoreTerrainDef;
use sim_core::catalog::behavior_config_from_parts;
use sim_core::command::SimCommand;
use sim_core::energy::PowerDef;
use sim_core::ids::BuildingId;
use sim_core::ids::CHUNK_SIZE;
use sim_core::ids::ChunkPos;
use sim_core::ids::ItemKindId;
use sim_core::ids::TilePos;
use sim_core::inventory::{InsertMode, SimInventorySnapshot};
use sim_core::topology::graph::Direction;
use sim_core::units::DistanceUnits;
use sim_core::units::UnitsPerTick;
use sim_core::world::SimWorld;
use sim_core::worldgen::{
    DEFAULT_WORLD_SEED, DistanceCurveDef, IntRange, ProfileRangePair, ResourceFrequencyDef,
    ResourceRuleDef, StartingAreaDef, StartingResourceDef, TerrainLayerDef, WorldGenProfile,
    WorldGenerator, default_profile,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::thread;

const SIM_TICKS_PER_SECOND: f64 = 60.0;

#[derive(Clone, Debug, PartialEq)]
struct MachineUiSnapshot {
    id: BuildingId,
    def_id: String,
    ui_kind: &'static str,
    status: String,
    recipe_selector_visible: bool,
    recipe_grid_visible: bool,
    active_recipe: Option<String>,
    process_progress: f64,
    fuel_progress: f64,
    recipes: Vec<RecipeUiSnapshot>,
    inventories: Vec<InventoryUiSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecipeUiSnapshot {
    id: String,
    label: String,
    duration_ticks: u32,
    inputs: Vec<ItemStackUiSnapshot>,
    outputs: Vec<ItemStackUiSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InventoryUiSnapshot {
    role: &'static str,
    slots: Vec<Option<ItemStackUiSnapshot>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlayerInventoryUiSnapshot {
    player_slots: Vec<Option<ItemStackUiSnapshot>>,
    sections: Vec<CharacterContainerUiSnapshot>,
    equipment: Vec<CharacterEquipmentUiSnapshot>,
    cursor: Option<ItemStackUiSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CharacterContainerUiSnapshot {
    id: String,
    name: String,
    slots: Vec<Option<ItemStackUiSnapshot>>,
    used_slots: usize,
    total_slots: usize,
    total_weight_grams: u32,
    max_weight_grams: Option<u32>,
    total_bulk_units: u32,
    max_bulk_units: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CharacterEquipmentUiSnapshot {
    slot: String,
    item: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ItemStackUiSnapshot {
    item: String,
    amount: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecipeCatalogEntry {
    id: String,
    label: String,
    duration_ticks: u32,
    machines: Vec<String>,
    inputs: Vec<ItemStackUiSnapshot>,
    outputs: Vec<ItemStackUiSnapshot>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct GodotCatalogBridge {
    item_def_ids_by_kind: BTreeMap<ItemKindId, String>,
    item_ids_by_def_id: BTreeMap<String, ItemKindId>,
    recipes: BTreeMap<String, RecipeCatalogEntry>,
    recipes_by_building: BTreeMap<String, Vec<String>>,
    warnings: Vec<String>,
}

impl GodotCatalogBridge {
    fn from_core_catalog(catalog: &CoreCatalog) -> Self {
        let mut bridge = Self::default();
        for item in catalog.items() {
            bridge
                .item_def_ids_by_kind
                .insert(item.id, item.def_id.clone());
            bridge
                .item_ids_by_def_id
                .insert(item.def_id.clone(), item.id);
        }
        bridge.load_builtin_recipe_rows();
        bridge
    }

    fn load_builtin_recipe_rows(&mut self) {
        for entry in builtin_recipe_entries() {
            self.insert_recipe(entry);
        }
    }

    fn merge_item_rows(&mut self, rows: &VarArray) {
        for raw in rows.iter_shared() {
            let Ok(row) = raw.try_to::<VarDictionary>() else {
                continue;
            };
            let Some(def_id) = string_field(&row, "id") else {
                continue;
            };
            if let Some(kind) = self.item_ids_by_def_id.get(&def_id).copied() {
                self.item_def_ids_by_kind.insert(kind, def_id);
            }
        }
    }

    fn merge_recipe_rows(&mut self, rows: &VarArray) {
        self.recipes.clear();
        self.recipes_by_building.clear();

        for raw in rows.iter_shared() {
            let Ok(row) = raw.try_to::<VarDictionary>() else {
                continue;
            };
            let Some(id) = string_field(&row, "id") else {
                continue;
            };
            let machines = string_array_field(&row, "machines");
            if machines.is_empty() {
                self.warnings.push(format!("recipe {id} has no machines"));
            }
            let label = string_field(&row, "label").unwrap_or_else(|| recipe_fallback_label(&id));
            let duration_ticks = u32_field(&row, "duration_ticks")
                .or_else(|| {
                    f64_field(&row, "duration_secs")
                        .map(|seconds| (seconds * SIM_TICKS_PER_SECOND).round().max(1.0) as u32)
                })
                .unwrap_or(1);
            let entry = RecipeCatalogEntry {
                id,
                label,
                duration_ticks,
                machines,
                inputs: stack_rows_field(&row, "inputs"),
                outputs: stack_rows_field(&row, "outputs"),
            };
            self.insert_recipe(entry);
        }

        if self.recipes.is_empty() {
            self.warnings.push(
                "configured recipe catalog was empty; using built-in recipe rows".to_string(),
            );
            self.load_builtin_recipe_rows();
        }
    }

    fn insert_recipe(&mut self, entry: RecipeCatalogEntry) {
        for machine in &entry.machines {
            self.recipes_by_building
                .entry(machine.clone())
                .or_default()
                .push(entry.id.clone());
        }
        self.recipes.insert(entry.id.clone(), entry);
    }

    fn recipe_ids_for_building(&self, def_id: &str) -> Vec<String> {
        self.recipes_by_building
            .get(def_id)
            .cloned()
            .unwrap_or_default()
    }

    fn recipe_duration_ticks(&self, recipe_id: &str) -> u32 {
        self.recipes
            .get(recipe_id)
            .map(|recipe| recipe.duration_ticks)
            .unwrap_or(1)
    }

    fn recipes_for_ui(&self, def_id: &str) -> Vec<RecipeUiSnapshot> {
        self.recipe_ids_for_building(def_id)
            .into_iter()
            .filter_map(|recipe_id| self.recipes.get(&recipe_id))
            .map(|recipe| RecipeUiSnapshot {
                id: recipe.id.clone(),
                label: recipe.label.clone(),
                duration_ticks: recipe.duration_ticks,
                inputs: recipe.inputs.clone(),
                outputs: recipe.outputs.clone(),
            })
            .collect()
    }

    fn item_def_id(&self, kind: ItemKindId) -> String {
        self.item_def_ids_by_kind
            .get(&kind)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
    }
}

fn build_core_catalog_from_rows(
    item_rows: &VarArray,
    recipe_rows: &VarArray,
    building_rows: &VarArray,
    terrain_rows: &VarArray,
    player_rows: &VarArray,
) -> Result<CoreCatalog, String> {
    let item_ids = item_ids_from_rows(item_rows)?;
    let items = core_items_from_rows(item_rows, &item_ids)?;
    let terrains = core_terrain_from_rows(terrain_rows)?;
    let recipe_ids_by_machine = recipe_ids_by_machine(recipe_rows);
    let buildings = core_buildings_from_rows(building_rows, &item_ids, &recipe_ids_by_machine)?;
    let personal_inventories = core_personal_inventory_from_rows(player_rows, &item_ids)?;
    Ok(CoreCatalog::new_with_personal_inventories(
        items,
        terrains,
        buildings,
        personal_inventories,
    ))
}

fn build_worldgen_profile_from_rows(
    resource_rows: &VarArray,
    worldgen_rows: &VarArray,
) -> Result<WorldGenProfile, String> {
    let resource_items = resource_items_from_rows(resource_rows)?;
    let Some(profile_row) = select_worldgen_profile_row(worldgen_rows) else {
        return Err("worldgen catalog has no profiles".to_string());
    };
    let profile_id = string_field(&profile_row, "id").unwrap_or_else(|| "default".to_string());
    let starting_area = dictionary_field(&profile_row, "starting_area")
        .map(|row| StartingAreaDef {
            radius: u32_field(&row, "radius").unwrap_or(48),
            terrain: string_field(&row, "terrain").unwrap_or_else(|| "ground".to_string()),
        })
        .unwrap_or_else(|| StartingAreaDef {
            radius: 48,
            terrain: "ground".to_string(),
        });
    let terrain_layers = array_field(&profile_row, "terrain_layers")
        .iter_shared()
        .filter_map(|raw| raw.try_to::<VarDictionary>().ok())
        .map(|row| TerrainLayerDef {
            terrain: string_field(&row, "terrain").unwrap_or_else(|| "ground".to_string()),
            threshold: f64_field(&row, "threshold").unwrap_or(0.75) as f32,
            scale: f64_field(&row, "scale").unwrap_or(96.0),
            min_distance_from_spawn: u32_field(&row, "min_distance_from_spawn").unwrap_or(0),
        })
        .collect::<Vec<_>>();
    let resources = array_field(&profile_row, "resources")
        .iter_shared()
        .filter_map(|raw| raw.try_to::<VarDictionary>().ok())
        .map(|row| resource_rule_from_row(&row, &resource_items))
        .collect::<Result<Vec<_>, _>>()?;
    let starting_resources = array_field(&profile_row, "starting_resources")
        .iter_shared()
        .filter_map(|raw| raw.try_to::<VarDictionary>().ok())
        .map(|row| starting_resource_from_row(&row, &resource_items))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WorldGenProfile {
        id: profile_id,
        starting_area,
        terrain_layers,
        resources,
        starting_resources,
    })
}

fn select_worldgen_profile_row(worldgen_rows: &VarArray) -> Option<VarDictionary> {
    let mut first = None;
    for raw in worldgen_rows.iter_shared() {
        let Ok(row) = raw.try_to::<VarDictionary>() else {
            continue;
        };
        if first.is_none() {
            first = Some(row.clone());
        }
        if string_field(&row, "id").as_deref() == Some("default") {
            return Some(row);
        }
    }
    first
}

fn resource_items_from_rows(resource_rows: &VarArray) -> Result<BTreeMap<String, String>, String> {
    let mut items = BTreeMap::new();
    for raw in resource_rows.iter_shared() {
        let Ok(row) = raw.try_to::<VarDictionary>() else {
            continue;
        };
        let Some(resource_id) = string_field(&row, "id") else {
            continue;
        };
        let item_def_id = string_field(&row, "item")
            .ok_or_else(|| format!("resource '{resource_id}' has no item"))?;
        if items.insert(resource_id.clone(), item_def_id).is_some() {
            return Err(format!("duplicate resource id '{resource_id}'"));
        }
    }
    if items.is_empty() {
        return Err("resource catalog has no resources".to_string());
    }
    Ok(items)
}

fn resource_rule_from_row(
    row: &VarDictionary,
    resource_items: &BTreeMap<String, String>,
) -> Result<ResourceRuleDef, String> {
    let resource = string_field(row, "resource")
        .ok_or_else(|| "worldgen resource rule has no resource".to_string())?;
    let item_def_id = resource_items.get(&resource).cloned().ok_or_else(|| {
        format!("worldgen resource rule references unknown resource '{resource}'")
    })?;
    let patch_frequency = dictionary_field(row, "patch_frequency")
        .map(|frequency| ResourceFrequencyDef {
            base: u32_field(&frequency, "base").unwrap_or(25).max(1),
            distance_curve: dictionary_field(&frequency, "distance_curve")
                .map(|curve| DistanceCurveDef {
                    start_distance: u32_field(&curve, "start_distance").unwrap_or(64),
                    end_distance: u32_field(&curve, "end_distance").unwrap_or(512),
                    multiplier_at_end: f64_field(&curve, "multiplier_at_end").unwrap_or(3.0) as f32,
                })
                .unwrap_or(DistanceCurveDef {
                    start_distance: 64,
                    end_distance: 512,
                    multiplier_at_end: 3.0,
                }),
        })
        .unwrap_or(ResourceFrequencyDef {
            base: 25,
            distance_curve: DistanceCurveDef {
                start_distance: 64,
                end_distance: 512,
                multiplier_at_end: 3.0,
            },
        });
    Ok(ResourceRuleDef {
        resource,
        item_def_id,
        allowed_terrains: non_empty_string_array_field(row, "allowed_terrains", &["ground"]),
        patch_frequency,
        patch_radius: range_pair_field(row, "patch_radius", (3, 5), (7, 12)),
        richness: range_pair_field(row, "richness", (3500, 8000), (25000, 80000)),
    })
}

fn starting_resource_from_row(
    row: &VarDictionary,
    resource_items: &BTreeMap<String, String>,
) -> Result<StartingResourceDef, String> {
    let resource = string_field(row, "resource")
        .ok_or_else(|| "worldgen starting resource has no resource".to_string())?;
    let item_def_id = resource_items.get(&resource).cloned().ok_or_else(|| {
        format!("worldgen starting resource references unknown resource '{resource}'")
    })?;
    Ok(StartingResourceDef {
        resource,
        item_def_id,
        patch_count: u32_field(row, "patch_count").unwrap_or(1).max(1),
        distance_range: range_field(row, "distance_range", 8, 28),
        radius_range: range_field(row, "radius_range", 3, 6),
        amount_range: range_field(row, "amount_range", 6000, 14000),
        allowed_terrains: non_empty_string_array_field(row, "allowed_terrains", &["ground"]),
    })
}

fn range_pair_field(
    row: &VarDictionary,
    key: &str,
    default_near: (u32, u32),
    default_far: (u32, u32),
) -> ProfileRangePair<IntRange> {
    dictionary_field(row, key)
        .map(|pair| ProfileRangePair {
            near: range_field(&pair, "near", default_near.0, default_near.1),
            far: range_field(&pair, "far", default_far.0, default_far.1),
        })
        .unwrap_or(ProfileRangePair {
            near: IntRange {
                min: default_near.0,
                max: default_near.1,
            },
            far: IntRange {
                min: default_far.0,
                max: default_far.1,
            },
        })
}

fn range_field(row: &VarDictionary, key: &str, default_min: u32, default_max: u32) -> IntRange {
    let Some(range) = dictionary_field(row, key) else {
        return IntRange {
            min: default_min,
            max: default_max.max(default_min),
        };
    };
    let min = u32_field(&range, "min").unwrap_or(default_min);
    let max = u32_field(&range, "max").unwrap_or(default_max).max(min);
    IntRange { min, max }
}

fn non_empty_string_array_field(row: &VarDictionary, key: &str, fallback: &[&str]) -> Vec<String> {
    let values = string_array_field(row, key);
    if values.is_empty() {
        fallback.iter().map(|value| (*value).to_string()).collect()
    } else {
        values
    }
}

fn item_ids_from_rows(item_rows: &VarArray) -> Result<BTreeMap<String, ItemKindId>, String> {
    let mut ids = BTreeMap::new();
    for raw in item_rows.iter_shared() {
        let Ok(row) = raw.try_to::<VarDictionary>() else {
            continue;
        };
        let Some(def_id) = string_field(&row, "id") else {
            continue;
        };
        if ids.contains_key(&def_id) {
            return Err(format!("duplicate item id '{def_id}'"));
        }
        let next = ids.len() + 1;
        if next > u16::MAX as usize {
            return Err("too many item definitions for ItemKindId".to_string());
        }
        ids.insert(def_id, ItemKindId(next as u16));
    }
    if ids.is_empty() {
        return Err("item catalog has no item definitions".to_string());
    }
    Ok(ids)
}

fn core_items_from_rows(
    item_rows: &VarArray,
    item_ids: &BTreeMap<String, ItemKindId>,
) -> Result<Vec<CoreItemDef>, String> {
    let mut items = Vec::new();
    for raw in item_rows.iter_shared() {
        let Ok(row) = raw.try_to::<VarDictionary>() else {
            continue;
        };
        let Some(def_id) = string_field(&row, "id") else {
            continue;
        };
        let Some(id) = item_ids.get(&def_id).copied() else {
            continue;
        };
        items.push(CoreItemDef {
            id,
            def_id: def_id.clone(),
            max_stack: u32_field(&row, "max_stack").unwrap_or(100).max(1),
            weight_grams: u32_field(&row, "weight_grams").unwrap_or(0),
            bulk_units: u32_field(&row, "bulk_units").unwrap_or(0),
            size_class: string_field(&row, "size_class")
                .and_then(|value| core_item_size_class(&value))
                .unwrap_or(CoreItemSizeClass::Medium),
            tags: string_array_field(&row, "tags"),
            equipment: equipment_def_from_row(&row, item_ids)?,
        });
    }
    Ok(items)
}

fn equipment_def_from_row(
    row: &VarDictionary,
    item_ids: &BTreeMap<String, ItemKindId>,
) -> Result<Option<CoreEquipmentDef>, String> {
    let Some(equipment) = dictionary_field(row, "equipment") else {
        return Ok(None);
    };
    let Some(slot) = string_field(&equipment, "slot") else {
        return Ok(None);
    };
    let containers = array_field(&equipment, "provides_containers")
        .iter_shared()
        .filter_map(|raw| raw.try_to::<VarDictionary>().ok())
        .map(|container| provided_container_from_row(&container, item_ids))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(CoreEquipmentDef {
        slot,
        provides_containers: containers,
    }))
}

fn provided_container_from_row(
    row: &VarDictionary,
    item_ids: &BTreeMap<String, ItemKindId>,
) -> Result<CoreProvidedContainerDef, String> {
    let id = string_field(row, "id").ok_or_else(|| "provided container has no id".to_string())?;
    Ok(CoreProvidedContainerDef {
        name: string_field(row, "name").unwrap_or_else(|| id.clone()),
        id,
        policy: CoreContainerPolicy {
            slots: u32_field(row, "slots").unwrap_or(1).max(1) as usize,
            max_stack: u32_field(row, "max_stack").unwrap_or(100).max(1),
            stack_limits: stack_limits_from_row(row, item_ids)?,
            comfortable_weight_limit_grams: u32_field(row, "comfortable_weight_limit_grams"),
            hard_weight_limit_grams: u32_field(row, "hard_weight_limit_grams")
                .or_else(|| u32_field(row, "max_weight_grams")),
            max_bulk_units: u32_field(row, "max_bulk_units"),
            max_item_size: string_field(row, "max_item_size")
                .and_then(|value| core_item_size_class(&value))
                .unwrap_or(CoreItemSizeClass::Oversized),
            accepts_tags: string_array_field(row, "accepts_tags"),
            rejects_tags: string_array_field(row, "rejects_tags"),
            accepts_items: item_list_field(row, "accepts_items", item_ids)?,
            rejects_items: item_list_field(row, "rejects_items", item_ids)?,
            pickup_priority: i32_field(row, "pickup_priority").unwrap_or(0),
            quick_access: bool_field(row, "quick_access").unwrap_or(false),
        },
    })
}

fn core_terrain_from_rows(terrain_rows: &VarArray) -> Result<Vec<CoreTerrainDef>, String> {
    let mut terrains = Vec::new();
    for raw in terrain_rows.iter_shared() {
        let Ok(row) = raw.try_to::<VarDictionary>() else {
            continue;
        };
        let Some(id) = string_field(&row, "id") else {
            continue;
        };
        terrains.push(CoreTerrainDef {
            id,
            buildable: bool_field(&row, "buildable").unwrap_or(true),
            weight: u32_field(&row, "weight").unwrap_or(1).max(1),
        });
    }
    if terrains.is_empty() {
        return Err("terrain catalog has no terrain definitions".to_string());
    }
    Ok(terrains)
}

fn core_buildings_from_rows(
    building_rows: &VarArray,
    item_ids: &BTreeMap<String, ItemKindId>,
    recipe_ids_by_machine: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<CoreBuildingDef>, String> {
    let mut buildings = Vec::new();
    for raw in building_rows.iter_shared() {
        let Ok(row) = raw.try_to::<VarDictionary>() else {
            continue;
        };
        let Some(id) = string_field(&row, "id") else {
            continue;
        };
        let sim = simulation_dictionary(&row);
        let kind = string_field(&sim, "kind")
            .and_then(|value| core_building_kind(&value))
            .ok_or_else(|| format!("building '{id}' has no valid sim.kind"))?;
        let footprint = footprint_field(&sim, "footprint")
            .ok_or_else(|| format!("building '{id}' has no valid sim.footprint"))?;
        buildings.push(CoreBuildingDef {
            id: id.clone(),
            kind,
            footprint,
            rotate_footprint: bool_field(&sim, "rotate_footprint").unwrap_or(false),
            inputs: ports_from_field(&sim, "inputs", item_ids)?,
            outputs: ports_from_field(&sim, "outputs", item_ids)?,
            inventories: inventories_from_field(&sim, "inventories", item_ids)?,
            inserter_deposit_limits: inserter_deposit_limits_from_field(&sim, item_ids)?,
            behavior: behavior_from_row(&id, &sim, recipe_ids_by_machine)?,
            power: PowerDef::none(),
        });
    }
    if buildings.is_empty() {
        return Err("building catalog has no building definitions".to_string());
    }
    Ok(buildings)
}

fn core_personal_inventory_from_rows(
    player_rows: &VarArray,
    item_ids: &BTreeMap<String, ItemKindId>,
) -> Result<CorePersonalInventoryDefs, String> {
    let player = player_rows
        .iter_shared()
        .filter_map(|raw| raw.try_to::<VarDictionary>().ok())
        .next();
    let player_inventory = player
        .as_ref()
        .and_then(|row| dictionary_field(row, "inventory"))
        .map(|row| inventory_from_row(&row, CoreInventoryRole::Storage, item_ids))
        .transpose()?
        .unwrap_or_else(|| {
            let mut inventory = CoreInventoryDef::new(CoreInventoryRole::Storage, 80, 100);
            inventory.comfortable_weight_limit_grams = Some(40_000);
            inventory.hard_weight_limit_grams = Some(46_000);
            inventory
        });
    let cursor = player
        .as_ref()
        .and_then(|row| dictionary_field(row, "cursor"))
        .map(|row| inventory_from_row(&row, CoreInventoryRole::Storage, item_ids))
        .transpose()?
        .unwrap_or_else(|| CoreInventoryDef::new(CoreInventoryRole::Storage, 1, 100));
    let starting_equipment = player
        .as_ref()
        .map(|row| starting_equipment_from_row(row, item_ids))
        .transpose()?
        .unwrap_or_default();
    Ok(CorePersonalInventoryDefs {
        player: player_inventory,
        cursor,
        starting_equipment,
    })
}

fn recipe_ids_by_machine(recipe_rows: &VarArray) -> BTreeMap<String, Vec<String>> {
    let mut recipes_by_machine: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for raw in recipe_rows.iter_shared() {
        let Ok(row) = raw.try_to::<VarDictionary>() else {
            continue;
        };
        let Some(recipe_id) = string_field(&row, "id") else {
            continue;
        };
        for machine in string_array_field(&row, "machines") {
            recipes_by_machine
                .entry(machine)
                .or_default()
                .push(recipe_id.clone());
        }
    }
    recipes_by_machine
}

fn simulation_dictionary(row: &VarDictionary) -> VarDictionary {
    dictionary_field(row, "sim").unwrap_or_else(|| row.clone())
}

fn ports_from_field(
    row: &VarDictionary,
    key: &str,
    item_ids: &BTreeMap<String, ItemKindId>,
) -> Result<Vec<CorePortDef>, String> {
    array_field(row, key)
        .iter_shared()
        .filter_map(|raw| raw.try_to::<VarDictionary>().ok())
        .map(|port| {
            let role = string_field(&port, "role")
                .and_then(|value| core_port_role(&value))
                .ok_or_else(|| format!("port in '{key}' has no valid role"))?;
            let side = string_field(&port, "side")
                .and_then(|value| core_port_side(&value))
                .ok_or_else(|| format!("port in '{key}' has no valid side"))?;
            Ok(CorePortDef {
                role,
                side,
                offsets: i32_array_field(&port, "offsets"),
                accepts: item_list_field(&port, "accepts", item_ids)?,
            })
        })
        .collect()
}

fn inventories_from_field(
    row: &VarDictionary,
    key: &str,
    item_ids: &BTreeMap<String, ItemKindId>,
) -> Result<Vec<CoreInventoryDef>, String> {
    array_field(row, key)
        .iter_shared()
        .filter_map(|raw| raw.try_to::<VarDictionary>().ok())
        .map(|inventory| inventory_from_row(&inventory, CoreInventoryRole::Storage, item_ids))
        .collect()
}

fn inventory_from_row(
    row: &VarDictionary,
    default_role: CoreInventoryRole,
    item_ids: &BTreeMap<String, ItemKindId>,
) -> Result<CoreInventoryDef, String> {
    let role = string_field(row, "role")
        .and_then(|value| core_inventory_role(&value))
        .unwrap_or(default_role);
    let mut inventory = CoreInventoryDef::new(
        role,
        u32_field(row, "slots").unwrap_or(1).max(1) as usize,
        u32_field(row, "max_stack").unwrap_or(100).max(1),
    );
    inventory.stack_limits = stack_limits_from_row(row, item_ids)?;
    inventory.comfortable_weight_limit_grams = u32_field(row, "comfortable_weight_limit_grams");
    inventory.hard_weight_limit_grams =
        u32_field(row, "hard_weight_limit_grams").or_else(|| u32_field(row, "max_weight_grams"));
    inventory.max_bulk_units = u32_field(row, "max_bulk_units");
    inventory.max_item_size = string_field(row, "max_item_size")
        .and_then(|value| core_item_size_class(&value))
        .unwrap_or(CoreItemSizeClass::Oversized);
    inventory.accepts_tags = string_array_field(row, "accepts_tags");
    inventory.rejects_tags = string_array_field(row, "rejects_tags");
    inventory.accepts = item_list_field(row, "accepts", item_ids)
        .or_else(|_| item_list_field(row, "accepts_items", item_ids))?;
    inventory.rejects = item_list_field(row, "rejects", item_ids)
        .or_else(|_| item_list_field(row, "rejects_items", item_ids))?;
    inventory.pickup_priority = i32_field(row, "pickup_priority").unwrap_or(0);
    inventory.quick_access = bool_field(row, "quick_access").unwrap_or(false);
    Ok(inventory)
}

fn inserter_deposit_limits_from_field(
    row: &VarDictionary,
    item_ids: &BTreeMap<String, ItemKindId>,
) -> Result<Vec<CoreInserterDepositLimit>, String> {
    array_field(row, "inserter_deposit_limits")
        .iter_shared()
        .filter_map(|raw| raw.try_to::<VarDictionary>().ok())
        .map(|limit| {
            let role = string_field(&limit, "role")
                .and_then(|value| core_inventory_role(&value))
                .ok_or_else(|| "inserter deposit limit has no valid role".to_string())?;
            let item = item_field(&limit, "item", item_ids)?;
            Ok(CoreInserterDepositLimit {
                role,
                item,
                max_amount: u32_field(&limit, "max_amount").unwrap_or(1).max(1),
            })
        })
        .collect()
}

fn behavior_from_row(
    building_id: &str,
    row: &VarDictionary,
    recipe_ids_by_machine: &BTreeMap<String, Vec<String>>,
) -> Result<CoreBuildingBehavior, String> {
    let behavior = dictionary_field(row, "behavior");
    let driver = behavior
        .as_ref()
        .and_then(|value| string_field(value, "driver"))
        .unwrap_or_else(|| "noop".to_string());
    match normalized_key(&driver).as_str() {
        "noop" => Ok(CoreBuildingBehavior::noop(
            behavior
                .as_ref()
                .and_then(|value| string_field(value, "behavior_id"))
                .unwrap_or_else(|| "mod:noop".to_string()),
        )),
        "transport" => Ok(CoreBuildingBehavior::transport(UnitsPerTick::new(
            behavior
                .as_ref()
                .and_then(|value| u32_field(value, "speed_units_per_tick"))
                .unwrap_or(4)
                .min(i32::MAX as u32) as i32,
        ))),
        "underground" => Ok(CoreBuildingBehavior::underground(
            UnitsPerTick::new(
                behavior
                    .as_ref()
                    .and_then(|value| u32_field(value, "speed_units_per_tick"))
                    .unwrap_or(4)
                    .min(i32::MAX as u32) as i32,
            ),
            behavior
                .as_ref()
                .and_then(|value| u32_field(value, "max_range_tiles"))
                .unwrap_or(4)
                .min(u8::MAX as u32) as u8,
        )),
        "splitter" => Ok(CoreBuildingBehavior::splitter(UnitsPerTick::new(
            behavior
                .as_ref()
                .and_then(|value| u32_field(value, "speed_units_per_tick"))
                .unwrap_or(4)
                .min(i32::MAX as u32) as i32,
        ))),
        "inserter" => Ok(CoreBuildingBehavior::inserter(
            behavior
                .as_ref()
                .and_then(|value| u32_field(value, "cooldown_ticks"))
                .unwrap_or(27),
        )),
        "behaviorhost" | "host" => {
            let behavior = behavior.ok_or_else(|| {
                format!("building '{building_id}' behavior_host has no behavior object")
            })?;
            let role = string_field(&behavior, "role").unwrap_or_else(|| "processor".to_string());
            let recipes = {
                let configured = string_array_field(&behavior, "recipes");
                if configured.is_empty() {
                    recipe_ids_by_machine
                        .get(building_id)
                        .cloned()
                        .unwrap_or_default()
                } else {
                    configured
                }
            };
            let work_area = footprint_field(&behavior, "work_area").unwrap_or_default();
            Ok(CoreBuildingBehavior::hosted(
                string_field(&behavior, "behavior_id")
                    .unwrap_or_else(|| "test:behavior".to_string()),
                behavior_config_from_parts(&role, recipes, work_area),
            ))
        }
        _ => Err(format!(
            "building '{building_id}' has unknown behavior driver '{driver}'"
        )),
    }
}

fn starting_equipment_from_row(
    row: &VarDictionary,
    item_ids: &BTreeMap<String, ItemKindId>,
) -> Result<Vec<CoreStartingEquipment>, String> {
    array_field(row, "starting_equipment")
        .iter_shared()
        .filter_map(|raw| raw.try_to::<VarDictionary>().ok())
        .map(|entry| {
            let slot = string_field(&entry, "slot")
                .ok_or_else(|| "starting equipment entry has no slot".to_string())?;
            let item = item_field(&entry, "item", item_ids)?;
            Ok(CoreStartingEquipment { slot, item })
        })
        .collect()
}

fn stack_limits_from_row(
    row: &VarDictionary,
    item_ids: &BTreeMap<String, ItemKindId>,
) -> Result<Vec<CoreItemStackLimit>, String> {
    array_field(row, "stack_limits")
        .iter_shared()
        .filter_map(|raw| raw.try_to::<VarDictionary>().ok())
        .map(|limit| {
            let item = item_field(&limit, "item", item_ids)?;
            Ok(CoreItemStackLimit {
                item,
                max_stack: u32_field(&limit, "max_stack").unwrap_or(1).max(1),
            })
        })
        .collect()
}

fn item_list_field(
    row: &VarDictionary,
    key: &str,
    item_ids: &BTreeMap<String, ItemKindId>,
) -> Result<Vec<ItemKindId>, String> {
    string_array_field(row, key)
        .into_iter()
        .map(|def_id| {
            item_ids
                .get(&def_id)
                .copied()
                .ok_or_else(|| format!("unknown item id '{def_id}' in {key}"))
        })
        .collect()
}

fn item_field(
    row: &VarDictionary,
    key: &str,
    item_ids: &BTreeMap<String, ItemKindId>,
) -> Result<ItemKindId, String> {
    let def_id = string_field(row, key).ok_or_else(|| format!("missing item field '{key}'"))?;
    item_ids
        .get(&def_id)
        .copied()
        .ok_or_else(|| format!("unknown item id '{def_id}' in {key}"))
}

fn footprint_field(row: &VarDictionary, key: &str) -> Option<Vec<(i32, i32)>> {
    let value = row.get(key)?;
    if let Ok(dictionary) = value.try_to::<VarDictionary>() {
        if let Some(rectangle) = rectangle_from_dictionary(&dictionary) {
            return Some(rectangle);
        }
    }
    let Ok(array) = value.try_to::<VarArray>() else {
        return None;
    };
    let pairs = array
        .iter_shared()
        .filter_map(|raw| {
            if let Ok(dictionary) = raw.try_to::<VarDictionary>() {
                return Some((i32_field(&dictionary, "x")?, i32_field(&dictionary, "y")?));
            }
            let pair = raw.try_to::<VarArray>().ok()?;
            let values = pair
                .iter_shared()
                .filter_map(|value| variant_i32(&value))
                .collect::<Vec<_>>();
            match values.as_slice() {
                [x, y, ..] => Some((*x, *y)),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    if pairs.is_empty() { None } else { Some(pairs) }
}

fn rectangle_from_dictionary(dictionary: &VarDictionary) -> Option<Vec<(i32, i32)>> {
    let rectangle = array_field(dictionary, "rectangle")
        .iter_shared()
        .filter_map(|value| variant_i32(&value))
        .collect::<Vec<_>>();
    match rectangle.as_slice() {
        [width, height, ..] => Some(rectangle_tiles(*width, *height)),
        _ => None,
    }
}

fn rectangle_tiles(width: i32, height: i32) -> Vec<(i32, i32)> {
    let width = width.max(0);
    let height = height.max(0);
    (0..width)
        .flat_map(|x| (0..height).map(move |y| (x, y)))
        .collect()
}

fn dictionary_field(dictionary: &VarDictionary, key: &str) -> Option<VarDictionary> {
    dictionary.get(key)?.try_to::<VarDictionary>().ok()
}

fn array_field(dictionary: &VarDictionary, key: &str) -> VarArray {
    dictionary
        .get(key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_else(VarArray::new)
}

fn bool_field(dictionary: &VarDictionary, key: &str) -> Option<bool> {
    dictionary.get(key)?.try_to::<bool>().ok()
}

fn i32_field(dictionary: &VarDictionary, key: &str) -> Option<i32> {
    dictionary.get(key).and_then(|value| variant_i32(&value))
}

fn variant_i32(value: &Variant) -> Option<i32> {
    if let Ok(number) = value.try_to::<i64>() {
        return Some(number.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32);
    }
    value.try_to::<f64>().ok().map(|number| {
        number
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX))
            .round() as i32
    })
}

fn i32_array_field(dictionary: &VarDictionary, key: &str) -> Vec<i32> {
    array_field(dictionary, key)
        .iter_shared()
        .filter_map(|value| variant_i32(&value))
        .collect()
}

fn normalized_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| *character != '_' && *character != '-' && !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn core_item_size_class(value: &str) -> Option<CoreItemSizeClass> {
    match normalized_key(value).as_str() {
        "tiny" => Some(CoreItemSizeClass::Tiny),
        "small" => Some(CoreItemSizeClass::Small),
        "medium" => Some(CoreItemSizeClass::Medium),
        "large" => Some(CoreItemSizeClass::Large),
        "oversized" => Some(CoreItemSizeClass::Oversized),
        _ => None,
    }
}

fn core_building_kind(value: &str) -> Option<CoreBuildingKind> {
    match normalized_key(value).as_str() {
        "machine" => Some(CoreBuildingKind::Machine),
        "transport" => Some(CoreBuildingKind::Transport),
        "passive" => Some(CoreBuildingKind::Passive),
        "inserter" => Some(CoreBuildingKind::Inserter),
        _ => None,
    }
}

fn core_inventory_role(value: &str) -> Option<CoreInventoryRole> {
    match normalized_key(value).as_str() {
        "input" => Some(CoreInventoryRole::Input),
        "output" => Some(CoreInventoryRole::Output),
        "fuel" => Some(CoreInventoryRole::Fuel),
        "storage" => Some(CoreInventoryRole::Storage),
        "inserterhand" | "hand" => Some(CoreInventoryRole::InserterHand),
        _ => None,
    }
}

fn core_port_role(value: &str) -> Option<CorePortRole> {
    match normalized_key(value).as_str() {
        "input" => Some(CorePortRole::Input),
        "output" => Some(CorePortRole::Output),
        "fuel" => Some(CorePortRole::Fuel),
        "storage" => Some(CorePortRole::Storage),
        "beltlane" => Some(CorePortRole::BeltLane),
        _ => None,
    }
}

fn core_port_side(value: &str) -> Option<CorePortSide> {
    match normalized_key(value).as_str() {
        "north" => Some(CorePortSide::North),
        "east" => Some(CorePortSide::East),
        "south" => Some(CorePortSide::South),
        "west" => Some(CorePortSide::West),
        "outputdirection" => Some(CorePortSide::OutputDirection),
        "oppositeoutput" => Some(CorePortSide::OppositeOutput),
        "outputdirectionleft" => Some(CorePortSide::OutputDirectionLeft),
        "outputdirectionright" => Some(CorePortSide::OutputDirectionRight),
        "alledges" => Some(CorePortSide::AllEdges),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum UiInventorySlotRef {
    Character {
        container_id: String,
        slot: usize,
    },
    Building {
        building_id: BuildingId,
        role: CoreInventoryRole,
        slot: usize,
    },
    Player {
        slot: usize,
    },
}

struct NeptuneGodotExtension;

#[gdextension]
unsafe impl ExtensionLibrary for NeptuneGodotExtension {}

#[derive(GodotClass)]
#[class(base=RefCounted)]
pub struct NeptuneChunkTileProvider {
    configured: bool,
    worldgen_profile: WorldGenProfile,
    next_chunk_job_id: i64,
    chunk_jobs: BTreeMap<i64, Arc<Mutex<Option<ChunkTileJobResult>>>>,
    base: Base<RefCounted>,
}

struct ChunkTileJobResult {
    terrain_tiles: Vec<sim_core::worldgen::GeneratedTerrainTile>,
    resources: BTreeMap<TilePos, (String, u32)>,
    core_min: TilePos,
    core_max: TilePos,
}

#[godot_api]
impl IRefCounted for NeptuneChunkTileProvider {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            configured: false,
            worldgen_profile: default_profile(),
            next_chunk_job_id: 1,
            chunk_jobs: BTreeMap::new(),
            base,
        }
    }
}

#[godot_api]
impl NeptuneChunkTileProvider {
    #[func]
    pub fn configure_worldgen(&mut self, resource_rows: VarArray, worldgen_rows: VarArray) -> bool {
        if resource_rows.is_empty() && worldgen_rows.is_empty() {
            self.worldgen_profile = default_profile();
            self.configured = true;
            self.next_chunk_job_id = 1;
            self.chunk_jobs.clear();
            return true;
        }
        match build_worldgen_profile_from_rows(&resource_rows, &worldgen_rows) {
            Ok(profile) => {
                self.worldgen_profile = profile;
                self.configured = true;
                self.next_chunk_job_id = 1;
                self.chunk_jobs.clear();
                true
            }
            Err(error) => {
                godot_error!("failed to configure render worldgen provider: {error}");
                false
            }
        }
    }

    #[func]
    pub fn start_chunk_tiles_job(&mut self, chunk_x: i32, chunk_y: i32, margin: i32) -> i64 {
        if !self.configured {
            godot_error!(
                "NeptuneChunkTileProvider.start_chunk_tiles_job called before configure_worldgen()"
            );
            return -1;
        }

        let job_id = self.next_chunk_job_id;
        self.next_chunk_job_id += 1;
        let result_slot = Arc::new(Mutex::new(None));
        let thread_result_slot = Arc::clone(&result_slot);
        let profile = self.worldgen_profile.clone();
        thread::spawn(move || {
            let result = generate_chunk_tile_job(profile, chunk_x, chunk_y, margin);
            if let Ok(mut slot) = thread_result_slot.lock() {
                *slot = Some(result);
            }
        });
        self.chunk_jobs.insert(job_id, result_slot);
        job_id
    }

    #[func]
    pub fn is_chunk_tiles_job_ready(&self, job_id: i64) -> bool {
        let Some(result_slot) = self.chunk_jobs.get(&job_id) else {
            return false;
        };
        result_slot
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    #[func]
    pub fn take_chunk_tiles_job(&mut self, job_id: i64) -> VarArray {
        let Some(result_slot) = self.chunk_jobs.get(&job_id) else {
            return VarArray::new();
        };
        let Ok(mut slot) = result_slot.lock() else {
            return VarArray::new();
        };
        let Some(result) = slot.take() else {
            return VarArray::new();
        };
        drop(slot);
        self.chunk_jobs.remove(&job_id);
        chunk_tile_job_result_to_var_array(result)
    }

    #[func]
    pub fn discard_chunk_tiles_job(&mut self, job_id: i64) {
        self.chunk_jobs.remove(&job_id);
    }
}

#[derive(GodotClass)]
#[class(base=RefCounted)]
pub struct NeptuneSim {
    world: SimWorld,
    configured: bool,
    map_min: TilePos,
    map_max: TilePos,
    generated_chunks: BTreeSet<ChunkPos>,
    selected_recipes: BTreeMap<BuildingId, String>,
    worldgen_profile: WorldGenProfile,
    catalog: GodotCatalogBridge,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for NeptuneSim {
    fn init(base: Base<RefCounted>) -> Self {
        let core_catalog = CoreCatalog::for_tests();
        let catalog = GodotCatalogBridge::from_core_catalog(&core_catalog);
        Self {
            world: SimWorld::with_catalog(core_catalog),
            configured: false,
            map_min: TilePos::new(0, 0),
            map_max: TilePos::new(0, 0),
            generated_chunks: BTreeSet::new(),
            selected_recipes: BTreeMap::new(),
            worldgen_profile: default_profile(),
            catalog,
            base,
        }
    }
}

#[godot_api]
impl NeptuneSim {
    #[func]
    pub fn configure_catalogs(
        &mut self,
        item_rows: VarArray,
        recipe_rows: VarArray,
        building_rows: VarArray,
        terrain_rows: VarArray,
        player_rows: VarArray,
        resource_rows: VarArray,
        worldgen_rows: VarArray,
    ) -> bool {
        let core_catalog = match build_core_catalog_from_rows(
            &item_rows,
            &recipe_rows,
            &building_rows,
            &terrain_rows,
            &player_rows,
        ) {
            Ok(catalog) => catalog,
            Err(error) => {
                godot_error!("failed to configure simulation catalog: {error}");
                return false;
            }
        };
        let worldgen_profile =
            match build_worldgen_profile_from_rows(&resource_rows, &worldgen_rows) {
                Ok(profile) => profile,
                Err(error) => {
                    godot_error!("failed to configure worldgen catalog: {error}");
                    return false;
                }
            };
        self.world = SimWorld::with_catalog(core_catalog);
        self.worldgen_profile = worldgen_profile;

        let mut ui_catalog = GodotCatalogBridge::from_core_catalog(self.world.catalog());
        ui_catalog.merge_item_rows(&item_rows);
        ui_catalog.merge_recipe_rows(&recipe_rows);
        for warning in &ui_catalog.warnings {
            godot_warn!("catalog bridge warning: {warning}");
        }
        self.catalog = ui_catalog;
        self.map_min = TilePos::new(0, 0);
        self.map_max = TilePos::new(0, 0);
        self.generated_chunks.clear();
        self.selected_recipes.clear();
        self.configured = true;
        true
    }

    #[func]
    pub fn tick(&mut self) {
        if !self.ensure_configured("tick") {
            return;
        }
        self.world.tick_core_only_for_tests();
    }

    #[func]
    pub fn tick_many(&mut self, count: u32) {
        for _ in 0..count {
            self.tick();
        }
    }

    #[func]
    pub fn core_tick(&self) -> i64 {
        self.world.current_tick().raw() as i64
    }

    #[func]
    pub fn digest(&self) -> i64 {
        self.world.digest().0 as i64
    }

    #[func]
    pub fn building_count(&self) -> i64 {
        self.world.building_snapshots().len() as i64
    }

    #[func]
    pub fn can_place_building(&self, def_id: GString, x: i32, y: i32, quarter_turns: i32) -> bool {
        if !self.ensure_configured("can_place_building") {
            return false;
        }
        can_place_building_for_godot(
            &self.world,
            def_id.to_string().as_str(),
            x,
            y,
            quarter_turns,
        )
    }

    #[func]
    pub fn place_building(&mut self, def_id: GString, x: i32, y: i32, quarter_turns: i32) -> bool {
        if !self.ensure_configured("place_building") {
            return false;
        }
        place_building_for_godot(
            &mut self.world,
            def_id.to_string().as_str(),
            x,
            y,
            quarter_turns,
        )
    }

    #[func]
    pub fn remove_building(&mut self, x: i32, y: i32) -> bool {
        if !self.ensure_configured("remove_building") {
            return false;
        }
        remove_building_for_godot(&mut self.world, x, y)
    }

    #[func]
    pub fn building_footprint(
        &self,
        def_id: GString,
        x: i32,
        y: i32,
        quarter_turns: i32,
    ) -> VarArray {
        if !self.ensure_configured("building_footprint") {
            return VarArray::new();
        }
        tile_pairs_to_var_array(building_footprint_for_godot(
            &self.world,
            def_id.to_string().as_str(),
            x,
            y,
            quarter_turns,
        ))
    }

    #[func]
    pub fn generate_starting_map(&mut self, radius: i32) {
        if !self.ensure_configured("generate_starting_map") {
            return;
        }
        let radius = radius.max(1);
        self.ensure_generated_rect(-radius, -radius, radius, radius);
    }

    #[func]
    pub fn chunk_size(&self) -> i32 {
        CHUNK_SIZE
    }

    #[func]
    pub fn ensure_generated_rect(
        &mut self,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
    ) -> VarArray {
        if !self.ensure_configured("ensure_generated_rect") {
            return VarArray::new();
        }

        let min = TilePos::new(min_x.min(max_x), min_y.min(max_y));
        let max = TilePos::new(min_x.max(max_x), min_y.max(max_y));
        let min_chunk = min.chunk_pos();
        let max_chunk = max.chunk_pos();
        let mut generated_chunks = VarArray::new();

        for chunk_y in min_chunk.y..=max_chunk.y {
            for chunk_x in min_chunk.x..=max_chunk.x {
                let chunk = ChunkPos::new(chunk_x, chunk_y);
                if !self.generated_chunks.insert(chunk) {
                    continue;
                }
                if let Err(error) = self.generate_chunk(chunk) {
                    self.generated_chunks.remove(&chunk);
                    godot_error!("failed to generate world chunk ({chunk_x}, {chunk_y}): {error}");
                    continue;
                }
                self.include_chunk_in_map_bounds(chunk);

                let mut chunk_row = VarDictionary::new();
                chunk_row.set("x", chunk_x);
                chunk_row.set("y", chunk_y);
                generated_chunks.push(&chunk_row);
            }
        }

        generated_chunks
    }

    #[func]
    pub fn map_tile_count(&self) -> i64 {
        let width = i64::from(self.map_max.x - self.map_min.x + 1).max(0);
        let height = i64::from(self.map_max.y - self.map_min.y + 1).max(0);
        width * height
    }

    #[func]
    pub fn resource_count(&self) -> i64 {
        self.world
            .resource_tiles_in_rect(self.map_min, self.map_max)
            .len() as i64
    }

    #[func]
    pub fn map_tiles(&self) -> VarArray {
        if !self.ensure_configured("map_tiles") {
            return VarArray::new();
        }
        let resources = self
            .world
            .resource_tiles_in_rect(self.map_min, self.map_max)
            .into_iter()
            .map(|(pos, item, amount)| (pos, (item, amount)))
            .collect::<std::collections::BTreeMap<_, _>>();
        let terrain_tiles = self
            .world
            .terrain_tiles_in_rect(self.map_min, self.map_max)
            .into_iter()
            .map(|(pos, terrain_id)| sim_core::worldgen::GeneratedTerrainTile { pos, terrain_id })
            .collect::<Vec<_>>();
        generated_tiles_to_var_array(&terrain_tiles, resources, self.map_min, self.map_max)
    }

    #[func]
    pub fn buildings(&self) -> VarArray {
        if !self.ensure_configured("buildings") {
            return VarArray::new();
        }
        let mut buildings = VarArray::new();
        for snapshot in self.world.building_snapshots() {
            let mut building = VarDictionary::new();
            building.set("id", snapshot.id.0 as i64);
            building.set("def_id", snapshot.def_id.as_str());
            building.set("x", snapshot.origin.x);
            building.set("y", snapshot.origin.y);
            building.set("direction", direction_name(snapshot.direction));
            building.set(
                "quarter_turns",
                quarter_turns_from_direction(snapshot.direction),
            );
            if let Some(belt) = self.world.belt_tile_at(snapshot.origin) {
                building.set("input_direction", direction_name(belt.input_direction));
                building.set(
                    "input_quarter_turns",
                    quarter_turns_from_direction(belt.input_direction),
                );
                building.set(
                    "belt_speed_tiles_per_second",
                    belt_speed_tiles_per_second(&self.world, snapshot.def_id.as_str()),
                );
            }
            building.set(
                "footprint",
                &tile_pairs_to_var_array(building_footprint_for_godot(
                    &self.world,
                    snapshot.def_id.as_str(),
                    snapshot.origin.x,
                    snapshot.origin.y,
                    quarter_turns_from_direction(snapshot.direction),
                )),
            );
            buildings.push(&building);
        }
        buildings
    }

    #[func]
    pub fn building_ui_snapshot(&self, building_id: i64) -> VarDictionary {
        if !self.ensure_configured("building_ui_snapshot") {
            return VarDictionary::new();
        }
        building_ui_snapshot_for_godot(
            &self.world,
            &self.catalog,
            &self.selected_recipes,
            BuildingId(building_id.max(0) as u32),
        )
        .unwrap_or_else(VarDictionary::new)
    }

    #[func]
    pub fn inventory_snapshot(&self) -> VarDictionary {
        if !self.ensure_configured("inventory_snapshot") {
            return VarDictionary::new();
        }
        inventory_ui_snapshot_to_godot(inventory_ui_snapshot_data(&self.world, &self.catalog))
    }

    #[func]
    pub fn give_item(&mut self, item_id: GString, amount: i64) -> bool {
        if !self.ensure_configured("give_item") {
            return false;
        }
        let item_id = item_id.to_string();
        let amount = amount.clamp(1, i64::from(u32::MAX)) as u32;
        give_item_for_godot(&mut self.world, item_id.as_str(), amount)
    }

    #[func]
    pub fn transfer_inventory_slot(
        &mut self,
        from_ref: VarDictionary,
        to_ref: VarDictionary,
        amount: i64,
    ) -> bool {
        if !self.ensure_configured("transfer_inventory_slot") {
            return false;
        }
        let Some(from_ref) = ui_inventory_slot_ref_from_godot(&from_ref) else {
            return false;
        };
        let Some(to_ref) = ui_inventory_slot_ref_from_godot(&to_ref) else {
            return false;
        };
        let amount = amount.clamp(1, i64::from(u32::MAX)) as u32;
        transfer_inventory_slot_for_godot(&mut self.world, &from_ref, &to_ref, amount)
    }

    #[func]
    pub fn click_inventory_slot(&mut self, slot_ref: VarDictionary, action: GString) -> bool {
        if !self.ensure_configured("click_inventory_slot") {
            return false;
        }
        let Some(slot_ref) = ui_inventory_slot_ref_from_godot(&slot_ref) else {
            return false;
        };
        click_inventory_slot_for_godot(&mut self.world, &slot_ref, action.to_string().as_str())
    }

    #[func]
    pub fn set_building_recipe(&mut self, building_id: i64, recipe_id: GString) -> bool {
        if !self.ensure_configured("set_building_recipe") {
            return false;
        }
        let building_id = BuildingId(building_id.max(0) as u32);
        let Some(snapshot) = self
            .world
            .building_snapshots()
            .into_iter()
            .find(|snapshot| snapshot.id == building_id)
        else {
            return false;
        };
        let recipe_id = recipe_id.to_string();
        if recipe_id.is_empty() {
            self.selected_recipes.remove(&building_id);
            return true;
        }
        let valid = self
            .catalog
            .recipe_ids_for_building(&snapshot.def_id)
            .into_iter()
            .any(|recipe| recipe == recipe_id);
        if !valid {
            return false;
        }
        self.selected_recipes.insert(building_id, recipe_id);
        true
    }

    #[func]
    pub fn reset(&mut self) {
        if !self.ensure_configured("reset") {
            return;
        }
        self.world = SimWorld::with_catalog(self.world.catalog().clone());
        self.map_min = TilePos::new(0, 0);
        self.map_max = TilePos::new(0, 0);
        self.generated_chunks.clear();
        self.selected_recipes.clear();
    }

    fn generate_chunk(&mut self, chunk: ChunkPos) -> Result<(), String> {
        let generator = WorldGenerator::new(DEFAULT_WORLD_SEED, self.worldgen_profile.clone());
        let generated = generator.generate_rect(chunk_min_tile(chunk), chunk_max_tile(chunk));
        self.world.apply_generated_region(&generated)
    }

    fn include_chunk_in_map_bounds(&mut self, chunk: ChunkPos) {
        let min = chunk_min_tile(chunk);
        let max = chunk_max_tile(chunk);
        if self.generated_chunks.len() == 1 {
            self.map_min = min;
            self.map_max = max;
            return;
        }

        self.map_min = TilePos::new(self.map_min.x.min(min.x), self.map_min.y.min(min.y));
        self.map_max = TilePos::new(self.map_max.x.max(max.x), self.map_max.y.max(max.y));
    }

    fn ensure_configured(&self, method: &str) -> bool {
        if self.configured {
            true
        } else {
            godot_error!("NeptuneSim.{method} called before configure_catalogs()");
            false
        }
    }
}

fn chunk_min_tile(chunk: ChunkPos) -> TilePos {
    TilePos::new(chunk.x * CHUNK_SIZE, chunk.y * CHUNK_SIZE)
}

fn chunk_max_tile(chunk: ChunkPos) -> TilePos {
    TilePos::new(
        chunk.x * CHUNK_SIZE + CHUNK_SIZE - 1,
        chunk.y * CHUNK_SIZE + CHUNK_SIZE - 1,
    )
}

fn generate_chunk_tile_job(
    worldgen_profile: WorldGenProfile,
    chunk_x: i32,
    chunk_y: i32,
    margin: i32,
) -> ChunkTileJobResult {
    let chunk = ChunkPos::new(chunk_x, chunk_y);
    let margin = margin.max(0);
    let core_min = chunk_min_tile(chunk);
    let core_max = chunk_max_tile(chunk);
    let min = TilePos::new(core_min.x - margin, core_min.y - margin);
    let max = TilePos::new(core_max.x + margin, core_max.y + margin);
    let generator = WorldGenerator::new(DEFAULT_WORLD_SEED, worldgen_profile);
    let generated = generator.generate_rect(min, max);
    let resources = generated
        .resource_tiles
        .iter()
        .filter_map(|resource| {
            if resource.pos.x < core_min.x
                || resource.pos.x > core_max.x
                || resource.pos.y < core_min.y
                || resource.pos.y > core_max.y
            {
                return None;
            }
            Some((
                resource.pos,
                (resource.item_def_id.clone(), resource.amount),
            ))
        })
        .collect::<BTreeMap<_, _>>();

    ChunkTileJobResult {
        terrain_tiles: generated.terrain_tiles,
        resources,
        core_min,
        core_max,
    }
}

fn chunk_tile_job_result_to_var_array(result: ChunkTileJobResult) -> VarArray {
    generated_tiles_to_var_array(
        &result.terrain_tiles,
        result.resources,
        result.core_min,
        result.core_max,
    )
}

fn generated_tiles_to_var_array(
    terrain_tiles: &[sim_core::worldgen::GeneratedTerrainTile],
    resources: BTreeMap<TilePos, (String, u32)>,
    core_min: TilePos,
    core_max: TilePos,
) -> VarArray {
    let mut tiles = VarArray::new();
    for tile_data in terrain_tiles {
        let pos = tile_data.pos;
        let in_core = pos.x >= core_min.x
            && pos.x <= core_max.x
            && pos.y >= core_min.y
            && pos.y <= core_max.y;
        let mut tile = VarDictionary::new();
        tile.set("x", pos.x);
        tile.set("y", pos.y);
        tile.set("terrain", tile_data.terrain_id.clone());
        tile.set("render", in_core);
        if in_core {
            if let Some((resource, amount)) = resources.get(&pos) {
                tile.set("resource", resource.clone());
                tile.set("amount", *amount as i64);
            } else {
                tile.set("resource", "");
                tile.set("amount", 0_i64);
            }
        } else {
            tile.set("resource", "");
            tile.set("amount", 0_i64);
        }
        tiles.push(&tile);
    }
    tiles
}

fn can_place_building_for_godot(
    world: &SimWorld,
    def_id: &str,
    x: i32,
    y: i32,
    quarter_turns: i32,
) -> bool {
    world
        .can_place_core_building(
            def_id,
            TilePos::new(x, y),
            direction_from_quarter_turns(quarter_turns),
        )
        .is_ok()
}

fn place_building_for_godot(
    world: &mut SimWorld,
    def_id: &str,
    x: i32,
    y: i32,
    quarter_turns: i32,
) -> bool {
    world
        .apply_core_command_for_tests(SimCommand::PlaceBuilding {
            def_id: def_id.to_string(),
            origin: TilePos::new(x, y),
            direction: direction_from_quarter_turns(quarter_turns),
            inserter_drop_direction: None,
        })
        .is_ok()
}

fn remove_building_for_godot(world: &mut SimWorld, x: i32, y: i32) -> bool {
    world
        .apply_core_command_for_tests(SimCommand::RemoveBuilding {
            pos: TilePos::new(x, y),
        })
        .is_ok()
}

fn give_item_for_godot(world: &mut SimWorld, item_id: &str, amount: u32) -> bool {
    let Some(kind) = world.catalog().item_id_by_def_id(item_id) else {
        return false;
    };
    world
        .insert_into_player_inventory_for_tests(CoreItemStack { kind, amount })
        .is_ok()
}

fn transfer_inventory_slot_for_godot(
    world: &mut SimWorld,
    from_ref: &UiInventorySlotRef,
    to_ref: &UiInventorySlotRef,
    amount: u32,
) -> bool {
    if amount == 0 || from_ref == to_ref {
        return false;
    }

    let Some(source_stack) = take_ui_inventory_slot_stack(world, from_ref, amount) else {
        return false;
    };
    let target_stack = ui_inventory_slot_stack(world, to_ref);

    let transferred = if target_stack.is_some_and(|stack| stack.kind != source_stack.kind) {
        transfer_inventory_slot_with_swap(world, from_ref, to_ref, source_stack)
    } else {
        insert_ui_inventory_slot_stack(world, to_ref, source_stack)
    };

    if transferred {
        return true;
    }

    let _ = insert_ui_inventory_slot_stack(world, from_ref, source_stack);
    false
}

fn click_inventory_slot_for_godot(
    world: &mut SimWorld,
    slot_ref: &UiInventorySlotRef,
    action: &str,
) -> bool {
    match action {
        "one" => click_inventory_slot_one(world, slot_ref),
        "split" => click_inventory_slot_split(world, slot_ref),
        _ => click_inventory_slot_stack(world, slot_ref),
    }
}

fn click_inventory_slot_stack(world: &mut SimWorld, slot_ref: &UiInventorySlotRef) -> bool {
    let Some(held) = world.take_all_from_cursor_inventory() else {
        let Some(stack) = ui_inventory_slot_stack(world, slot_ref) else {
            return true;
        };
        if let Some(taken) = take_ui_inventory_slot_stack(world, slot_ref, stack.amount) {
            let _ = world.set_cursor_inventory_stack(Some(taken));
        }
        return true;
    };

    if let Some(existing) = ui_inventory_slot_stack(world, slot_ref)
        && existing.kind != held.kind
    {
        let Some(taken) = take_ui_inventory_slot_stack(world, slot_ref, existing.amount) else {
            let _ = world.set_cursor_inventory_stack(Some(held));
            return true;
        };
        if insert_ui_inventory_slot_stack(world, slot_ref, held) {
            let _ = world.set_cursor_inventory_stack(Some(taken));
        } else {
            let _ = insert_ui_inventory_slot_stack(world, slot_ref, taken);
            let _ = world.set_cursor_inventory_stack(Some(held));
        }
        return true;
    }

    if insert_ui_inventory_slot_stack(world, slot_ref, held) {
        let _ = world.set_cursor_inventory_stack(None);
    } else {
        let _ = world.set_cursor_inventory_stack(Some(held));
    }
    true
}

fn click_inventory_slot_one(world: &mut SimWorld, slot_ref: &UiInventorySlotRef) -> bool {
    let Some(cursor) = world.cursor_stack() else {
        if let Some(taken) = take_ui_inventory_slot_stack(world, slot_ref, 1) {
            let _ = world.set_cursor_inventory_stack(Some(taken));
        }
        return true;
    };

    let Some(taken) = world.take_from_cursor_inventory(1) else {
        return true;
    };
    if insert_ui_inventory_slot_stack(world, slot_ref, taken) {
        return true;
    }

    let restored = CoreItemStack {
        kind: cursor.kind,
        amount: world
            .cursor_stack()
            .map_or(taken.amount, |remaining| remaining.amount + taken.amount),
    };
    let _ = world.set_cursor_inventory_stack(Some(restored));
    true
}

fn click_inventory_slot_split(world: &mut SimWorld, slot_ref: &UiInventorySlotRef) -> bool {
    if world.cursor_stack().is_some() {
        return click_inventory_slot_stack(world, slot_ref);
    }
    let Some(stack) = ui_inventory_slot_stack(world, slot_ref) else {
        return true;
    };
    let amount = stack.amount / 2;
    if amount == 0 {
        return true;
    }
    if let Some(taken) = take_ui_inventory_slot_stack(world, slot_ref, amount) {
        let _ = world.set_cursor_inventory_stack(Some(taken));
    }
    true
}

fn transfer_inventory_slot_with_swap(
    world: &mut SimWorld,
    from_ref: &UiInventorySlotRef,
    to_ref: &UiInventorySlotRef,
    source_stack: CoreItemStack,
) -> bool {
    let Some(target_stack) = ui_inventory_slot_stack(world, to_ref) else {
        return insert_ui_inventory_slot_stack(world, to_ref, source_stack);
    };
    let Some(taken_target_stack) = take_ui_inventory_slot_stack(world, to_ref, target_stack.amount)
    else {
        return false;
    };

    if !insert_ui_inventory_slot_stack(world, to_ref, source_stack) {
        let _ = insert_ui_inventory_slot_stack(world, to_ref, taken_target_stack);
        return false;
    }

    if insert_ui_inventory_slot_stack(world, from_ref, taken_target_stack) {
        return true;
    }

    let _ = take_ui_inventory_slot_stack(world, to_ref, source_stack.amount);
    let _ = insert_ui_inventory_slot_stack(world, from_ref, source_stack);
    let _ = insert_ui_inventory_slot_stack(world, to_ref, taken_target_stack);
    false
}

fn ui_inventory_slot_stack(
    world: &SimWorld,
    slot_ref: &UiInventorySlotRef,
) -> Option<CoreItemStack> {
    match slot_ref {
        UiInventorySlotRef::Character { container_id, slot } => {
            world.character_container_slot(container_id, *slot)
        }
        UiInventorySlotRef::Building {
            building_id,
            role,
            slot,
        } => world.inventory_slot(*building_id, *role, *slot),
        UiInventorySlotRef::Player { slot } => world.player_inventory_slot(*slot),
    }
}

fn take_ui_inventory_slot_stack(
    world: &mut SimWorld,
    slot_ref: &UiInventorySlotRef,
    amount: u32,
) -> Option<CoreItemStack> {
    match slot_ref {
        UiInventorySlotRef::Character { container_id, slot } => {
            world.take_from_character_container_slot(container_id, *slot, amount)
        }
        UiInventorySlotRef::Building {
            building_id,
            role,
            slot,
        } => world
            .take_from_inventory_stack(*building_id, *role, *slot, amount)
            .ok(),
        UiInventorySlotRef::Player { slot } => world.take_from_player_inventory_slot(*slot, amount),
    }
}

fn insert_ui_inventory_slot_stack(
    world: &mut SimWorld,
    slot_ref: &UiInventorySlotRef,
    stack: CoreItemStack,
) -> bool {
    match slot_ref {
        UiInventorySlotRef::Character { container_id, slot } => world
            .insert_into_character_container_slot(
                container_id,
                *slot,
                stack,
                InsertMode::AtomicAllOrNothing,
            )
            .rejected
            .is_none(),
        UiInventorySlotRef::Building {
            building_id,
            role,
            slot,
        } => world
            .insert_into_inventory_slot(
                *building_id,
                *role,
                *slot,
                stack,
                InsertMode::AtomicAllOrNothing,
            )
            .rejected
            .is_none(),
        UiInventorySlotRef::Player { slot } => world
            .insert_into_player_inventory_slot(*slot, stack, InsertMode::AtomicAllOrNothing)
            .rejected
            .is_none(),
    }
}

fn ui_inventory_slot_ref_from_godot(dictionary: &VarDictionary) -> Option<UiInventorySlotRef> {
    let kind = dictionary
        .get("kind")?
        .try_to::<GString>()
        .ok()?
        .to_string();
    match kind.as_str() {
        "character" => Some(UiInventorySlotRef::Character {
            container_id: dictionary
                .get("container")?
                .try_to::<GString>()
                .ok()?
                .to_string(),
            slot: dictionary.get("slot")?.try_to::<i64>().ok()?.max(0) as usize,
        }),
        "building" => Some(UiInventorySlotRef::Building {
            building_id: BuildingId(
                dictionary.get("building_id")?.try_to::<i64>().ok()?.max(0) as u32
            ),
            role: core_inventory_role_from_ui(
                dictionary
                    .get("role")?
                    .try_to::<GString>()
                    .ok()?
                    .to_string()
                    .as_str(),
            )?,
            slot: dictionary.get("slot")?.try_to::<i64>().ok()?.max(0) as usize,
        }),
        "player" => Some(UiInventorySlotRef::Player {
            slot: dictionary.get("slot")?.try_to::<i64>().ok()?.max(0) as usize,
        }),
        _ => None,
    }
}

fn core_inventory_role_from_ui(role: &str) -> Option<CoreInventoryRole> {
    match role {
        "Input" => Some(CoreInventoryRole::Input),
        "Output" => Some(CoreInventoryRole::Output),
        "Fuel" => Some(CoreInventoryRole::Fuel),
        "Storage" => Some(CoreInventoryRole::Storage),
        "Hand" => Some(CoreInventoryRole::InserterHand),
        _ => None,
    }
}

fn building_footprint_for_godot(
    world: &SimWorld,
    def_id: &str,
    x: i32,
    y: i32,
    quarter_turns: i32,
) -> Vec<(i32, i32)> {
    world
        .building_footprint_for(
            def_id,
            TilePos::new(x, y),
            direction_from_quarter_turns(quarter_turns),
        )
        .unwrap_or_default()
        .into_iter()
        .map(|pos| (pos.x, pos.y))
        .collect()
}

fn direction_from_quarter_turns(quarter_turns: i32) -> Direction {
    match quarter_turns.rem_euclid(4) {
        1 => Direction::North,
        2 => Direction::West,
        3 => Direction::South,
        _ => Direction::East,
    }
}

fn quarter_turns_from_direction(direction: Direction) -> i32 {
    match direction {
        Direction::East => 0,
        Direction::North => 1,
        Direction::West => 2,
        Direction::South => 3,
    }
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::East => "east",
        Direction::North => "north",
        Direction::West => "west",
        Direction::South => "south",
    }
}

fn belt_speed_tiles_per_second(world: &SimWorld, def_id: &str) -> f64 {
    let Some(def) = world.catalog().building_by_id(def_id) else {
        return 0.0;
    };

    let speed_units_per_tick = match &def.behavior.driver {
        CoreBuildingDriver::Transport {
            speed_units_per_tick,
        }
        | CoreBuildingDriver::Underground {
            speed_units_per_tick,
            ..
        }
        | CoreBuildingDriver::Splitter {
            speed_units_per_tick,
        } => speed_units_per_tick,
        CoreBuildingDriver::Noop
        | CoreBuildingDriver::Inserter { .. }
        | CoreBuildingDriver::BehaviorHost => return 0.0,
    };

    f64::from(speed_units_per_tick.raw()) * SIM_TICKS_PER_SECOND
        / f64::from(DistanceUnits::UNITS_PER_TILE)
}

fn tile_pairs_to_var_array(tiles: Vec<(i32, i32)>) -> VarArray {
    let mut array = VarArray::new();
    for (x, y) in tiles {
        let mut tile = VarDictionary::new();
        tile.set("x", x);
        tile.set("y", y);
        array.push(&tile);
    }
    array
}

fn building_ui_snapshot_for_godot(
    world: &SimWorld,
    catalog: &GodotCatalogBridge,
    selected_recipes: &BTreeMap<BuildingId, String>,
    building_id: BuildingId,
) -> Option<VarDictionary> {
    building_ui_snapshot_data(world, catalog, selected_recipes, building_id)
        .map(machine_ui_snapshot_to_godot)
}

fn building_ui_snapshot_data(
    world: &SimWorld,
    catalog: &GodotCatalogBridge,
    selected_recipes: &BTreeMap<BuildingId, String>,
    building_id: BuildingId,
) -> Option<MachineUiSnapshot> {
    let snapshot = world
        .building_snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == building_id)?;
    let def = world.catalog().building_by_id(&snapshot.def_id)?;
    let ui_kind = ui_kind_for_building(def, &snapshot);
    if ui_kind.is_empty() {
        return None;
    }

    let active_recipe = active_recipe_for_snapshot(catalog, def, &snapshot, selected_recipes);
    Some(MachineUiSnapshot {
        id: snapshot.id,
        def_id: snapshot.def_id.clone(),
        ui_kind,
        status: behavior_status_for_snapshot(&snapshot),
        recipe_selector_visible: def_behavior_role(def) == Some("processor")
            && catalog.recipe_ids_for_building(&snapshot.def_id).len() > 1,
        recipe_grid_visible: def_behavior_role(def) == Some("processor") && active_recipe.is_none(),
        process_progress: process_progress_for_snapshot(
            catalog,
            &snapshot,
            active_recipe.as_deref(),
        ),
        fuel_progress: fuel_progress_for_snapshot(&snapshot),
        recipes: catalog.recipes_for_ui(&snapshot.def_id),
        inventories: inventories_for_ui(catalog, &snapshot.inventories),
        active_recipe,
    })
}

fn machine_ui_snapshot_to_godot(snapshot: MachineUiSnapshot) -> VarDictionary {
    let mut dictionary = VarDictionary::new();
    dictionary.set("id", snapshot.id.0 as i64);
    dictionary.set("def_id", snapshot.def_id.as_str());
    dictionary.set("ui_kind", snapshot.ui_kind);
    dictionary.set("status", snapshot.status.as_str());
    dictionary.set("recipe_selector_visible", snapshot.recipe_selector_visible);
    dictionary.set("recipe_grid_visible", snapshot.recipe_grid_visible);
    dictionary.set("active_recipe", snapshot.active_recipe.unwrap_or_default());
    dictionary.set("process_progress", snapshot.process_progress);
    dictionary.set("fuel_progress", snapshot.fuel_progress);
    dictionary.set("recipes", &recipes_to_godot(snapshot.recipes));
    dictionary.set("inventories", &inventories_to_godot(snapshot.inventories));
    dictionary
}

fn ui_kind_for_building(def: &CoreBuildingDef, snapshot: &SimBuildingSnapshot) -> &'static str {
    match def.kind {
        CoreBuildingKind::Machine if matches!(snapshot.state, SimBuildingState::Behavior(_)) => {
            "machine"
        }
        CoreBuildingKind::Passive
            if def
                .inventories
                .iter()
                .any(|inventory| inventory.role == CoreInventoryRole::Storage) =>
        {
            "container"
        }
        _ => "",
    }
}

fn active_recipe_for_snapshot(
    catalog: &GodotCatalogBridge,
    def: &CoreBuildingDef,
    snapshot: &SimBuildingSnapshot,
    selected_recipes: &BTreeMap<BuildingId, String>,
) -> Option<String> {
    if let Some(recipe) = selected_recipes.get(&snapshot.id) {
        return Some(recipe.clone());
    }
    if let SimBuildingState::Behavior(state) = &snapshot.state {
        if let Some(BehaviorStateValue::String(recipe)) = state.data.get("active_recipe") {
            return Some(recipe.clone());
        }
    }
    let recipes = catalog.recipe_ids_for_building(&snapshot.def_id);
    match (def_behavior_role(def), recipes.as_slice()) {
        (Some("extractor"), [first, ..]) => Some(first.clone()),
        (_, [only]) => Some(only.clone()),
        _ => None,
    }
}

fn behavior_status_for_snapshot(snapshot: &SimBuildingSnapshot) -> String {
    if let SimBuildingState::Behavior(state) = &snapshot.state {
        return state.status.as_str().replace('_', " ");
    }
    match snapshot.kind {
        CoreBuildingKind::Machine => "idle".to_string(),
        CoreBuildingKind::Transport => "transport".to_string(),
        CoreBuildingKind::Passive => "idle".to_string(),
        CoreBuildingKind::Inserter => "inserter".to_string(),
    }
}

fn process_progress_for_snapshot(
    catalog: &GodotCatalogBridge,
    snapshot: &SimBuildingSnapshot,
    active_recipe: Option<&str>,
) -> f64 {
    let Some(active_recipe) = active_recipe else {
        return 0.0;
    };
    let duration = catalog.recipe_duration_ticks(active_recipe).max(1);
    let progress_ticks = if let SimBuildingState::Behavior(state) = &snapshot.state {
        match state.data.get("progress_ticks") {
            Some(BehaviorStateValue::U32(value)) => *value,
            _ => 0,
        }
    } else {
        0
    };
    (f64::from(progress_ticks) / f64::from(duration)).clamp(0.0, 1.0)
}

fn fuel_progress_for_snapshot(snapshot: &SimBuildingSnapshot) -> f64 {
    let SimBuildingState::Behavior(state) = &snapshot.state else {
        return 0.0;
    };
    let remaining = match state.data.get("fuel_remaining_ticks") {
        Some(BehaviorStateValue::U32(value)) => *value,
        _ => 0,
    };
    let total = match state.data.get("fuel_total_ticks") {
        Some(BehaviorStateValue::U32(value)) => *value,
        _ => 0,
    };
    if total == 0 {
        return 0.0;
    }
    (f64::from(remaining) / f64::from(total)).clamp(0.0, 1.0)
}

fn recipes_to_godot(recipes_data: Vec<RecipeUiSnapshot>) -> VarArray {
    let mut recipes = VarArray::new();
    for recipe_data in recipes_data {
        let mut recipe = VarDictionary::new();
        recipe.set("id", recipe_data.id.as_str());
        recipe.set("label", recipe_data.label.as_str());
        recipe.set("duration_ticks", recipe_data.duration_ticks as i64);
        recipe.set("inputs", &recipe_stacks_to_godot(recipe_data.inputs));
        recipe.set("outputs", &recipe_stacks_to_godot(recipe_data.outputs));
        recipes.push(&recipe);
    }
    recipes
}

fn recipe_stacks_to_godot(stacks: Vec<ItemStackUiSnapshot>) -> VarArray {
    let mut array = VarArray::new();
    for stack_data in stacks {
        let mut stack = VarDictionary::new();
        stack.set("item", stack_data.item.as_str());
        stack.set("amount", stack_data.amount as i64);
        array.push(&stack);
    }
    array
}

fn inventories_for_ui(
    catalog: &GodotCatalogBridge,
    inventories: &[SimInventorySnapshot],
) -> Vec<InventoryUiSnapshot> {
    inventories
        .iter()
        .map(|inventory| InventoryUiSnapshot {
            role: inventory_role_name(inventory.role),
            slots: inventory
                .slots
                .iter()
                .map(|slot| {
                    slot.map(|stack| ItemStackUiSnapshot {
                        item: catalog.item_def_id(stack.kind),
                        amount: stack.amount,
                    })
                })
                .collect(),
        })
        .collect()
}

fn inventories_to_godot(inventories_data: Vec<InventoryUiSnapshot>) -> VarArray {
    let mut array = VarArray::new();
    for inventory_data in inventories_data {
        let mut row = VarDictionary::new();
        row.set("role", inventory_data.role);
        row.set("slots", &inventory_slots_to_godot(inventory_data.slots));
        array.push(&row);
    }
    array
}

fn inventory_slots_to_godot(slots: Vec<Option<ItemStackUiSnapshot>>) -> VarArray {
    let mut array = VarArray::new();
    for slot in slots {
        let mut row = VarDictionary::new();
        if let Some(stack) = slot {
            row.set("item", stack.item.as_str());
            row.set("amount", stack.amount as i64);
        } else {
            row.set("item", "");
            row.set("amount", 0_i64);
        }
        array.push(&row);
    }
    array
}

fn inventory_ui_snapshot_data(
    world: &SimWorld,
    catalog: &GodotCatalogBridge,
) -> PlayerInventoryUiSnapshot {
    PlayerInventoryUiSnapshot {
        player_slots: world
            .player_inventory_snapshot()
            .slots
            .iter()
            .map(|slot| {
                slot.map(|stack| ItemStackUiSnapshot {
                    item: catalog.item_def_id(stack.kind),
                    amount: stack.amount,
                })
            })
            .collect(),
        sections: world
            .character_container_sections()
            .into_iter()
            .map(|section| CharacterContainerUiSnapshot {
                id: section.container_id.as_str().to_string(),
                name: section.name,
                slots: section
                    .slots
                    .iter()
                    .map(|slot| {
                        slot.map(|stack| ItemStackUiSnapshot {
                            item: catalog.item_def_id(stack.kind),
                            amount: stack.amount,
                        })
                    })
                    .collect(),
                used_slots: section.used_slots,
                total_slots: section.total_slots,
                total_weight_grams: section.total_weight_grams,
                max_weight_grams: section.max_weight_grams,
                total_bulk_units: section.total_bulk_units,
                max_bulk_units: section.max_bulk_units,
            })
            .collect(),
        equipment: world
            .character_equipment()
            .into_iter()
            .map(|entry| CharacterEquipmentUiSnapshot {
                slot: entry.slot.as_str().to_string(),
                item: catalog.item_def_id(entry.item),
            })
            .collect(),
        cursor: world.cursor_stack().map(|stack| ItemStackUiSnapshot {
            item: catalog.item_def_id(stack.kind),
            amount: stack.amount,
        }),
    }
}

fn inventory_ui_snapshot_to_godot(snapshot: PlayerInventoryUiSnapshot) -> VarDictionary {
    let mut dictionary = VarDictionary::new();
    dictionary.set(
        "player_slots",
        &inventory_slots_to_godot(snapshot.player_slots),
    );
    dictionary.set("sections", &character_sections_to_godot(snapshot.sections));
    dictionary.set(
        "equipment",
        &character_equipment_to_godot(snapshot.equipment),
    );
    dictionary.set("cursor", &optional_item_stack_to_godot(snapshot.cursor));
    dictionary
}

fn character_sections_to_godot(sections_data: Vec<CharacterContainerUiSnapshot>) -> VarArray {
    let mut sections = VarArray::new();
    for section_data in sections_data {
        let mut section = VarDictionary::new();
        section.set("id", section_data.id.as_str());
        section.set("name", section_data.name.as_str());
        section.set("slots", &inventory_slots_to_godot(section_data.slots));
        section.set("used_slots", section_data.used_slots as i64);
        section.set("total_slots", section_data.total_slots as i64);
        section.set("total_weight_grams", section_data.total_weight_grams as i64);
        section.set(
            "max_weight_grams",
            section_data.max_weight_grams.map_or(0_i64, i64::from),
        );
        section.set("total_bulk_units", section_data.total_bulk_units as i64);
        section.set(
            "max_bulk_units",
            section_data.max_bulk_units.map_or(0_i64, i64::from),
        );
        sections.push(&section);
    }
    sections
}

fn character_equipment_to_godot(equipment_data: Vec<CharacterEquipmentUiSnapshot>) -> VarArray {
    let mut equipment = VarArray::new();
    for entry_data in equipment_data {
        let mut entry = VarDictionary::new();
        entry.set("slot", entry_data.slot.as_str());
        entry.set("item", entry_data.item.as_str());
        equipment.push(&entry);
    }
    equipment
}

fn optional_item_stack_to_godot(stack: Option<ItemStackUiSnapshot>) -> VarDictionary {
    let mut dictionary = VarDictionary::new();
    if let Some(stack) = stack {
        dictionary.set("item", stack.item.as_str());
        dictionary.set("amount", stack.amount as i64);
    } else {
        dictionary.set("item", "");
        dictionary.set("amount", 0_i64);
    }
    dictionary
}

fn def_behavior_role(def: &CoreBuildingDef) -> Option<&str> {
    match def.behavior.config.get("role") {
        Some(BehaviorConfigValue::String(value)) => Some(value.as_str()),
        _ => None,
    }
}

fn builtin_recipe_entries() -> Vec<RecipeCatalogEntry> {
    vec![
        recipe_entry(
            "mine_iron_ore",
            "Iron ore",
            60,
            &["basic_miner"],
            &[],
            &[("iron_ore", 1)],
        ),
        recipe_entry(
            "mine_copper_ore",
            "Copper ore",
            60,
            &["basic_miner"],
            &[],
            &[("copper_ore", 1)],
        ),
        recipe_entry(
            "mine_coal",
            "Coal",
            60,
            &["basic_miner"],
            &[],
            &[("coal", 1)],
        ),
        recipe_entry(
            "iron_plate",
            "Iron plate",
            120,
            &["stone_furnace"],
            &[("iron_ore", 1)],
            &[("iron_plate", 1)],
        ),
        recipe_entry(
            "copper_plate",
            "Copper plate",
            120,
            &["stone_furnace"],
            &[("copper_ore", 1)],
            &[("copper_plate", 1)],
        ),
        recipe_entry(
            "iron_gear",
            "Iron gear",
            30,
            &["basic_assembler"],
            &[("iron_plate", 2)],
            &[("iron_gear", 1)],
        ),
        recipe_entry(
            "copper_cable",
            "Copper cable",
            30,
            &["basic_assembler"],
            &[("copper_plate", 1)],
            &[("copper_cable", 2)],
        ),
        recipe_entry(
            "iron_stick",
            "Iron stick",
            30,
            &["basic_assembler"],
            &[("iron_plate", 1)],
            &[("iron_stick", 2)],
        ),
    ]
}

fn recipe_entry(
    id: &str,
    label: &str,
    duration_ticks: u32,
    machines: &[&str],
    inputs: &[(&str, u32)],
    outputs: &[(&str, u32)],
) -> RecipeCatalogEntry {
    RecipeCatalogEntry {
        id: id.to_string(),
        label: label.to_string(),
        duration_ticks,
        machines: machines
            .iter()
            .map(|machine| (*machine).to_string())
            .collect(),
        inputs: inputs
            .iter()
            .map(|(item, amount)| ItemStackUiSnapshot {
                item: (*item).to_string(),
                amount: *amount,
            })
            .collect(),
        outputs: outputs
            .iter()
            .map(|(item, amount)| ItemStackUiSnapshot {
                item: (*item).to_string(),
                amount: *amount,
            })
            .collect(),
    }
}

fn recipe_fallback_label(recipe_id: &str) -> String {
    let mut label = recipe_id.replace('_', " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}

fn string_field(dictionary: &VarDictionary, key: &str) -> Option<String> {
    let value = dictionary.get(key)?.try_to::<GString>().ok()?.to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn string_array_field(dictionary: &VarDictionary, key: &str) -> Vec<String> {
    let Some(value) = dictionary.get(key) else {
        return Vec::new();
    };
    let Ok(array) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    array
        .iter_shared()
        .filter_map(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn u32_field(dictionary: &VarDictionary, key: &str) -> Option<u32> {
    let value = dictionary.get(key)?;
    if let Ok(number) = value.try_to::<i64>() {
        return Some(number.clamp(0, i64::from(u32::MAX)) as u32);
    }
    value
        .try_to::<f64>()
        .ok()
        .map(|number| number.clamp(0.0, f64::from(u32::MAX)).round() as u32)
}

fn f64_field(dictionary: &VarDictionary, key: &str) -> Option<f64> {
    let value = dictionary.get(key)?;
    if let Ok(number) = value.try_to::<f64>() {
        return Some(number);
    }
    value.try_to::<i64>().ok().map(|number| number as f64)
}

fn stack_rows_field(dictionary: &VarDictionary, key: &str) -> Vec<ItemStackUiSnapshot> {
    let Some(value) = dictionary.get(key) else {
        return Vec::new();
    };
    let Ok(array) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    array
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .filter_map(|row| {
            let item = string_field(&row, "kind").or_else(|| string_field(&row, "item"))?;
            let amount = u32_field(&row, "amount").unwrap_or(1).max(1);
            Some(ItemStackUiSnapshot { item, amount })
        })
        .collect()
}

fn inventory_role_name(role: CoreInventoryRole) -> &'static str {
    match role {
        CoreInventoryRole::Input => "Input",
        CoreInventoryRole::Output => "Output",
        CoreInventoryRole::Fuel => "Fuel",
        CoreInventoryRole::Storage => "Storage",
        CoreInventoryRole::InserterHand => "Hand",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog_bridge(world: &SimWorld) -> GodotCatalogBridge {
        GodotCatalogBridge::from_core_catalog(world.catalog())
    }

    #[test]
    fn chunk_tile_bounds_use_core_chunk_size_for_negative_chunks() {
        assert_eq!(chunk_min_tile(ChunkPos::new(0, 0)), TilePos::new(0, 0));
        assert_eq!(chunk_max_tile(ChunkPos::new(0, 0)), TilePos::new(31, 31));
        assert_eq!(
            chunk_min_tile(ChunkPos::new(-1, -2)),
            TilePos::new(-32, -64)
        );
        assert_eq!(chunk_max_tile(ChunkPos::new(-1, -2)), TilePos::new(-1, -33));
    }

    #[test]
    fn placement_bridge_places_building_and_rejects_overlap() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());

        assert!(can_place_building_for_godot(
            &world,
            "wooden_chest",
            0,
            0,
            0
        ));
        assert!(place_building_for_godot(
            &mut world,
            "wooden_chest",
            0,
            0,
            0
        ));

        assert_eq!(world.building_snapshots().len(), 1);
        assert!(!can_place_building_for_godot(
            &world,
            "wooden_chest",
            0,
            0,
            0
        ));
    }

    #[test]
    fn placement_bridge_removes_building_by_occupied_tile() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());

        assert!(place_building_for_godot(
            &mut world,
            "stone_furnace",
            10,
            20,
            0
        ));
        assert_eq!(world.building_snapshots().len(), 1);

        assert!(remove_building_for_godot(&mut world, 11, 20));
        assert!(world.building_snapshots().is_empty());
    }

    #[test]
    fn footprint_bridge_uses_core_catalog_rotation_rules() {
        let world = SimWorld::with_catalog(CoreCatalog::for_tests());

        assert_eq!(
            building_footprint_for_godot(&world, "basic_splitter", 10, 20, 0),
            vec![(10, 20), (10, 21)]
        );
        assert_eq!(
            building_footprint_for_godot(&world, "basic_splitter", 10, 20, 1),
            vec![(10, 20), (11, 20)]
        );
    }

    #[test]
    fn placement_bridge_rejects_single_underground_endpoint() {
        let world = SimWorld::with_catalog(CoreCatalog::for_tests());

        assert!(!can_place_building_for_godot(
            &world,
            "basic_underground_belt",
            0,
            0,
            0
        ));
    }

    #[test]
    fn footprint_bridge_uses_neptune_miner_footprint() {
        let world = SimWorld::with_catalog(CoreCatalog::for_tests());

        assert_eq!(
            building_footprint_for_godot(&world, "basic_miner", 10, 20, 0),
            vec![(10, 20), (10, 21), (11, 20), (11, 21)]
        );
    }

    #[test]
    fn render_bridge_names_directions_for_godot() {
        assert_eq!(direction_name(Direction::East), "east");
        assert_eq!(direction_name(Direction::North), "north");
        assert_eq!(direction_name(Direction::West), "west");
        assert_eq!(direction_name(Direction::South), "south");
    }

    #[test]
    fn render_bridge_converts_belt_speed_to_tiles_per_second() {
        let world = SimWorld::with_catalog(CoreCatalog::for_tests());

        assert!((belt_speed_tiles_per_second(&world, "basic_belt") - 0.9375).abs() < f64::EPSILON);
        assert!(
            (belt_speed_tiles_per_second(&world, "accelerated_belt") - 1.40625).abs()
                < f64::EPSILON
        );
        assert!((belt_speed_tiles_per_second(&world, "fast_belt") - 1.875).abs() < f64::EPSILON);
    }

    #[test]
    fn placement_bridge_requires_miner_on_resource() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());

        assert!(!can_place_building_for_godot(
            &world,
            "basic_miner",
            10,
            20,
            0
        ));

        world.seed_resource_for_tests(TilePos::new(10, 20), sim_core::catalog::TEST_IRON_ORE, 10);

        assert!(can_place_building_for_godot(
            &world,
            "basic_miner",
            10,
            20,
            0
        ));
    }

    #[test]
    fn machine_ui_snapshot_exposes_processor_recipes_and_slots() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());
        assert!(place_building_for_godot(
            &mut world,
            "stone_furnace",
            10,
            20,
            0
        ));
        let building_id = world.building_snapshots()[0].id;

        let snapshot = building_ui_snapshot_data(
            &world,
            &catalog_bridge(&world),
            &BTreeMap::new(),
            building_id,
        )
        .unwrap();

        assert_eq!(snapshot.ui_kind, "machine");
        assert_eq!(snapshot.recipe_grid_visible, true);
        assert_eq!(snapshot.active_recipe, None);
        assert_eq!(snapshot.process_progress, 0.0);
        assert_eq!(snapshot.recipes.len(), 2);
        assert_eq!(snapshot.inventories.len(), 3);
    }

    #[test]
    fn machine_ui_snapshot_exposes_extractor_active_recipe() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());
        world.seed_resource_for_tests(TilePos::new(10, 20), sim_core::catalog::TEST_IRON_ORE, 10);
        assert!(place_building_for_godot(
            &mut world,
            "basic_miner",
            10,
            20,
            0
        ));
        let building_id = world.building_snapshots()[0].id;

        let snapshot = building_ui_snapshot_data(
            &world,
            &catalog_bridge(&world),
            &BTreeMap::new(),
            building_id,
        )
        .unwrap();

        assert_eq!(snapshot.ui_kind, "machine");
        assert_eq!(snapshot.active_recipe.as_deref(), Some("mine_iron_ore"));
        assert_eq!(snapshot.recipe_grid_visible, false);
        assert_eq!(snapshot.recipes.len(), 3);
    }

    #[test]
    fn recipe_selection_override_updates_machine_ui_snapshot() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());
        assert!(place_building_for_godot(
            &mut world,
            "stone_furnace",
            10,
            20,
            0
        ));
        let building_id = world.building_snapshots()[0].id;
        let selected = BTreeMap::from([(building_id, "copper_plate".to_string())]);

        let snapshot =
            building_ui_snapshot_data(&world, &catalog_bridge(&world), &selected, building_id)
                .unwrap();

        assert_eq!(snapshot.active_recipe.as_deref(), Some("copper_plate"));
        assert_eq!(snapshot.recipe_grid_visible, false);
    }

    #[test]
    fn inventory_ui_snapshot_exposes_player_slots_and_cursor() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());
        assert!(
            world
                .insert_into_player_inventory_for_tests(sim_core::catalog::CoreItemStack {
                    kind: sim_core::catalog::TEST_IRON_ORE,
                    amount: 7,
                })
                .is_ok()
        );
        assert!(
            world
                .set_cursor_stack_for_tests(sim_core::catalog::CoreItemStack {
                    kind: sim_core::catalog::TEST_COAL,
                    amount: 3,
                })
                .is_ok()
        );

        let snapshot = inventory_ui_snapshot_data(&world, &catalog_bridge(&world));

        assert_eq!(snapshot.player_slots.len(), 80);
        assert_eq!(
            snapshot.player_slots[0],
            Some(ItemStackUiSnapshot {
                item: "iron_ore".to_string(),
                amount: 7
            })
        );
        assert_eq!(
            snapshot.cursor,
            Some(ItemStackUiSnapshot {
                item: "coal".to_string(),
                amount: 3
            })
        );
    }

    #[test]
    fn give_item_bridge_inserts_known_item_into_player_inventory() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());

        assert!(give_item_for_godot(&mut world, "iron_ore", 5));
        assert!(!give_item_for_godot(&mut world, "missing_item", 5));

        let snapshot = inventory_ui_snapshot_data(&world, &catalog_bridge(&world));
        assert_eq!(
            snapshot.player_slots[0],
            Some(ItemStackUiSnapshot {
                item: "iron_ore".to_string(),
                amount: 5
            })
        );
    }

    #[test]
    fn inventory_transfer_bridge_moves_player_stack_to_machine_fuel_slot() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());
        assert!(place_building_for_godot(
            &mut world,
            "stone_furnace",
            10,
            20,
            0
        ));
        let building_id = world.building_snapshots()[0].id;
        assert_eq!(
            world
                .insert_into_player_inventory_slot(
                    0,
                    CoreItemStack {
                        kind: sim_core::catalog::TEST_COAL,
                        amount: 5,
                    },
                    InsertMode::AtomicAllOrNothing,
                )
                .rejected,
            None
        );

        assert!(transfer_inventory_slot_for_godot(
            &mut world,
            &UiInventorySlotRef::Player { slot: 0 },
            &UiInventorySlotRef::Building {
                building_id,
                role: CoreInventoryRole::Fuel,
                slot: 0,
            },
            5,
        ));

        assert_eq!(world.player_inventory_slot(0), None);
        assert_eq!(
            world.inventory_slot(building_id, CoreInventoryRole::Fuel, 0),
            Some(CoreItemStack {
                kind: sim_core::catalog::TEST_COAL,
                amount: 5,
            })
        );
    }

    #[test]
    fn inventory_transfer_bridge_rejects_invalid_machine_slot_without_losing_player_stack() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());
        assert!(place_building_for_godot(
            &mut world,
            "stone_furnace",
            10,
            20,
            0
        ));
        let building_id = world.building_snapshots()[0].id;
        let plate = CoreItemStack {
            kind: sim_core::catalog::TEST_IRON_PLATE,
            amount: 5,
        };
        assert_eq!(
            world
                .insert_into_player_inventory_slot(0, plate, InsertMode::AtomicAllOrNothing,)
                .rejected,
            None
        );

        assert!(!transfer_inventory_slot_for_godot(
            &mut world,
            &UiInventorySlotRef::Player { slot: 0 },
            &UiInventorySlotRef::Building {
                building_id,
                role: CoreInventoryRole::Fuel,
                slot: 0,
            },
            5,
        ));

        assert_eq!(world.player_inventory_slot(0), Some(plate));
        assert_eq!(
            world.inventory_slot(building_id, CoreInventoryRole::Fuel, 0),
            None
        );
    }

    #[test]
    fn inventory_transfer_bridge_moves_between_player_slots() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());
        let ore = CoreItemStack {
            kind: sim_core::catalog::TEST_IRON_ORE,
            amount: 7,
        };
        assert_eq!(
            world
                .insert_into_player_inventory_slot(0, ore, InsertMode::AtomicAllOrNothing,)
                .rejected,
            None
        );

        assert!(transfer_inventory_slot_for_godot(
            &mut world,
            &UiInventorySlotRef::Player { slot: 0 },
            &UiInventorySlotRef::Player { slot: 1 },
            7,
        ));

        assert_eq!(world.player_inventory_slot(0), None);
        assert_eq!(world.player_inventory_slot(1), Some(ore));
    }

    #[test]
    fn inventory_click_bridge_moves_stack_through_cursor() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());
        let ore = CoreItemStack {
            kind: sim_core::catalog::TEST_IRON_ORE,
            amount: 7,
        };
        assert_eq!(
            world
                .insert_into_player_inventory_slot(0, ore, InsertMode::AtomicAllOrNothing)
                .rejected,
            None
        );

        assert!(click_inventory_slot_for_godot(
            &mut world,
            &UiInventorySlotRef::Player { slot: 0 },
            "stack",
        ));
        assert_eq!(world.player_inventory_slot(0), None);
        assert_eq!(world.cursor_stack(), Some(ore));

        assert!(click_inventory_slot_for_godot(
            &mut world,
            &UiInventorySlotRef::Player { slot: 1 },
            "stack",
        ));
        assert_eq!(world.cursor_stack(), None);
        assert_eq!(world.player_inventory_slot(1), Some(ore));
    }

    #[test]
    fn inventory_click_bridge_right_click_moves_one_item() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());
        assert_eq!(
            world
                .insert_into_player_inventory_slot(
                    0,
                    CoreItemStack {
                        kind: sim_core::catalog::TEST_IRON_ORE,
                        amount: 5,
                    },
                    InsertMode::AtomicAllOrNothing,
                )
                .rejected,
            None
        );

        assert!(click_inventory_slot_for_godot(
            &mut world,
            &UiInventorySlotRef::Player { slot: 0 },
            "one",
        ));
        assert_eq!(
            world.player_inventory_slot(0),
            Some(CoreItemStack {
                kind: sim_core::catalog::TEST_IRON_ORE,
                amount: 4,
            })
        );
        assert_eq!(
            world.cursor_stack(),
            Some(CoreItemStack {
                kind: sim_core::catalog::TEST_IRON_ORE,
                amount: 1,
            })
        );

        assert!(click_inventory_slot_for_godot(
            &mut world,
            &UiInventorySlotRef::Player { slot: 1 },
            "one",
        ));
        assert_eq!(world.cursor_stack(), None);
        assert_eq!(
            world.player_inventory_slot(1),
            Some(CoreItemStack {
                kind: sim_core::catalog::TEST_IRON_ORE,
                amount: 1,
            })
        );
    }

    #[test]
    fn inventory_click_bridge_ctrl_click_splits_half_to_cursor() {
        let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());
        assert_eq!(
            world
                .insert_into_player_inventory_slot(
                    0,
                    CoreItemStack {
                        kind: sim_core::catalog::TEST_IRON_ORE,
                        amount: 5,
                    },
                    InsertMode::AtomicAllOrNothing,
                )
                .rejected,
            None
        );

        assert!(click_inventory_slot_for_godot(
            &mut world,
            &UiInventorySlotRef::Player { slot: 0 },
            "split",
        ));

        assert_eq!(
            world.player_inventory_slot(0),
            Some(CoreItemStack {
                kind: sim_core::catalog::TEST_IRON_ORE,
                amount: 3,
            })
        );
        assert_eq!(
            world.cursor_stack(),
            Some(CoreItemStack {
                kind: sim_core::catalog::TEST_IRON_ORE,
                amount: 2,
            })
        );
    }
}
