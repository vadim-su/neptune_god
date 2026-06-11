use godot::prelude::*;
use sim_core::catalog::CoreCatalog;
use sim_core::ids::TilePos;
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
    pub fn reset(&mut self) {
        self.world = SimWorld::with_catalog(CoreCatalog::for_tests());
        self.map_min = TilePos::new(0, 0);
        self.map_max = TilePos::new(0, 0);
    }
}
