//! Stable API between `sim_core` and behavior packs (Rust or WASM).
//!
//! Defines [`BehaviorHost`], catalog DTOs, tick/command IO, and pack manifests.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod composite;

pub use composite::CompositeBehaviorPack;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct BehaviorId(pub String);

impl BehaviorId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct BehaviorPackId(pub String);

impl BehaviorPackId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct BehaviorPackVersion(pub String);

impl BehaviorPackVersion {
    pub fn new(version: impl Into<String>) -> Self {
        Self(version.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct BehaviorPackName(pub String);

impl BehaviorPackName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct BehaviorApiVersion(pub u16);

impl BehaviorApiVersion {
    pub fn new(version: u16) -> Self {
        Self(version)
    }

    pub fn value(&self) -> u16 {
        self.0
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct BehaviorPackContentHash(pub String);

impl BehaviorPackContentHash {
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BehaviorPackManifest {
    pub pack_id: BehaviorPackId,
    pub name: BehaviorPackName,
    pub version: BehaviorPackVersion,
    pub api_version: BehaviorApiVersion,
    pub content_hash: BehaviorPackContentHash,
    pub behaviors: Vec<BehaviorId>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BehaviorInstanceState {
    pub behavior_id: BehaviorId,
    pub status: BehaviorStatus,
    pub data: BTreeMap<String, BehaviorStateValue>,
}

impl BehaviorInstanceState {
    pub fn new(behavior_id: BehaviorId, status: BehaviorStatus) -> Self {
        Self {
            behavior_id,
            status,
            data: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BehaviorStatus(pub String);

impl BehaviorStatus {
    pub fn new(status: impl Into<String>) -> Self {
        Self(status.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BehaviorItemStack {
    pub kind: u16,
    pub amount: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct BehaviorTilePos {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum BehaviorInventoryRole {
    Input,
    Output,
    Fuel,
    Storage,
    InserterHand,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BehaviorInventory {
    pub role: BehaviorInventoryRole,
    pub slots: Vec<Option<BehaviorItemStack>>,
    pub max_stack: u32,
    pub accepts: Vec<u16>,
}

impl BehaviorInventory {
    pub fn accepts(&self, kind: u16) -> bool {
        self.accepts.is_empty() || self.accepts.contains(&kind)
    }

    pub fn add(&mut self, stack: BehaviorItemStack) -> Option<BehaviorItemStack> {
        if stack.amount == 0 || !self.accepts(stack.kind) {
            return Some(stack);
        }
        let mut remaining = stack.amount;
        for slot in self.slots.iter_mut().flatten() {
            if slot.kind != stack.kind || slot.amount >= self.max_stack {
                continue;
            }
            let accepted = (self.max_stack - slot.amount).min(remaining);
            slot.amount += accepted;
            remaining -= accepted;
            if remaining == 0 {
                return None;
            }
        }
        for slot in &mut self.slots {
            if slot.is_some() {
                continue;
            }
            let accepted = self.max_stack.min(remaining);
            *slot = Some(BehaviorItemStack {
                kind: stack.kind,
                amount: accepted,
            });
            remaining -= accepted;
            if remaining == 0 {
                return None;
            }
        }
        Some(BehaviorItemStack {
            kind: stack.kind,
            amount: remaining,
        })
    }

    pub fn remove(&mut self, stack: BehaviorItemStack) -> bool {
        if self.count(stack.kind) < stack.amount {
            return false;
        }
        let mut remaining = stack.amount;
        for slot in &mut self.slots {
            let Some(existing) = slot else {
                continue;
            };
            if existing.kind != stack.kind {
                continue;
            }
            let removed = existing.amount.min(remaining);
            existing.amount -= removed;
            remaining -= removed;
            if existing.amount == 0 {
                *slot = None;
            }
            if remaining == 0 {
                break;
            }
        }
        true
    }

    pub fn take_first_matching(
        &mut self,
        accepts: impl Fn(u16) -> bool,
    ) -> Option<BehaviorItemStack> {
        for slot in &mut self.slots {
            let Some(existing) = slot else {
                continue;
            };
            if !accepts(existing.kind) {
                continue;
            }
            let taken = BehaviorItemStack {
                kind: existing.kind,
                amount: 1,
            };
            existing.amount -= 1;
            if existing.amount == 0 {
                *slot = None;
            }
            return Some(taken);
        }
        None
    }

    pub fn count(&self, kind: u16) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|stack| stack.kind == kind)
            .map(|stack| stack.amount)
            .sum()
    }

    pub fn drain(&mut self) -> Vec<BehaviorItemStack> {
        self.slots.iter_mut().filter_map(Option::take).collect()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BehaviorBuildingContext {
    pub id: u32,
    pub def_id: String,
    pub origin: BehaviorTilePos,
    pub footprint: Vec<BehaviorTilePos>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BehaviorResource {
    pub kind: u16,
    pub amount: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BehaviorFuelDef {
    pub energy: f32,
    pub burn_temperature: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BehaviorItemDef {
    pub kind: u16,
    pub def_id: String,
    pub max_stack: u32,
    pub fuel: Option<BehaviorFuelDef>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum BehaviorRecipeKind {
    Extraction { resource: u16 },
    Processing,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BehaviorRecipeEnergyDef {
    pub required_per_second: f32,
    pub min_temperature: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BehaviorRecipeDef {
    pub id: String,
    pub machines: Vec<String>,
    pub kind: BehaviorRecipeKind,
    pub duration_ticks: u32,
    pub inputs: Vec<BehaviorItemStack>,
    pub outputs: Vec<BehaviorItemStack>,
    pub energy: Option<BehaviorRecipeEnergyDef>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BehaviorCatalog {
    pub items: Vec<BehaviorItemDef>,
    pub recipes: Vec<BehaviorRecipeDef>,
}

impl BehaviorCatalog {
    pub fn item(&self, kind: u16) -> Option<&BehaviorItemDef> {
        self.items.iter().find(|item| item.kind == kind)
    }

    pub fn recipe(&self, id: &str) -> Option<&BehaviorRecipeDef> {
        self.recipes.iter().find(|recipe| recipe.id == id)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum BehaviorStateValue {
    ItemStacks(Vec<BehaviorItemStack>),
    String(String),
    U32(u32),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BehaviorTileOffset {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum BehaviorConfigValue {
    String(String),
    U32(u32),
    StringList(Vec<String>),
    TileOffsets(Vec<BehaviorTileOffset>),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BehaviorCommand {
    pub name: String,
    pub data: BTreeMap<String, BehaviorStateValue>,
}

impl BehaviorCommand {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            data: BTreeMap::new(),
        }
    }
}

pub struct BehaviorInitInput<'a> {
    pub behavior_id: &'a str,
    pub config: &'a BTreeMap<String, BehaviorConfigValue>,
}

pub struct BehaviorCommandInput<'a> {
    pub catalog: &'a BehaviorCatalog,
    pub building: &'a BehaviorBuildingContext,
    pub config: &'a BTreeMap<String, BehaviorConfigValue>,
    pub state: BehaviorInstanceState,
    pub command: BehaviorCommand,
    pub inventories: Vec<BehaviorInventory>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum BehaviorEffect {
    SetState(BehaviorInstanceState),
    SetPowerOutput {
        max_output: u32,
    },
    TakeInventory {
        role: BehaviorInventoryRole,
        stack: BehaviorItemStack,
    },
    InsertInventory {
        role: BehaviorInventoryRole,
        stack: BehaviorItemStack,
    },
    DrainInventory {
        role: BehaviorInventoryRole,
    },
    DepleteResource {
        pos: BehaviorTilePos,
    },
    DropItems {
        stacks: Vec<BehaviorItemStack>,
    },
}

pub struct BehaviorCommandOutput {
    pub effects: Vec<BehaviorEffect>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BehaviorPowerInput {
    pub required: u32,
    pub supplied: u32,
    pub supplied_ratio_ppm: u32,
    pub offline: bool,
}

impl Default for BehaviorPowerInput {
    fn default() -> Self {
        Self {
            required: 0,
            supplied: 0,
            supplied_ratio_ppm: 1_000_000,
            offline: false,
        }
    }
}

pub struct BehaviorTickInput<'a> {
    pub catalog: &'a BehaviorCatalog,
    pub building: &'a BehaviorBuildingContext,
    pub config: &'a BTreeMap<String, BehaviorConfigValue>,
    pub state: BehaviorInstanceState,
    pub inventories: Vec<BehaviorInventory>,
    pub resources: &'a BTreeMap<BehaviorTilePos, BehaviorResource>,
    pub power: BehaviorPowerInput,
    pub tick: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BehaviorTickMetrics {
    pub active_behaviors: usize,
    pub fuel_starved_behaviors: usize,
    pub blocked_outputs: usize,
    pub inventory_transfers: usize,
}

pub struct BehaviorTickOutput {
    pub effects: Vec<BehaviorEffect>,
    pub metrics: BehaviorTickMetrics,
    pub output_changed: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum BehaviorHostErrorKind {
    MissingBehavior,
    InvalidState,
    InvalidCommand,
    InvalidManifest,
    RuntimeFailure,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BehaviorHostError {
    pub kind: BehaviorHostErrorKind,
    pub message: String,
}

impl BehaviorHostError {
    pub fn new(kind: BehaviorHostErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn missing_behavior(behavior_id: &str) -> Self {
        Self::new(
            BehaviorHostErrorKind::MissingBehavior,
            format!("missing behavior '{behavior_id}'"),
        )
    }
}

pub type BehaviorHostResult<T> = Result<T, BehaviorHostError>;

/// Machine logic invoked by `sim_core` each tick (vanilla, WASM, or test hosts).
pub trait BehaviorHost: Send + Sync {
    fn initial_behavior_state(
        &self,
        input: BehaviorInitInput<'_>,
    ) -> BehaviorHostResult<BehaviorInstanceState>;

    fn apply_behavior_command(
        &self,
        input: BehaviorCommandInput<'_>,
    ) -> BehaviorHostResult<BehaviorCommandOutput>;

    fn tick_behavior(&self, input: BehaviorTickInput<'_>)
    -> BehaviorHostResult<BehaviorTickOutput>;

    fn removed_behavior_effects(
        &self,
        catalog: &BehaviorCatalog,
        config: &BTreeMap<String, BehaviorConfigValue>,
        state: &BehaviorInstanceState,
    ) -> BehaviorHostResult<Vec<BehaviorEffect>>;

    fn behavior_accepts_input(
        &self,
        catalog: &BehaviorCatalog,
        config: &BTreeMap<String, BehaviorConfigValue>,
        state: &BehaviorInstanceState,
        kind: u16,
    ) -> BehaviorHostResult<bool>;
}

pub trait BehaviorPack: Send + Sync {
    fn manifest(&self) -> &BehaviorPackManifest;

    fn catalog(&self) -> &BehaviorCatalog;

    fn host(&self) -> &dyn BehaviorHost;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_output_expresses_inventory_changes_as_effects() {
        let state = BehaviorInstanceState::new(
            BehaviorId::new("test:behavior"),
            BehaviorStatus::new("idle"),
        );
        let output = BehaviorTickOutput {
            effects: vec![
                BehaviorEffect::SetState(state),
                BehaviorEffect::TakeInventory {
                    role: BehaviorInventoryRole::Input,
                    stack: BehaviorItemStack { kind: 1, amount: 2 },
                },
                BehaviorEffect::InsertInventory {
                    role: BehaviorInventoryRole::Output,
                    stack: BehaviorItemStack { kind: 2, amount: 1 },
                },
            ],
            metrics: BehaviorTickMetrics::default(),
            output_changed: true,
        };

        assert_eq!(output.effects.len(), 3);
    }

    #[test]
    fn behavior_pack_manifest_describes_registered_behaviors() {
        let manifest = BehaviorPackManifest {
            pack_id: BehaviorPackId::new("vanilla"),
            name: BehaviorPackName::new("Vanilla"),
            version: BehaviorPackVersion::new("0.1.0"),
            api_version: BehaviorApiVersion::new(1),
            content_hash: BehaviorPackContentHash::new("sha256:abc123"),
            behaviors: vec![BehaviorId::new("vanilla:machine")],
        };

        assert_eq!(manifest.pack_id.as_str(), "vanilla");
        assert_eq!(manifest.name.as_str(), "Vanilla");
        assert_eq!(manifest.version.as_str(), "0.1.0");
        assert_eq!(manifest.api_version.value(), 1);
        assert_eq!(manifest.content_hash.as_str(), "sha256:abc123");
        assert_eq!(manifest.behaviors[0].as_str(), "vanilla:machine");
    }

    #[derive(Clone, Debug)]
    struct RoutedBehaviorPack {
        manifest: BehaviorPackManifest,
        catalog: BehaviorCatalog,
        host: RoutedBehaviorHost,
    }

    impl RoutedBehaviorPack {
        fn new(behavior_id: &str, status_prefix: &str) -> Self {
            Self {
                manifest: BehaviorPackManifest {
                    pack_id: BehaviorPackId::new(format!("pack-{behavior_id}")),
                    name: BehaviorPackName::new(format!("Pack {behavior_id}")),
                    version: BehaviorPackVersion::new("0.0.0"),
                    api_version: BehaviorApiVersion::new(1),
                    content_hash: BehaviorPackContentHash::new(format!("hash-{behavior_id}")),
                    behaviors: vec![BehaviorId::new(behavior_id)],
                },
                catalog: BehaviorCatalog::default(),
                host: RoutedBehaviorHost {
                    behavior_id: behavior_id.to_string(),
                    status_prefix: status_prefix.to_string(),
                },
            }
        }
    }

    impl BehaviorPack for RoutedBehaviorPack {
        fn manifest(&self) -> &BehaviorPackManifest {
            &self.manifest
        }

        fn catalog(&self) -> &BehaviorCatalog {
            &self.catalog
        }

        fn host(&self) -> &dyn BehaviorHost {
            &self.host
        }
    }

    #[derive(Clone, Debug)]
    struct RoutedBehaviorHost {
        behavior_id: String,
        status_prefix: String,
    }

    impl BehaviorHost for RoutedBehaviorHost {
        fn initial_behavior_state(
            &self,
            input: BehaviorInitInput<'_>,
        ) -> BehaviorHostResult<BehaviorInstanceState> {
            assert_eq!(input.behavior_id, self.behavior_id);
            Ok(BehaviorInstanceState::new(
                BehaviorId::new(input.behavior_id),
                BehaviorStatus::new(format!("{}:initial", self.status_prefix)),
            ))
        }

        fn apply_behavior_command(
            &self,
            input: BehaviorCommandInput<'_>,
        ) -> BehaviorHostResult<BehaviorCommandOutput> {
            assert_eq!(input.state.behavior_id.as_str(), self.behavior_id);
            Ok(BehaviorCommandOutput {
                effects: vec![BehaviorEffect::SetState(input.state)],
            })
        }

        fn tick_behavior(
            &self,
            input: BehaviorTickInput<'_>,
        ) -> BehaviorHostResult<BehaviorTickOutput> {
            assert_eq!(input.state.behavior_id.as_str(), self.behavior_id);
            Ok(BehaviorTickOutput {
                effects: vec![BehaviorEffect::SetState(BehaviorInstanceState::new(
                    BehaviorId::new(self.behavior_id.clone()),
                    BehaviorStatus::new(format!("{}:tick", self.status_prefix)),
                ))],
                metrics: BehaviorTickMetrics {
                    active_behaviors: 1,
                    ..BehaviorTickMetrics::default()
                },
                output_changed: true,
            })
        }

        fn removed_behavior_effects(
            &self,
            _catalog: &BehaviorCatalog,
            _config: &BTreeMap<String, BehaviorConfigValue>,
            state: &BehaviorInstanceState,
        ) -> BehaviorHostResult<Vec<BehaviorEffect>> {
            assert_eq!(state.behavior_id.as_str(), self.behavior_id);
            Ok(Vec::new())
        }

        fn behavior_accepts_input(
            &self,
            _catalog: &BehaviorCatalog,
            _config: &BTreeMap<String, BehaviorConfigValue>,
            state: &BehaviorInstanceState,
            _kind: u16,
        ) -> BehaviorHostResult<bool> {
            assert_eq!(state.behavior_id.as_str(), self.behavior_id);
            Ok(true)
        }
    }

    #[test]
    fn composite_behavior_pack_routes_each_behavior_to_owning_pack() {
        let composite = CompositeBehaviorPack::new(vec![
            Box::new(RoutedBehaviorPack::new("native:machine", "native")),
            Box::new(RoutedBehaviorPack::new("wasm:machine", "wasm")),
        ])
        .unwrap();

        assert_eq!(
            composite.manifest().behaviors,
            vec![
                BehaviorId::new("native:machine"),
                BehaviorId::new("wasm:machine"),
            ]
        );

        let native_state = composite
            .host()
            .initial_behavior_state(BehaviorInitInput {
                behavior_id: "native:machine",
                config: &BTreeMap::new(),
            })
            .unwrap();
        let wasm_state = composite
            .host()
            .initial_behavior_state(BehaviorInitInput {
                behavior_id: "wasm:machine",
                config: &BTreeMap::new(),
            })
            .unwrap();

        assert_eq!(native_state.status.as_str(), "native:initial");
        assert_eq!(wasm_state.status.as_str(), "wasm:initial");
    }

    #[test]
    fn composite_behavior_pack_rejects_duplicate_behavior_ids() {
        let result = CompositeBehaviorPack::new(vec![
            Box::new(RoutedBehaviorPack::new("duplicate:machine", "first")),
            Box::new(RoutedBehaviorPack::new("duplicate:machine", "second")),
        ]);
        let Err(error) = result else {
            panic!("expected duplicate behavior ids to be rejected");
        };

        assert_eq!(error.kind, BehaviorHostErrorKind::InvalidManifest);
        assert!(
            error
                .message
                .contains("duplicate behavior 'duplicate:machine'")
        );
    }
}
