use behavior_api::{BehaviorConfigValue, BehaviorStateValue};
use godot::prelude::*;
use sim_core::building::{SimBuildingSnapshot, SimBuildingState};
use sim_core::catalog::CoreBuildingDriver;
use sim_core::catalog::CoreCatalog;
use sim_core::catalog::{CoreBuildingDef, CoreBuildingKind, CoreInventoryRole};
use sim_core::command::SimCommand;
use sim_core::ids::BuildingId;
use sim_core::ids::TilePos;
use sim_core::inventory::SimInventorySnapshot;
use sim_core::topology::graph::Direction;
use sim_core::units::DistanceUnits;
use sim_core::world::SimWorld;
use sim_core::worldgen::{DEFAULT_WORLD_SEED, WorldGenerator};
use std::collections::BTreeMap;

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
    id: &'static str,
    label: &'static str,
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
    item: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ItemStackUiSnapshot {
    item: &'static str,
    amount: u32,
}

struct NeptuneGodotExtension;

#[gdextension]
unsafe impl ExtensionLibrary for NeptuneGodotExtension {}

#[derive(GodotClass)]
#[class(base=RefCounted)]
pub struct NeptuneSim {
    world: SimWorld,
    map_min: TilePos,
    map_max: TilePos,
    selected_recipes: BTreeMap<BuildingId, String>,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for NeptuneSim {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            world: SimWorld::with_catalog(CoreCatalog::for_tests()),
            map_min: TilePos::new(0, 0),
            map_max: TilePos::new(0, 0),
            selected_recipes: BTreeMap::new(),
            base,
        }
    }
}

#[godot_api]
impl NeptuneSim {
    #[func]
    pub fn tick(&mut self) {
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
        let generator = WorldGenerator::new_default(DEFAULT_WORLD_SEED);
        let generated = generator.generate_square_around_spawn(radius);
        self.map_min = generated.min;
        self.map_max = generated.max;
        if let Err(error) = self.world.apply_generated_region(&generated) {
            godot_error!("failed to apply generated world region: {error}");
        }
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
        let mut resources = self
            .world
            .resource_tiles_in_rect(self.map_min, self.map_max)
            .into_iter()
            .map(|(pos, item, amount)| (pos, (item, amount)))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut tiles = VarArray::new();
        for (pos, terrain) in self.world.terrain_tiles_in_rect(self.map_min, self.map_max) {
            let mut tile = VarDictionary::new();
            tile.set("x", pos.x);
            tile.set("y", pos.y);
            tile.set("terrain", terrain);
            if let Some((resource, amount)) = resources.remove(&pos) {
                tile.set("resource", resource);
                tile.set("amount", amount as i64);
            } else {
                tile.set("resource", "");
                tile.set("amount", 0_i64);
            }
            tiles.push(&tile);
        }
        tiles
    }

    #[func]
    pub fn buildings(&self) -> VarArray {
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
        building_ui_snapshot_for_godot(
            &self.world,
            &self.selected_recipes,
            BuildingId(building_id.max(0) as u32),
        )
        .unwrap_or_else(VarDictionary::new)
    }

    #[func]
    pub fn inventory_snapshot(&self) -> VarDictionary {
        inventory_ui_snapshot_to_godot(inventory_ui_snapshot_data(&self.world))
    }

    #[func]
    pub fn set_building_recipe(&mut self, building_id: i64, recipe_id: GString) -> bool {
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
        let valid = recipe_ids_for_building(&snapshot.def_id)
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
        self.world = SimWorld::with_catalog(CoreCatalog::for_tests());
        self.map_min = TilePos::new(0, 0);
        self.map_max = TilePos::new(0, 0);
        self.selected_recipes.clear();
    }
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
    selected_recipes: &BTreeMap<BuildingId, String>,
    building_id: BuildingId,
) -> Option<VarDictionary> {
    building_ui_snapshot_data(world, selected_recipes, building_id)
        .map(machine_ui_snapshot_to_godot)
}

fn building_ui_snapshot_data(
    world: &SimWorld,
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

    let active_recipe = active_recipe_for_snapshot(def, &snapshot, selected_recipes);
    Some(MachineUiSnapshot {
        id: snapshot.id,
        def_id: snapshot.def_id.clone(),
        ui_kind,
        status: behavior_status_for_snapshot(&snapshot),
        recipe_selector_visible: def_behavior_role(def) == Some("processor")
            && recipe_ids_for_building(&snapshot.def_id).len() > 1,
        recipe_grid_visible: def_behavior_role(def) == Some("processor") && active_recipe.is_none(),
        process_progress: process_progress_for_snapshot(&snapshot, active_recipe.as_deref()),
        fuel_progress: fuel_progress_for_snapshot(&snapshot),
        recipes: recipes_for_ui(&snapshot.def_id),
        inventories: inventories_for_ui(&snapshot.inventories),
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
    let recipes = recipe_ids_for_building(&snapshot.def_id);
    match (def_behavior_role(def), recipes.as_slice()) {
        (Some("extractor"), [first, ..]) => Some((*first).to_string()),
        (_, [only]) => Some((*only).to_string()),
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
    snapshot: &SimBuildingSnapshot,
    active_recipe: Option<&str>,
) -> f64 {
    let Some(active_recipe) = active_recipe else {
        return 0.0;
    };
    let duration = recipe_duration_ticks(active_recipe).max(1);
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

fn recipes_for_ui(def_id: &str) -> Vec<RecipeUiSnapshot> {
    recipe_ids_for_building(def_id)
        .into_iter()
        .map(|recipe_id| RecipeUiSnapshot {
            id: recipe_id,
            label: recipe_label(recipe_id),
            duration_ticks: recipe_duration_ticks(recipe_id),
            inputs: recipe_inputs(recipe_id)
                .into_iter()
                .map(|(item, amount)| ItemStackUiSnapshot { item, amount })
                .collect(),
            outputs: recipe_outputs(recipe_id)
                .into_iter()
                .map(|(item, amount)| ItemStackUiSnapshot { item, amount })
                .collect(),
        })
        .collect()
}

fn recipes_to_godot(recipes_data: Vec<RecipeUiSnapshot>) -> VarArray {
    let mut recipes = VarArray::new();
    for recipe_data in recipes_data {
        let mut recipe = VarDictionary::new();
        recipe.set("id", recipe_data.id);
        recipe.set("label", recipe_data.label);
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
        stack.set("item", stack_data.item);
        stack.set("amount", stack_data.amount as i64);
        array.push(&stack);
    }
    array
}

fn inventories_for_ui(inventories: &[SimInventorySnapshot]) -> Vec<InventoryUiSnapshot> {
    inventories
        .iter()
        .map(|inventory| InventoryUiSnapshot {
            role: inventory_role_name(inventory.role),
            slots: inventory
                .slots
                .iter()
                .map(|slot| {
                    slot.map(|stack| ItemStackUiSnapshot {
                        item: item_def_id(stack.kind),
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
            row.set("item", stack.item);
            row.set("amount", stack.amount as i64);
        } else {
            row.set("item", "");
            row.set("amount", 0_i64);
        }
        array.push(&row);
    }
    array
}

fn inventory_ui_snapshot_data(world: &SimWorld) -> PlayerInventoryUiSnapshot {
    PlayerInventoryUiSnapshot {
        player_slots: world
            .player_inventory_snapshot()
            .slots
            .iter()
            .map(|slot| {
                slot.map(|stack| ItemStackUiSnapshot {
                    item: item_def_id(stack.kind),
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
                            item: item_def_id(stack.kind),
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
                item: item_def_id(entry.item),
            })
            .collect(),
        cursor: world.cursor_stack().map(|stack| ItemStackUiSnapshot {
            item: item_def_id(stack.kind),
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
        entry.set("item", entry_data.item);
        equipment.push(&entry);
    }
    equipment
}

fn optional_item_stack_to_godot(stack: Option<ItemStackUiSnapshot>) -> VarDictionary {
    let mut dictionary = VarDictionary::new();
    if let Some(stack) = stack {
        dictionary.set("item", stack.item);
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

fn recipe_ids_for_building(def_id: &str) -> Vec<&'static str> {
    match def_id {
        "basic_miner" => vec!["mine_iron_ore", "mine_copper_ore", "mine_coal"],
        "stone_furnace" => vec!["iron_plate", "copper_plate"],
        "basic_assembler" => vec!["iron_gear", "copper_cable", "iron_stick"],
        _ => Vec::new(),
    }
}

fn recipe_label(recipe_id: &str) -> &'static str {
    match recipe_id {
        "mine_iron_ore" => "Iron ore",
        "mine_copper_ore" => "Copper ore",
        "mine_coal" => "Coal",
        "iron_plate" => "Iron plate",
        "copper_plate" => "Copper plate",
        "iron_gear" => "Iron gear",
        "copper_cable" => "Copper cable",
        "iron_stick" => "Iron stick",
        _ => "Recipe",
    }
}

fn recipe_duration_ticks(recipe_id: &str) -> u32 {
    match recipe_id {
        "mine_iron_ore" | "mine_copper_ore" | "mine_coal" => 60,
        "iron_plate" | "copper_plate" => 120,
        "iron_gear" | "copper_cable" | "iron_stick" => 30,
        _ => 1,
    }
}

fn recipe_inputs(recipe_id: &str) -> Vec<(&'static str, u32)> {
    match recipe_id {
        "iron_plate" => vec![("iron_ore", 1)],
        "copper_plate" => vec![("copper_ore", 1)],
        "iron_gear" => vec![("iron_plate", 2)],
        "copper_cable" => vec![("copper_plate", 1)],
        "iron_stick" => vec![("iron_plate", 1)],
        _ => Vec::new(),
    }
}

fn recipe_outputs(recipe_id: &str) -> Vec<(&'static str, u32)> {
    match recipe_id {
        "mine_iron_ore" => vec![("iron_ore", 1)],
        "mine_copper_ore" => vec![("copper_ore", 1)],
        "mine_coal" => vec![("coal", 1)],
        "iron_plate" => vec![("iron_plate", 1)],
        "copper_plate" => vec![("copper_plate", 1)],
        "iron_gear" => vec![("iron_gear", 1)],
        "copper_cable" => vec![("copper_cable", 2)],
        "iron_stick" => vec![("iron_stick", 2)],
        _ => Vec::new(),
    }
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

fn item_def_id(kind: sim_core::ids::ItemKindId) -> &'static str {
    match kind {
        sim_core::catalog::TEST_IRON_ORE => "iron_ore",
        sim_core::catalog::TEST_COPPER_ORE => "copper_ore",
        sim_core::catalog::TEST_IRON_PLATE => "iron_plate",
        sim_core::catalog::TEST_COPPER_PLATE => "copper_plate",
        sim_core::catalog::TEST_IRON_GEAR => "iron_gear",
        sim_core::catalog::TEST_COPPER_CABLE => "copper_cable",
        sim_core::catalog::TEST_IRON_STICK => "iron_stick",
        sim_core::catalog::TEST_COAL => "coal",
        sim_core::catalog::TEST_WOOD => "wood",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let snapshot = building_ui_snapshot_data(&world, &BTreeMap::new(), building_id).unwrap();

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

        let snapshot = building_ui_snapshot_data(&world, &BTreeMap::new(), building_id).unwrap();

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

        let snapshot = building_ui_snapshot_data(&world, &selected, building_id).unwrap();

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

        let snapshot = inventory_ui_snapshot_data(&world);

        assert_eq!(snapshot.player_slots.len(), 80);
        assert_eq!(
            snapshot.player_slots[0],
            Some(ItemStackUiSnapshot {
                item: "iron_ore",
                amount: 7
            })
        );
        assert_eq!(
            snapshot.cursor,
            Some(ItemStackUiSnapshot {
                item: "coal",
                amount: 3
            })
        );
    }
}
