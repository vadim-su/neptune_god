//! Player character equipment, nested containers, and routing between sections.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::catalog::{
    CoreCatalog, CoreContainerPolicy, CoreInventoryDef, CoreInventoryRole, CoreItemSizeClass,
    CoreItemStack, CoreProvidedContainerDef,
};
use crate::ids::ItemKindId;
use crate::inventory::{
    InventoryInsertResult, InventoryItemRules, InventoryRejection, InventorySlotEntry, SimInventory,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CharacterContainerId(pub String);

impl CharacterContainerId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct EquipmentSlotId(pub String);

impl EquipmentSlotId {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct EquippedItem {
    pub slot: EquipmentSlotId,
    pub item: ItemKindId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterEquipmentEntry {
    pub slot: EquipmentSlotId,
    pub item: ItemKindId,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SimCharacterContainer {
    pub id: CharacterContainerId,
    pub name: String,
    pub source_slot: EquipmentSlotId,
    pub source_item: ItemKindId,
    pub inventory: SimInventory,
    pub max_weight_grams: Option<u32>,
    pub max_bulk_units: Option<u32>,
    pub pickup_priority: i32,
    pub quick_access: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CharacterContainerSection {
    pub container_id: CharacterContainerId,
    pub name: String,
    pub slots: Vec<Option<CoreItemStack>>,
    pub used_slots: usize,
    pub total_slots: usize,
    pub total_weight_grams: u32,
    pub max_weight_grams: Option<u32>,
    pub total_bulk_units: u32,
    pub max_bulk_units: Option<u32>,
    pub max_item_size: CoreItemSizeClass,
    pub accepts_tags: Vec<String>,
    pub rejects_tags: Vec<String>,
    pub accepts_items: Vec<ItemKindId>,
    pub rejects_items: Vec<ItemKindId>,
    pub pickup_priority: i32,
    pub quick_access: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LoadedContainerInstance {
    pub item: ItemKindId,
    pub containers: Vec<LoadedContainerSection>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LoadedContainerSection {
    pub container_id: CharacterContainerId,
    pub inventory: SimInventory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterEquipResult {
    pub equipped: Option<CoreItemStack>,
    pub replaced: Option<InventorySlotEntry>,
    pub rejected: Option<InventorySlotEntry>,
    pub rejection: Option<InventoryRejection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterRouteResult {
    pub accepted: Option<CoreItemStack>,
    pub rejected: Option<CoreItemStack>,
    pub rejection: Option<InventoryRejection>,
    pub accepted_container: Option<String>,
}

impl From<InventoryInsertResult> for CharacterRouteResult {
    fn from(result: InventoryInsertResult) -> Self {
        Self {
            accepted: result.accepted,
            rejected: result.rejected,
            rejection: result.rejection,
            accepted_container: None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct SimCharacterInventory {
    pub equipment: BTreeMap<EquipmentSlotId, EquippedItem>,
    pub containers: Vec<SimCharacterContainer>,
}

impl SimCharacterInventory {
    pub fn from_catalog(catalog: &CoreCatalog) -> Self {
        let mut character = Self::default();
        for starting in &catalog.personal_inventories.starting_equipment {
            if character
                .equipment
                .contains_key(&EquipmentSlotId(starting.slot.clone()))
            {
                continue;
            }
            let Some(item) = catalog.item(starting.item) else {
                continue;
            };
            let Some(equipment) = &item.equipment else {
                continue;
            };
            let slot = EquipmentSlotId(starting.slot.clone());
            character.equipment.insert(
                slot.clone(),
                EquippedItem {
                    slot: slot.clone(),
                    item: starting.item,
                },
            );
            for provided in &equipment.provides_containers {
                if character
                    .containers
                    .iter()
                    .any(|container| container.id.as_str() == provided.id.as_str())
                {
                    continue;
                }
                character.containers.push(container_from_def(
                    provided,
                    slot.clone(),
                    starting.item,
                ));
            }
        }
        character
            .containers
            .sort_by_key(|container| std::cmp::Reverse(container.pickup_priority));
        character
    }

    pub fn container(&self, id: &str) -> Option<&SimCharacterContainer> {
        self.containers
            .iter()
            .find(|container| container.id.as_str() == id)
    }

    pub fn container_mut(&mut self, id: &str) -> Option<&mut SimCharacterContainer> {
        self.containers
            .iter_mut()
            .find(|container| container.id.as_str() == id)
    }

    pub fn route_order_for_stack(
        &self,
        stack: CoreItemStack,
        rules: &InventoryItemRules,
    ) -> Vec<String> {
        let mut candidates = self
            .containers
            .iter()
            .filter(|container| container.inventory.can_accept_stack(stack, rules))
            .map(|container| {
                let specialized = container.inventory.has_filters();
                (
                    !specialized,
                    !container.quick_access,
                    -container.pickup_priority,
                    container.id.as_str().to_string(),
                )
            })
            .collect::<Vec<_>>();
        candidates.sort();
        candidates.into_iter().map(|(_, _, _, id)| id).collect()
    }

    pub fn matches_catalog(&self, catalog: &CoreCatalog) -> bool {
        if !self.has_unique_container_ids() {
            return false;
        }

        let mut expected_containers = Vec::new();
        for (slot, equipped) in &self.equipment {
            if &equipped.slot != slot {
                return false;
            }
            let Some(item) = catalog.item(equipped.item) else {
                return false;
            };
            let Some(equipment) = &item.equipment else {
                return false;
            };
            if equipment.slot.as_str() != slot.as_str() {
                return false;
            }
            for provided in &equipment.provides_containers {
                expected_containers.push(container_from_def(provided, slot.clone(), equipped.item));
            }
        }
        expected_containers.sort_by_key(|container| std::cmp::Reverse(container.pickup_priority));

        self.containers.len() == expected_containers.len()
            && self
                .containers
                .iter()
                .zip(&expected_containers)
                .all(|(actual, expected)| actual.matches_definition(expected))
    }

    fn has_unique_container_ids(&self) -> bool {
        let mut seen = BTreeSet::new();
        self.containers
            .iter()
            .all(|container| seen.insert(container.id.as_str()))
    }
}

impl SimCharacterContainer {
    pub fn section_snapshot(&self, item_rules: &InventoryItemRules) -> CharacterContainerSection {
        let slots = self.inventory.snapshot().slots;
        let total_weight_grams = slots
            .iter()
            .flatten()
            .map(|stack| {
                item_rules
                    .weights
                    .get(&stack.kind)
                    .copied()
                    .unwrap_or(0)
                    .saturating_mul(stack.amount)
            })
            .fold(0, u32::saturating_add);
        let total_bulk_units = slots
            .iter()
            .flatten()
            .map(|stack| {
                item_rules
                    .bulk
                    .get(&stack.kind)
                    .copied()
                    .unwrap_or(0)
                    .saturating_mul(stack.amount)
            })
            .fold(0, u32::saturating_add);
        CharacterContainerSection {
            container_id: self.id.clone(),
            name: self.name.clone(),
            used_slots: slots.iter().filter(|slot| slot.is_some()).count(),
            total_slots: slots.len(),
            slots,
            total_weight_grams,
            max_weight_grams: self.max_weight_grams,
            total_bulk_units,
            max_bulk_units: self.max_bulk_units,
            max_item_size: self.inventory.max_item_size(),
            accepts_tags: self.inventory.accepts_tags().to_vec(),
            rejects_tags: self.inventory.rejects_tags().to_vec(),
            accepts_items: self.inventory.accepts_items().to_vec(),
            rejects_items: self.inventory.rejects_items().to_vec(),
            pickup_priority: self.pickup_priority,
            quick_access: self.quick_access,
        }
    }

    fn matches_definition(&self, expected: &SimCharacterContainer) -> bool {
        self.id == expected.id
            && self.name == expected.name
            && self.source_slot == expected.source_slot
            && self.source_item == expected.source_item
            && self.pickup_priority == expected.pickup_priority
            && self.quick_access == expected.quick_access
            && self.max_weight_grams == expected.max_weight_grams
            && self.max_bulk_units == expected.max_bulk_units
            && self.inventory.matches_inventory_shape(&expected.inventory)
    }
}

pub(crate) fn container_from_def(
    provided: &CoreProvidedContainerDef,
    source_slot: EquipmentSlotId,
    source_item: ItemKindId,
) -> SimCharacterContainer {
    SimCharacterContainer {
        id: CharacterContainerId::new(&provided.id),
        name: provided.name.clone(),
        source_slot,
        source_item,
        inventory: SimInventory::from_def(&inventory_def_from_policy(&provided.policy)),
        max_weight_grams: provided.policy.hard_weight_limit_grams,
        max_bulk_units: provided.policy.max_bulk_units,
        pickup_priority: provided.policy.pickup_priority,
        quick_access: provided.policy.quick_access,
    }
}

fn inventory_def_from_policy(policy: &CoreContainerPolicy) -> CoreInventoryDef {
    CoreInventoryDef {
        role: CoreInventoryRole::Storage,
        slots: policy.slots,
        max_stack: policy.max_stack,
        stack_limits: policy.stack_limits.clone(),
        comfortable_weight_limit_grams: policy.comfortable_weight_limit_grams,
        hard_weight_limit_grams: policy.hard_weight_limit_grams,
        max_bulk_units: policy.max_bulk_units,
        max_item_size: policy.max_item_size,
        accepts_tags: policy.accepts_tags.clone(),
        rejects_tags: policy.rejects_tags.clone(),
        accepts: policy.accepts_items.clone(),
        rejects: policy.rejects_items.clone(),
        pickup_priority: policy.pickup_priority,
        quick_access: policy.quick_access,
    }
}
