use godot::prelude::*;
use sim_core::catalog::CoreCatalog;
use sim_core::world::SimWorld;

struct NeptuneGodotExtension;

#[gdextension]
unsafe impl ExtensionLibrary for NeptuneGodotExtension {}

#[derive(GodotClass)]
#[class(base=RefCounted)]
pub struct NeptuneSim {
    world: SimWorld,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for NeptuneSim {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            world: SimWorld::with_catalog(CoreCatalog::for_tests()),
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
    pub fn reset(&mut self) {
        self.world = SimWorld::with_catalog(CoreCatalog::for_tests());
    }
}
