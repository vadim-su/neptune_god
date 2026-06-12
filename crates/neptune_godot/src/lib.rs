use godot::prelude::*;
use sim_core::catalog::CoreCatalog;
use sim_core::command::SimCommand;
use sim_core::ids::TilePos;
use sim_core::topology::graph::Direction;
use sim_core::world::SimWorld;
use sim_core::worldgen::{DEFAULT_WORLD_SEED, WorldGenerator};

struct NeptuneGodotExtension;

#[gdextension]
unsafe impl ExtensionLibrary for NeptuneGodotExtension {}

#[derive(GodotClass)]
#[class(base=RefCounted)]
pub struct NeptuneSim {
    world: SimWorld,
    map_min: TilePos,
    map_max: TilePos,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for NeptuneSim {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            world: SimWorld::with_catalog(CoreCatalog::for_tests()),
            map_min: TilePos::new(0, 0),
            map_max: TilePos::new(0, 0),
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
            building.set(
                "quarter_turns",
                quarter_turns_from_direction(snapshot.direction),
            );
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
    pub fn reset(&mut self) {
        self.world = SimWorld::with_catalog(CoreCatalog::for_tests());
        self.map_min = TilePos::new(0, 0);
        self.map_max = TilePos::new(0, 0);
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
}
