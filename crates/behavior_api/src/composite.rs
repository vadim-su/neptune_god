//! Routes behavior ids to multiple [`BehaviorPack`] implementations.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    BehaviorApiVersion, BehaviorCatalog, BehaviorCommandInput, BehaviorCommandOutput,
    BehaviorConfigValue, BehaviorEffect, BehaviorHost, BehaviorHostError, BehaviorHostErrorKind,
    BehaviorHostResult, BehaviorId, BehaviorInitInput, BehaviorInstanceState, BehaviorPack,
    BehaviorPackContentHash, BehaviorPackId, BehaviorPackManifest, BehaviorPackName,
    BehaviorPackVersion, BehaviorTickInput, BehaviorTickOutput,
};

/// Delegates each behavior id to one child pack; merges catalogs for install.
pub struct CompositeBehaviorPack {
    manifest: BehaviorPackManifest,
    catalog: BehaviorCatalog,
    routes: BTreeMap<BehaviorId, usize>,
    packs: Vec<Box<dyn BehaviorPack>>,
}

impl CompositeBehaviorPack {
    pub fn new(packs: Vec<Box<dyn BehaviorPack>>) -> BehaviorHostResult<Self> {
        let mut routes = BTreeMap::new();
        let mut behaviors = Vec::new();
        let mut items = Vec::new();
        let mut recipes = Vec::new();
        let mut content_hash_parts = Vec::new();
        let mut api_versions = BTreeSet::new();

        for (pack_index, pack) in packs.iter().enumerate() {
            let manifest = pack.manifest();
            api_versions.insert(manifest.api_version);
            content_hash_parts.push(format!(
                "{}@{}:{}",
                manifest.pack_id.as_str(),
                manifest.version.as_str(),
                manifest.content_hash.as_str()
            ));
            items.extend(pack.catalog().items.clone());
            recipes.extend(pack.catalog().recipes.clone());

            for behavior_id in &manifest.behaviors {
                if routes.insert(behavior_id.clone(), pack_index).is_some() {
                    return Err(BehaviorHostError::new(
                        BehaviorHostErrorKind::InvalidManifest,
                        format!("duplicate behavior '{}'", behavior_id.as_str()),
                    ));
                }
                behaviors.push(behavior_id.clone());
            }
        }

        let api_version = api_versions
            .iter()
            .next()
            .copied()
            .unwrap_or_else(|| BehaviorApiVersion::new(1));
        if api_versions.len() > 1 {
            return Err(BehaviorHostError::new(
                BehaviorHostErrorKind::InvalidManifest,
                "behavior packs use incompatible API versions",
            ));
        }

        Ok(Self {
            manifest: BehaviorPackManifest {
                pack_id: BehaviorPackId::new("composite"),
                name: BehaviorPackName::new("Composite Behavior Pack"),
                version: BehaviorPackVersion::new("0.0.0"),
                api_version,
                content_hash: BehaviorPackContentHash::new(format!(
                    "composite:{}",
                    content_hash_parts.join("|")
                )),
                behaviors,
            },
            catalog: BehaviorCatalog { items, recipes },
            routes,
            packs,
        })
    }

    pub fn pack_manifests(&self) -> impl Iterator<Item = &BehaviorPackManifest> {
        self.packs.iter().map(|pack| pack.manifest())
    }

    fn pack_for_behavior(&self, behavior_id: &str) -> BehaviorHostResult<&dyn BehaviorPack> {
        let behavior_id = BehaviorId::new(behavior_id);
        let Some(pack_index) = self.routes.get(&behavior_id) else {
            return Err(BehaviorHostError::missing_behavior(behavior_id.as_str()));
        };
        Ok(self.packs[*pack_index].as_ref())
    }
}

impl Default for CompositeBehaviorPack {
    fn default() -> Self {
        Self::new(Vec::new()).expect("empty composite behavior pack is valid")
    }
}

impl BehaviorPack for CompositeBehaviorPack {
    fn manifest(&self) -> &BehaviorPackManifest {
        &self.manifest
    }

    fn catalog(&self) -> &BehaviorCatalog {
        &self.catalog
    }

    fn host(&self) -> &dyn BehaviorHost {
        self
    }
}

impl BehaviorHost for CompositeBehaviorPack {
    fn initial_behavior_state(
        &self,
        input: BehaviorInitInput<'_>,
    ) -> BehaviorHostResult<BehaviorInstanceState> {
        self.pack_for_behavior(input.behavior_id)?
            .host()
            .initial_behavior_state(input)
    }

    fn apply_behavior_command(
        &self,
        input: BehaviorCommandInput<'_>,
    ) -> BehaviorHostResult<BehaviorCommandOutput> {
        let pack = self.pack_for_behavior(input.state.behavior_id.as_str())?;
        pack.host().apply_behavior_command(BehaviorCommandInput {
            catalog: pack.catalog(),
            building: input.building,
            config: input.config,
            state: input.state,
            command: input.command,
            inventories: input.inventories,
        })
    }

    fn tick_behavior(
        &self,
        input: BehaviorTickInput<'_>,
    ) -> BehaviorHostResult<BehaviorTickOutput> {
        let pack = self.pack_for_behavior(input.state.behavior_id.as_str())?;
        pack.host().tick_behavior(BehaviorTickInput {
            catalog: pack.catalog(),
            building: input.building,
            config: input.config,
            state: input.state,
            inventories: input.inventories,
            resources: input.resources,
            power: input.power,
            tick: input.tick,
        })
    }

    fn removed_behavior_effects(
        &self,
        _catalog: &BehaviorCatalog,
        config: &BTreeMap<String, BehaviorConfigValue>,
        state: &BehaviorInstanceState,
    ) -> BehaviorHostResult<Vec<BehaviorEffect>> {
        let pack = self.pack_for_behavior(state.behavior_id.as_str())?;
        pack.host()
            .removed_behavior_effects(pack.catalog(), config, state)
    }

    fn behavior_accepts_input(
        &self,
        _catalog: &BehaviorCatalog,
        config: &BTreeMap<String, BehaviorConfigValue>,
        state: &BehaviorInstanceState,
        kind: u16,
    ) -> BehaviorHostResult<bool> {
        let pack = self.pack_for_behavior(state.behavior_id.as_str())?;
        pack.host()
            .behavior_accepts_input(pack.catalog(), config, state, kind)
    }
}
