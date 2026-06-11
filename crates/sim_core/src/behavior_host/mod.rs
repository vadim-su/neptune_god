//! Behavior pack host: tick bridge between [`SimWorld`] and [`behavior_api`].

use std::collections::{BTreeMap, BTreeSet};

use behavior_api::{
    BehaviorBuildingContext, BehaviorCatalog, BehaviorCommandOutput, BehaviorConfigValue,
    BehaviorEffect, BehaviorHostResult, BehaviorId, BehaviorInstanceState, BehaviorInventory,
    BehaviorInventoryRole, BehaviorItemStack, BehaviorPack, BehaviorResource, BehaviorStatus,
    BehaviorTickMetrics, BehaviorTickOutput, BehaviorTilePos,
};

use crate::building::SimBuilding;
use crate::catalog::{CoreCatalog, CoreItemStack};
use crate::ids::{ItemKindId, TilePos};
use crate::inventory::SimInventory;
use crate::metrics::SimMetricsSnapshot;

pub use behavior_api::{BehaviorCommandInput, BehaviorHost, BehaviorInitInput, BehaviorTickInput};

#[derive(Clone, Copy)]
pub struct BehaviorRuntime<'a> {
    host: &'a dyn BehaviorHost,
    catalog: &'a BehaviorCatalog,
    policy: BehaviorRuntimePolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorRuntimePolicy {
    pub effect_rejection: BehaviorEffectRejectionPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorEffectRejectionPolicy {
    Panic,
    ReportOnly,
    QuarantineInstance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BehaviorPackValidationReport {
    pub required_behaviors: Vec<BehaviorId>,
    pub provided_behaviors: Vec<BehaviorId>,
    pub missing_behaviors: Vec<BehaviorId>,
    pub unused_behaviors: Vec<BehaviorId>,
}

impl BehaviorPackValidationReport {
    pub fn is_valid(&self) -> bool {
        self.missing_behaviors.is_empty()
    }
}

impl Default for BehaviorRuntimePolicy {
    fn default() -> Self {
        Self {
            effect_rejection: if cfg!(debug_assertions) {
                BehaviorEffectRejectionPolicy::Panic
            } else {
                BehaviorEffectRejectionPolicy::ReportOnly
            },
        }
    }
}

impl<'a> BehaviorRuntime<'a> {
    pub fn new<H: BehaviorHost + 'a>(host: &'a H, catalog: &'a BehaviorCatalog) -> Self {
        Self::new_with_policy(host, catalog, BehaviorRuntimePolicy::default())
    }

    pub fn new_with_policy<H: BehaviorHost + 'a>(
        host: &'a H,
        catalog: &'a BehaviorCatalog,
        policy: BehaviorRuntimePolicy,
    ) -> Self {
        Self {
            host,
            catalog,
            policy,
        }
    }

    pub fn from_pack(pack: &'a dyn BehaviorPack) -> Self {
        Self {
            host: pack.host(),
            catalog: pack.catalog(),
            policy: BehaviorRuntimePolicy::default(),
        }
    }

    pub fn host(&self) -> &'a dyn BehaviorHost {
        self.host
    }

    pub fn catalog(&self) -> &'a BehaviorCatalog {
        self.catalog
    }

    pub fn policy(&self) -> BehaviorRuntimePolicy {
        self.policy
    }
}

pub fn validate_behavior_pack_bindings(
    catalog: &CoreCatalog,
    pack: &dyn BehaviorPack,
) -> BehaviorPackValidationReport {
    let required = catalog
        .buildings
        .iter()
        .filter(|building| building.behavior.requires_behavior_host())
        .map(|building| BehaviorId::new(&building.behavior.behavior_id))
        .collect::<BTreeSet<_>>();
    let provided = pack
        .manifest()
        .behaviors
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    let missing_behaviors = required.difference(&provided).cloned().collect::<Vec<_>>();
    let unused_behaviors = provided.difference(&required).cloned().collect::<Vec<_>>();

    BehaviorPackValidationReport {
        required_behaviors: required.into_iter().collect(),
        provided_behaviors: provided.into_iter().collect(),
        missing_behaviors,
        unused_behaviors,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopBehaviorHost;

pub const NOOP_BEHAVIOR_HOST: NoopBehaviorHost = NoopBehaviorHost;

impl BehaviorHost for NoopBehaviorHost {
    fn initial_behavior_state(
        &self,
        input: BehaviorInitInput<'_>,
    ) -> BehaviorHostResult<BehaviorInstanceState> {
        Ok(BehaviorInstanceState::new(
            behavior_api::BehaviorId::new(input.behavior_id),
            BehaviorStatus::new("idle"),
        ))
    }

    fn apply_behavior_command(
        &self,
        input: BehaviorCommandInput<'_>,
    ) -> BehaviorHostResult<BehaviorCommandOutput> {
        Ok(BehaviorCommandOutput {
            effects: vec![BehaviorEffect::SetState(input.state)],
        })
    }

    fn tick_behavior(
        &self,
        input: BehaviorTickInput<'_>,
    ) -> BehaviorHostResult<BehaviorTickOutput> {
        Ok(BehaviorTickOutput {
            effects: vec![BehaviorEffect::SetState(input.state)],
            metrics: BehaviorTickMetrics::default(),
            output_changed: false,
        })
    }

    fn removed_behavior_effects(
        &self,
        _catalog: &BehaviorCatalog,
        _config: &BTreeMap<String, BehaviorConfigValue>,
        _state: &BehaviorInstanceState,
    ) -> BehaviorHostResult<Vec<BehaviorEffect>> {
        Ok(Vec::new())
    }

    fn behavior_accepts_input(
        &self,
        _catalog: &BehaviorCatalog,
        _config: &BTreeMap<String, BehaviorConfigValue>,
        _state: &BehaviorInstanceState,
        _kind: u16,
    ) -> BehaviorHostResult<bool> {
        Ok(true)
    }
}

pub fn behavior_building(building: &SimBuilding) -> BehaviorBuildingContext {
    BehaviorBuildingContext {
        id: building.id.0,
        def_id: building.def_id.clone(),
        origin: behavior_tile_pos(building.origin),
        footprint: building
            .footprint
            .iter()
            .copied()
            .map(behavior_tile_pos)
            .collect(),
    }
}

pub fn behavior_resources(
    resources: &BTreeMap<TilePos, (ItemKindId, u32)>,
) -> BTreeMap<BehaviorTilePos, BehaviorResource> {
    resources
        .iter()
        .map(|(pos, (kind, amount))| {
            (
                behavior_tile_pos(*pos),
                BehaviorResource {
                    kind: kind.0,
                    amount: *amount,
                },
            )
        })
        .collect()
}

pub fn behavior_inventories(inventories: Vec<SimInventory>) -> Vec<BehaviorInventory> {
    inventories
        .into_iter()
        .map(SimInventory::into_behavior_inventory)
        .collect()
}

pub fn core_inventories(inventories: Vec<BehaviorInventory>) -> Vec<SimInventory> {
    inventories
        .into_iter()
        .map(SimInventory::from_behavior_inventory)
        .collect()
}

pub fn core_stacks(stacks: Vec<BehaviorItemStack>) -> Vec<CoreItemStack> {
    stacks
        .into_iter()
        .map(|stack| CoreItemStack {
            kind: ItemKindId(stack.kind),
            amount: stack.amount,
        })
        .collect()
}

pub fn core_stack(stack: BehaviorItemStack) -> CoreItemStack {
    CoreItemStack {
        kind: ItemKindId(stack.kind),
        amount: stack.amount,
    }
}

pub fn core_tile_pos(pos: BehaviorTilePos) -> TilePos {
    TilePos::new(pos.x, pos.y)
}

pub fn behavior_kind(kind: ItemKindId) -> u16 {
    kind.0
}

pub fn core_inventory_role(role: BehaviorInventoryRole) -> crate::catalog::CoreInventoryRole {
    match role {
        BehaviorInventoryRole::Input => crate::catalog::CoreInventoryRole::Input,
        BehaviorInventoryRole::Output => crate::catalog::CoreInventoryRole::Output,
        BehaviorInventoryRole::Fuel => crate::catalog::CoreInventoryRole::Fuel,
        BehaviorInventoryRole::Storage => crate::catalog::CoreInventoryRole::Storage,
        BehaviorInventoryRole::InserterHand => crate::catalog::CoreInventoryRole::InserterHand,
    }
}

pub fn apply_behavior_metrics(
    behavior_metrics: BehaviorTickMetrics,
    metrics: &mut SimMetricsSnapshot,
) {
    metrics.active_behaviors += behavior_metrics.active_behaviors;
    metrics.fuel_starved_behaviors += behavior_metrics.fuel_starved_behaviors;
    metrics.blocked_outputs += behavior_metrics.blocked_outputs;
    metrics.inventory_transfers += behavior_metrics.inventory_transfers;
}

fn behavior_tile_pos(pos: TilePos) -> BehaviorTilePos {
    BehaviorTilePos { x: pos.x, y: pos.y }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::CoreCatalog;
    use behavior_api::{
        BehaviorApiVersion, BehaviorId, BehaviorPackContentHash, BehaviorPackId,
        BehaviorPackManifest, BehaviorPackName, BehaviorPackVersion,
    };

    #[derive(Clone, Debug)]
    struct TestBehaviorPack {
        manifest: BehaviorPackManifest,
        catalog: BehaviorCatalog,
    }

    impl TestBehaviorPack {
        fn with_behaviors(behaviors: impl IntoIterator<Item = &'static str>) -> Self {
            Self {
                manifest: BehaviorPackManifest {
                    pack_id: BehaviorPackId::new("test-pack"),
                    name: BehaviorPackName::new("Test Pack"),
                    version: BehaviorPackVersion::new("0.0.0"),
                    api_version: BehaviorApiVersion::new(1),
                    content_hash: BehaviorPackContentHash::new("test"),
                    behaviors: behaviors.into_iter().map(BehaviorId::new).collect(),
                },
                catalog: BehaviorCatalog::default(),
            }
        }
    }

    impl BehaviorPack for TestBehaviorPack {
        fn manifest(&self) -> &BehaviorPackManifest {
            &self.manifest
        }

        fn catalog(&self) -> &BehaviorCatalog {
            &self.catalog
        }

        fn host(&self) -> &dyn BehaviorHost {
            &NOOP_BEHAVIOR_HOST
        }
    }

    fn catalog_with_behavior_id(behavior: &str) -> CoreCatalog {
        let mut catalog = CoreCatalog::for_tests();
        for building in &mut catalog.buildings {
            if building.behavior.requires_behavior_host() {
                building.behavior.behavior_id = behavior.to_string();
            }
        }
        catalog
    }

    #[test]
    fn behavior_runtime_pairs_host_and_catalog() {
        let catalog = BehaviorCatalog::default();
        let runtime = BehaviorRuntime::new(&NOOP_BEHAVIOR_HOST, &catalog);

        assert!(std::ptr::eq(runtime.catalog(), &catalog));
        let state = runtime
            .host()
            .initial_behavior_state(BehaviorInitInput {
                behavior_id: "test:behavior",
                config: &BTreeMap::new(),
            })
            .unwrap();
        assert_eq!(state.behavior_id.as_str(), "test:behavior");
    }

    #[test]
    fn behavior_pack_validation_accepts_matching_catalog_binding() {
        let catalog = catalog_with_behavior_id("test:machine");
        let pack = TestBehaviorPack::with_behaviors(["test:machine"]);

        let report = validate_behavior_pack_bindings(&catalog, &pack);

        assert!(report.is_valid());
        assert_eq!(
            report.required_behaviors,
            vec![BehaviorId::new("test:machine")]
        );
        assert_eq!(
            report.provided_behaviors,
            vec![BehaviorId::new("test:machine")]
        );
        assert!(report.missing_behaviors.is_empty());
        assert!(report.unused_behaviors.is_empty());
    }

    #[test]
    fn behavior_pack_validation_reports_missing_and_unused_behaviors() {
        let catalog = catalog_with_behavior_id("missing:machine");
        let pack = TestBehaviorPack::with_behaviors(["extra:machine"]);

        let report = validate_behavior_pack_bindings(&catalog, &pack);

        assert!(!report.is_valid());
        assert_eq!(
            report.missing_behaviors,
            vec![BehaviorId::new("missing:machine")]
        );
        assert_eq!(
            report.unused_behaviors,
            vec![BehaviorId::new("extra:machine")]
        );
    }
}
