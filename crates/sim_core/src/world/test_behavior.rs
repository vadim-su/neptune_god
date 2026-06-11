//! Test-only behavior host so `sim_core` tests do not depend on `vanilla_behavior`.

use std::collections::BTreeMap;

use behavior_api::{
    BehaviorBuildingContext, BehaviorCatalog, BehaviorCommand, BehaviorCommandInput,
    BehaviorCommandOutput, BehaviorConfigValue, BehaviorEffect, BehaviorHost, BehaviorHostResult,
    BehaviorId, BehaviorInitInput, BehaviorInstanceState, BehaviorInventory, BehaviorInventoryRole,
    BehaviorItemStack, BehaviorRecipeDef, BehaviorRecipeKind, BehaviorResource, BehaviorStateValue,
    BehaviorStatus, BehaviorTickInput, BehaviorTickMetrics, BehaviorTickOutput, BehaviorTilePos,
};
use serde::{Deserialize, Serialize};

pub const TEST_MACHINE_BEHAVIOR_ID: &str = "test:behavior";
pub const SET_RECIPE_COMMAND: &str = "set_recipe";

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum MachineStatus {
    #[default]
    Idle,
    Working,
    NoRecipeSelected,
    NoMatchingResource,
    MissingInput,
    MissingFuel,
    OutputBlocked,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct MachineRuntime {
    pub active_recipe: Option<String>,
    pub progress_ticks: u32,
    pub status: MachineStatus,
    pub fuel_remaining_ticks: u32,
    pub fuel_total_ticks: u32,
    pub fuel_temperature: u32,
    pub pending_outputs: Vec<BehaviorItemStack>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FuelBurn {
    pub remaining_ticks: u32,
    pub temperature: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuelAvailability {
    Available { consumed: Option<BehaviorItemStack> },
    Missing,
}

pub fn ensure_fuel(
    catalog: &BehaviorCatalog,
    recipe: &BehaviorRecipeDef,
    fuel_inventory: &mut BehaviorInventory,
    burn: &mut FuelBurn,
) -> FuelAvailability {
    let Some(energy) = recipe.energy else {
        return FuelAvailability::Available { consumed: None };
    };
    if burn.remaining_ticks > 0 && burn.temperature as f32 >= energy.min_temperature {
        return FuelAvailability::Available { consumed: None };
    }
    let Some(stack) = fuel_inventory.take_first_matching(|kind| {
        catalog
            .item(kind)
            .and_then(|item| item.fuel)
            .is_some_and(|fuel| fuel.burn_temperature >= energy.min_temperature)
    }) else {
        return FuelAvailability::Missing;
    };
    let fuel = catalog.item(stack.kind).and_then(|item| item.fuel).unwrap();
    burn.remaining_ticks = (fuel.energy / energy.required_per_second * 60.0).round() as u32;
    burn.temperature = fuel.burn_temperature.round() as u32;
    FuelAvailability::Available {
        consumed: Some(stack),
    }
}

pub fn tick_fuel(burn: &mut FuelBurn) {
    if burn.remaining_ticks > 0 {
        burn.remaining_ticks -= 1;
    }
    if burn.remaining_ticks == 0 {
        burn.temperature = 0;
    }
}

pub fn can_add_all_outputs(
    outputs: &[BehaviorItemStack],
    output_inventory: &BehaviorInventory,
) -> bool {
    let mut clone = output_inventory.clone();
    outputs
        .iter()
        .copied()
        .all(|stack| clone.add(stack).is_none())
}

pub fn missing_inputs(
    required: &[BehaviorItemStack],
    input_inventory: Option<&BehaviorInventory>,
) -> bool {
    if required.is_empty() {
        return false;
    }
    let Some(input_inventory) = input_inventory else {
        return true;
    };
    required
        .iter()
        .any(|stack| input_inventory.count(stack.kind) < stack.amount)
}

pub fn set_recipe_command(recipe: Option<String>) -> BehaviorCommand {
    let mut command = BehaviorCommand::new(SET_RECIPE_COMMAND);
    if let Some(recipe) = recipe {
        command
            .data
            .insert("recipe".to_string(), BehaviorStateValue::String(recipe));
    }
    command
}

pub fn test_machine_behavior_state(machine: MachineRuntime) -> BehaviorInstanceState {
    let mut data = BTreeMap::new();
    if let Some(active_recipe) = machine.active_recipe {
        data.insert(
            "active_recipe".to_string(),
            BehaviorStateValue::String(active_recipe),
        );
    }
    data.insert(
        "progress_ticks".to_string(),
        BehaviorStateValue::U32(machine.progress_ticks),
    );
    data.insert(
        "fuel_remaining_ticks".to_string(),
        BehaviorStateValue::U32(machine.fuel_remaining_ticks),
    );
    data.insert(
        "fuel_total_ticks".to_string(),
        BehaviorStateValue::U32(machine.fuel_total_ticks),
    );
    data.insert(
        "fuel_temperature".to_string(),
        BehaviorStateValue::U32(machine.fuel_temperature),
    );
    data.insert(
        "pending_outputs".to_string(),
        BehaviorStateValue::ItemStacks(machine.pending_outputs),
    );

    BehaviorInstanceState {
        behavior_id: BehaviorId::new(TEST_MACHINE_BEHAVIOR_ID),
        status: BehaviorStatus::new(machine_status_name(machine.status)),
        data,
    }
}

pub fn test_machine_runtime(state: &BehaviorInstanceState) -> Option<MachineRuntime> {
    if state.behavior_id.as_str() != TEST_MACHINE_BEHAVIOR_ID {
        return None;
    }

    Some(MachineRuntime {
        active_recipe: state
            .data
            .get("active_recipe")
            .and_then(string_value)
            .map(str::to_string),
        progress_ticks: state
            .data
            .get("progress_ticks")
            .and_then(u32_value)
            .unwrap_or_default(),
        status: machine_status_from_name(state.status.as_str())?,
        fuel_remaining_ticks: state
            .data
            .get("fuel_remaining_ticks")
            .and_then(u32_value)
            .unwrap_or_default(),
        fuel_total_ticks: state
            .data
            .get("fuel_total_ticks")
            .and_then(u32_value)
            .unwrap_or_default(),
        fuel_temperature: state
            .data
            .get("fuel_temperature")
            .and_then(u32_value)
            .unwrap_or_default(),
        pending_outputs: state
            .data
            .get("pending_outputs")
            .and_then(item_stacks_value)
            .unwrap_or_default(),
    })
}

pub fn initial_machine_behavior_state(
    config: &BTreeMap<String, BehaviorConfigValue>,
) -> BehaviorInstanceState {
    let recipes = config_recipes(config);
    let role = config_role(config);
    test_machine_behavior_state(MachineRuntime {
        active_recipe: initial_active_recipe(role, &recipes),
        progress_ticks: 0,
        status: if recipes.is_empty() || (role == Some("processor") && recipes.len() > 1) {
            MachineStatus::NoRecipeSelected
        } else {
            MachineStatus::Idle
        },
        fuel_remaining_ticks: 0,
        fuel_total_ticks: 0,
        fuel_temperature: 0,
        pending_outputs: Vec::new(),
    })
}

pub fn tick_machine_behavior(input: BehaviorTickInput<'_>) -> BehaviorTickOutput {
    let BehaviorTickInput {
        catalog,
        building,
        config,
        state,
        mut inventories,
        resources,
        power: _,
        tick,
    } = input;
    let mut machine = test_machine_runtime(&state).unwrap_or(MachineRuntime {
        active_recipe: None,
        progress_ticks: 0,
        status: MachineStatus::NoRecipeSelected,
        fuel_remaining_ticks: 0,
        fuel_total_ticks: 0,
        fuel_temperature: 0,
        pending_outputs: Vec::new(),
    });
    let mut metrics = BehaviorTickMetrics::default();
    let mut output_changed = false;
    let mut effects = Vec::new();

    let Some(mut recipe) =
        selected_recipe(catalog, building, config, resources, tick, &machine, None)
    else {
        machine.status = if machine.active_recipe.is_none() {
            MachineStatus::NoRecipeSelected
        } else if has_matching_resource(catalog, building, config, resources) {
            MachineStatus::OutputBlocked
        } else {
            MachineStatus::NoMatchingResource
        };
        return machine_tick_output(machine, metrics, output_changed, effects);
    };
    machine.active_recipe = Some(recipe.0.id.clone());
    let extraction_tile = recipe.1;
    let recipe_def = &mut recipe.0;

    let input_index = inventories
        .iter()
        .position(|inventory| inventory.role == BehaviorInventoryRole::Input);
    let output_index = inventories
        .iter()
        .position(|inventory| inventory.role == BehaviorInventoryRole::Output);
    let fuel_index = inventories
        .iter()
        .position(|inventory| inventory.role == BehaviorInventoryRole::Fuel);

    if recipe_outputs_blocked(
        &recipe_def.outputs,
        output_index.map(|index| &inventories[index]),
    ) {
        metrics.blocked_outputs += 1;
        machine.status = MachineStatus::OutputBlocked;
        return machine_tick_output(machine, metrics, output_changed, effects);
    }

    if machine.progress_ticks >= recipe_def.duration_ticks {
        if let Some(index) = output_index {
            for output in &recipe_def.outputs {
                inventories[index].add(*output);
                effects.push(BehaviorEffect::InsertInventory {
                    role: BehaviorInventoryRole::Output,
                    stack: *output,
                });
                metrics.inventory_transfers += 1;
            }
        }
        machine.progress_ticks = 0;
        machine.status = MachineStatus::Idle;
        output_changed = true;
        return machine_tick_output(machine, metrics, output_changed, effects);
    }

    if machine.progress_ticks == 0
        && missing_inputs(
            &recipe_def.inputs,
            input_index.map(|index| &inventories[index]),
        )
    {
        machine.status = MachineStatus::MissingInput;
        return machine_tick_output(machine, metrics, output_changed, effects);
    }

    let mut burn = FuelBurn {
        remaining_ticks: machine.fuel_remaining_ticks,
        temperature: machine.fuel_temperature,
    };
    let previous_fuel_remaining_ticks = burn.remaining_ticks;
    let fuel_result = match fuel_index {
        Some(index) => ensure_fuel(catalog, recipe_def, &mut inventories[index], &mut burn),
        None if recipe_def.energy.is_none() => FuelAvailability::Available { consumed: None },
        None => FuelAvailability::Missing,
    };
    if let FuelAvailability::Available {
        consumed: Some(stack),
    } = fuel_result
    {
        effects.push(BehaviorEffect::TakeInventory {
            role: BehaviorInventoryRole::Fuel,
            stack,
        });
    }
    machine.fuel_remaining_ticks = burn.remaining_ticks;
    machine.fuel_temperature = burn.temperature;
    if previous_fuel_remaining_ticks == 0 && burn.remaining_ticks > 0 {
        machine.fuel_total_ticks = burn.remaining_ticks;
    }
    if fuel_result == FuelAvailability::Missing {
        metrics.fuel_starved_behaviors += 1;
        machine.status = MachineStatus::MissingFuel;
        return machine_tick_output(machine, metrics, output_changed, effects);
    }

    if machine.progress_ticks == 0 {
        if let Some(index) = input_index {
            for input in &recipe_def.inputs {
                inventories[index].remove(*input);
                effects.push(BehaviorEffect::TakeInventory {
                    role: BehaviorInventoryRole::Input,
                    stack: *input,
                });
                metrics.inventory_transfers += 1;
            }
        }
        if let Some(pos) = extraction_tile {
            effects.push(BehaviorEffect::DepleteResource { pos });
        }
    }

    if machine.progress_ticks < recipe_def.duration_ticks {
        tick_fuel(&mut burn);
        machine.fuel_remaining_ticks = burn.remaining_ticks;
        machine.fuel_temperature = burn.temperature;
        if burn.remaining_ticks == 0 {
            machine.fuel_total_ticks = 0;
        }
        machine.progress_ticks += 1;
        machine.status = MachineStatus::Working;
        metrics.active_behaviors += 1;
    }

    if machine.progress_ticks >= recipe_def.duration_ticks {
        if let Some(index) = output_index {
            for output in &recipe_def.outputs {
                inventories[index].add(*output);
                effects.push(BehaviorEffect::InsertInventory {
                    role: BehaviorInventoryRole::Output,
                    stack: *output,
                });
                metrics.inventory_transfers += 1;
            }
        }
        machine.progress_ticks = 0;
        machine.status = MachineStatus::Idle;
        output_changed = true;
    }

    machine_tick_output(machine, metrics, output_changed, effects)
}

fn machine_tick_output(
    machine: MachineRuntime,
    metrics: BehaviorTickMetrics,
    output_changed: bool,
    mut effects: Vec<BehaviorEffect>,
) -> BehaviorTickOutput {
    effects.push(BehaviorEffect::SetState(test_machine_behavior_state(
        machine,
    )));
    BehaviorTickOutput {
        effects,
        metrics,
        output_changed,
    }
}

pub fn apply_machine_command(input: BehaviorCommandInput<'_>) -> BehaviorCommandOutput {
    let BehaviorCommandInput {
        catalog,
        building,
        config,
        state,
        command,
        inventories,
    } = input;
    let Some(mut machine) = test_machine_runtime(&state) else {
        return BehaviorCommandOutput {
            effects: vec![BehaviorEffect::SetState(state)],
        };
    };
    if command.name != SET_RECIPE_COMMAND {
        return BehaviorCommandOutput {
            effects: vec![BehaviorEffect::SetState(test_machine_behavior_state(
                machine,
            ))],
        };
    }

    let recipe = command
        .data
        .get("recipe")
        .and_then(string_value)
        .map(str::to_string);
    let recipes = config_recipes(config);
    if let Some(recipe_id) = &recipe {
        let valid = recipes.contains(recipe_id)
            && catalog
                .recipe(recipe_id)
                .is_some_and(|recipe| recipe.machines.contains(&building.def_id));
        if !valid {
            return BehaviorCommandOutput {
                effects: vec![BehaviorEffect::SetState(test_machine_behavior_state(
                    machine,
                ))],
            };
        }
    }

    let drop_stacks = removed_machine_drops(catalog, config, &state);
    let mut effects = Vec::new();
    if machine.active_recipe != recipe {
        effects.extend(
            [
                BehaviorInventoryRole::Input,
                BehaviorInventoryRole::Output,
                BehaviorInventoryRole::Fuel,
            ]
            .into_iter()
            .filter(|role| inventories.iter().any(|inventory| inventory.role == *role))
            .map(|role| BehaviorEffect::DrainInventory { role }),
        );
    }
    if !drop_stacks.is_empty() {
        effects.push(BehaviorEffect::DropItems {
            stacks: drop_stacks,
        });
    }
    machine.status = if recipe.is_some() {
        MachineStatus::Idle
    } else {
        MachineStatus::NoRecipeSelected
    };
    machine.active_recipe = recipe;
    machine.progress_ticks = 0;
    machine.fuel_remaining_ticks = 0;
    machine.fuel_total_ticks = 0;
    machine.fuel_temperature = 0;
    machine.pending_outputs.clear();
    effects.push(BehaviorEffect::SetState(test_machine_behavior_state(
        machine,
    )));

    BehaviorCommandOutput { effects }
}

pub fn removed_machine_drops(
    catalog: &BehaviorCatalog,
    _config: &BTreeMap<String, BehaviorConfigValue>,
    state: &BehaviorInstanceState,
) -> Vec<BehaviorItemStack> {
    let Some(machine) = test_machine_runtime(state) else {
        return Vec::new();
    };
    if !machine.pending_outputs.is_empty() {
        return machine.pending_outputs;
    }
    if machine.progress_ticks == 0 {
        return Vec::new();
    }
    let Some(recipe_id) = &machine.active_recipe else {
        return Vec::new();
    };
    let Some(recipe) = catalog.recipe(recipe_id) else {
        return Vec::new();
    };
    if machine.progress_ticks < recipe.duration_ticks {
        match recipe.kind {
            BehaviorRecipeKind::Extraction { resource } => {
                let amount = recipe
                    .outputs
                    .iter()
                    .filter(|stack| stack.kind == resource)
                    .map(|stack| stack.amount)
                    .sum::<u32>()
                    .max(1);
                vec![BehaviorItemStack {
                    kind: resource,
                    amount,
                }]
            }
            BehaviorRecipeKind::Processing => recipe.inputs.clone(),
        }
    } else {
        recipe.outputs.clone()
    }
}

pub fn machine_accepts_input(
    catalog: &BehaviorCatalog,
    _config: &BTreeMap<String, BehaviorConfigValue>,
    state: &BehaviorInstanceState,
    kind: u16,
) -> bool {
    let Some(machine) = test_machine_runtime(state) else {
        return true;
    };
    let Some(recipe_id) = &machine.active_recipe else {
        return false;
    };
    catalog
        .recipe(recipe_id)
        .is_some_and(|recipe| recipe.inputs.iter().any(|input| input.kind == kind))
}

pub fn recipe_outputs_blocked(
    outputs: &[BehaviorItemStack],
    output_inventory: Option<&BehaviorInventory>,
) -> bool {
    if outputs.is_empty() {
        return false;
    }
    let Some(output_inventory) = output_inventory else {
        return true;
    };
    !can_add_all_outputs(outputs, output_inventory)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TestBehaviorHost;

impl BehaviorHost for TestBehaviorHost {
    fn initial_behavior_state(
        &self,
        input: BehaviorInitInput<'_>,
    ) -> BehaviorHostResult<BehaviorInstanceState> {
        if input.behavior_id == TEST_MACHINE_BEHAVIOR_ID {
            return Ok(initial_machine_behavior_state(input.config));
        }
        Ok(BehaviorInstanceState::new(
            BehaviorId::new(input.behavior_id),
            BehaviorStatus::new("idle"),
        ))
    }

    fn apply_behavior_command(
        &self,
        input: BehaviorCommandInput<'_>,
    ) -> BehaviorHostResult<BehaviorCommandOutput> {
        if input.state.behavior_id.as_str() == TEST_MACHINE_BEHAVIOR_ID {
            return Ok(apply_machine_command(input));
        }
        Ok(BehaviorCommandOutput {
            effects: vec![BehaviorEffect::SetState(input.state)],
        })
    }

    fn tick_behavior(
        &self,
        input: BehaviorTickInput<'_>,
    ) -> BehaviorHostResult<BehaviorTickOutput> {
        if input.state.behavior_id.as_str() == TEST_MACHINE_BEHAVIOR_ID {
            return Ok(tick_machine_behavior(input));
        }
        Ok(BehaviorTickOutput {
            effects: vec![BehaviorEffect::SetState(input.state)],
            metrics: BehaviorTickMetrics::default(),
            output_changed: false,
        })
    }

    fn removed_behavior_effects(
        &self,
        catalog: &BehaviorCatalog,
        config: &BTreeMap<String, BehaviorConfigValue>,
        state: &BehaviorInstanceState,
    ) -> BehaviorHostResult<Vec<BehaviorEffect>> {
        if state.behavior_id.as_str() == TEST_MACHINE_BEHAVIOR_ID {
            let stacks = removed_machine_drops(catalog, config, state);
            if stacks.is_empty() {
                return Ok(Vec::new());
            }
            return Ok(vec![BehaviorEffect::DropItems { stacks }]);
        }
        Ok(Vec::new())
    }

    fn behavior_accepts_input(
        &self,
        catalog: &BehaviorCatalog,
        config: &BTreeMap<String, BehaviorConfigValue>,
        state: &BehaviorInstanceState,
        kind: u16,
    ) -> BehaviorHostResult<bool> {
        if state.behavior_id.as_str() == TEST_MACHINE_BEHAVIOR_ID {
            return Ok(machine_accepts_input(catalog, config, state, kind));
        }
        Ok(true)
    }
}

fn selected_recipe(
    catalog: &BehaviorCatalog,
    building: &BehaviorBuildingContext,
    config: &BTreeMap<String, BehaviorConfigValue>,
    resources: &BTreeMap<BehaviorTilePos, BehaviorResource>,
    tick: u64,
    machine: &MachineRuntime,
    output_inventory: Option<&BehaviorInventory>,
) -> Option<(BehaviorRecipeDef, Option<BehaviorTilePos>)> {
    if machine.active_recipe.is_none() && config_role(config) == Some("extractor") {
        return extraction_target(
            catalog,
            building,
            config,
            resources,
            tick,
            output_inventory,
            machine,
        )
        .map(|(recipe, tile)| (recipe, Some(tile)));
    }
    let recipe_id = machine.active_recipe.as_ref()?;
    let mut recipe = catalog.recipe(recipe_id)?.clone();
    if machine.progress_ticks == 0
        && matches!(recipe.kind, BehaviorRecipeKind::Extraction { .. })
        && let Some((selected_recipe, tile)) = extraction_target(
            catalog,
            building,
            config,
            resources,
            tick,
            output_inventory,
            machine,
        )
    {
        recipe = selected_recipe;
        return Some((recipe, Some(tile)));
    }
    Some((recipe, None))
}

fn extraction_target(
    catalog: &BehaviorCatalog,
    building: &BehaviorBuildingContext,
    config: &BTreeMap<String, BehaviorConfigValue>,
    resources: &BTreeMap<BehaviorTilePos, BehaviorResource>,
    tick: u64,
    output_inventory: Option<&BehaviorInventory>,
    machine: &MachineRuntime,
) -> Option<(BehaviorRecipeDef, BehaviorTilePos)> {
    let recipes = config_recipes(config);
    let candidate_tiles = config_work_area(config)
        .filter(|work_area| !work_area.is_empty())
        .map(|work_area| footprint_tiles(building.origin, &work_area))
        .unwrap_or_else(|| building.footprint.clone());

    if machine.status == MachineStatus::MissingFuel
        && let Some(recipe) = machine
            .active_recipe
            .as_ref()
            .and_then(|recipe_id| catalog.recipe(recipe_id))
            .cloned()
        && let BehaviorRecipeKind::Extraction { resource } = recipe.kind
        && !recipe_outputs_blocked_for_selection(&recipe.outputs, output_inventory)
        && let Some(pos) = candidate_tiles
            .iter()
            .copied()
            .filter(|pos| {
                resources
                    .get(pos)
                    .is_some_and(|entry| entry.kind == resource && entry.amount > 0)
            })
            .min()
    {
        return Some((recipe, pos));
    }

    recipes
        .iter()
        .filter_map(|recipe_id| catalog.recipe(recipe_id))
        .filter_map(|recipe| match recipe.kind {
            BehaviorRecipeKind::Extraction { resource } => Some((recipe, resource)),
            BehaviorRecipeKind::Processing => None,
        })
        .filter(|(recipe, _)| {
            !recipe_outputs_blocked_for_selection(&recipe.outputs, output_inventory)
        })
        .flat_map(|(recipe, resource)| {
            candidate_tiles.iter().copied().filter_map(move |pos| {
                resources
                    .get(&pos)
                    .is_some_and(|entry| entry.kind == resource && entry.amount > 0)
                    .then_some((recipe.clone(), pos))
            })
        })
        .max_by_key(|(_, pos)| resource_tile_selection_key(building.id as u64, tick, *pos))
}

fn recipe_outputs_blocked_for_selection(
    outputs: &[BehaviorItemStack],
    output_inventory: Option<&BehaviorInventory>,
) -> bool {
    output_inventory.is_some_and(|inventory| recipe_outputs_blocked(outputs, Some(inventory)))
}

fn has_matching_resource(
    catalog: &BehaviorCatalog,
    building: &BehaviorBuildingContext,
    config: &BTreeMap<String, BehaviorConfigValue>,
    resources: &BTreeMap<BehaviorTilePos, BehaviorResource>,
) -> bool {
    let candidate_tiles = config_work_area(config)
        .filter(|work_area| !work_area.is_empty())
        .map(|work_area| footprint_tiles(building.origin, &work_area))
        .unwrap_or_else(|| building.footprint.clone());
    config_recipes(config)
        .iter()
        .filter_map(|recipe_id| catalog.recipe(recipe_id))
        .filter_map(|recipe| match recipe.kind {
            BehaviorRecipeKind::Extraction { resource } => Some(resource),
            BehaviorRecipeKind::Processing => None,
        })
        .any(|resource| {
            candidate_tiles.iter().any(|pos| {
                resources
                    .get(pos)
                    .is_some_and(|entry| entry.kind == resource && entry.amount > 0)
            })
        })
}

fn resource_tile_selection_key(building: u64, tick: u64, pos: BehaviorTilePos) -> u64 {
    let mut value = building
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(tick.rotate_left(17))
        .wrapping_add((pos.x as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add((pos.y as i64 as u64).wrapping_mul(0x94D0_49BB_1331_11EB));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn footprint_tiles(origin: BehaviorTilePos, offsets: &[(i32, i32)]) -> Vec<BehaviorTilePos> {
    let mut tiles = offsets
        .iter()
        .map(|(x, y)| BehaviorTilePos {
            x: origin.x + *x,
            y: origin.y + *y,
        })
        .collect::<Vec<_>>();
    tiles.sort();
    tiles
}

fn initial_active_recipe(role: Option<&str>, recipes: &[String]) -> Option<String> {
    match (role, recipes) {
        (_, []) => None,
        (Some("processor"), [_first, _second, ..]) => None,
        (_, [recipe]) => Some(recipe.clone()),
        (Some("extractor"), recipes) => recipes.first().cloned(),
        _ => recipes.first().cloned(),
    }
}

fn config_role(config: &BTreeMap<String, BehaviorConfigValue>) -> Option<&str> {
    match config.get("role") {
        Some(BehaviorConfigValue::String(value)) => Some(value.as_str()),
        _ => None,
    }
}

fn config_recipes(config: &BTreeMap<String, BehaviorConfigValue>) -> Vec<String> {
    match config.get("recipes") {
        Some(BehaviorConfigValue::StringList(values)) => values.clone(),
        _ => Vec::new(),
    }
}

fn config_work_area(config: &BTreeMap<String, BehaviorConfigValue>) -> Option<Vec<(i32, i32)>> {
    match config.get("work_area") {
        Some(BehaviorConfigValue::TileOffsets(values)) => {
            Some(values.iter().map(|offset| (offset.x, offset.y)).collect())
        }
        _ => None,
    }
}

fn machine_status_name(status: MachineStatus) -> &'static str {
    match status {
        MachineStatus::Idle => "idle",
        MachineStatus::Working => "working",
        MachineStatus::NoRecipeSelected => "no_recipe_selected",
        MachineStatus::NoMatchingResource => "no_matching_resource",
        MachineStatus::MissingInput => "missing_input",
        MachineStatus::MissingFuel => "missing_fuel",
        MachineStatus::OutputBlocked => "output_blocked",
    }
}

fn machine_status_from_name(value: &str) -> Option<MachineStatus> {
    match value {
        "idle" => Some(MachineStatus::Idle),
        "working" => Some(MachineStatus::Working),
        "no_recipe_selected" => Some(MachineStatus::NoRecipeSelected),
        "no_matching_resource" => Some(MachineStatus::NoMatchingResource),
        "missing_input" => Some(MachineStatus::MissingInput),
        "missing_fuel" => Some(MachineStatus::MissingFuel),
        "output_blocked" => Some(MachineStatus::OutputBlocked),
        _ => None,
    }
}

fn string_value(value: &BehaviorStateValue) -> Option<&str> {
    match value {
        BehaviorStateValue::String(value) => Some(value.as_str()),
        _ => None,
    }
}

fn u32_value(value: &BehaviorStateValue) -> Option<u32> {
    match value {
        BehaviorStateValue::U32(value) => Some(*value),
        _ => None,
    }
}

fn item_stacks_value(value: &BehaviorStateValue) -> Option<Vec<BehaviorItemStack>> {
    match value {
        BehaviorStateValue::ItemStacks(value) => Some(value.clone()),
        _ => None,
    }
}
