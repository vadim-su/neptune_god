//! Machine and player inventories: slots, stack limits, insert rules, weight/bulk caps.

use std::collections::BTreeMap;

use crate::catalog::{
    CoreContainerPolicy, CoreInventoryDef, CoreInventoryRole, CoreItemSizeClass, CoreItemStack,
    CoreItemStackLimit,
};
use crate::ids::{ItemInstanceId, ItemKindId};
use behavior_api::{BehaviorInventory, BehaviorInventoryRole, BehaviorItemStack};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SimInventory {
    #[serde(with = "serde_core_inventory_role")]
    role: CoreInventoryRole,
    #[serde(with = "serde_core_item_stack_slots")]
    slots: Vec<Option<CoreItemStack>>,
    #[serde(default)]
    slot_instances: Vec<Option<ItemInstanceId>>,
    max_stack: u32,
    #[serde(default, with = "serde_core_item_stack_limits")]
    stack_limits: Vec<CoreItemStackLimit>,
    #[serde(default)]
    comfortable_weight_limit_grams: Option<u32>,
    #[serde(default)]
    hard_weight_limit_grams: Option<u32>,
    #[serde(default)]
    max_bulk_units: Option<u32>,
    #[serde(default = "default_max_item_size")]
    max_item_size: CoreItemSizeClass,
    #[serde(default)]
    accepts_tags: Vec<String>,
    #[serde(default)]
    rejects_tags: Vec<String>,
    accepts: Vec<ItemKindId>,
    #[serde(default)]
    rejects: Vec<ItemKindId>,
    #[serde(default)]
    pickup_priority: i32,
    #[serde(default)]
    quick_access: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimInventorySnapshot {
    pub role: CoreInventoryRole,
    pub slots: Vec<Option<CoreItemStack>>,
    pub slot_instances: Vec<Option<ItemInstanceId>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertMode {
    AtomicAllOrNothing,
    PartialFit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferMode {
    Manual,
    Inserter,
    SystemDropDrain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryWeightState {
    Normal,
    Overburdened,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InventorySlotEntry {
    pub stack: CoreItemStack,
    pub instance: Option<ItemInstanceId>,
}

impl std::ops::Deref for InventorySlotEntry {
    type Target = CoreItemStack;

    fn deref(&self) -> &Self::Target {
        &self.stack
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryRejection {
    UnknownItem,
    MissingInventory,
    MissingContainer,
    MissingEquipmentSlot,
    SlotOutOfRange,
    ItemNotAccepted,
    ItemTooLarge,
    StackLimitExceeded,
    WeightLimitExceeded,
    BulkLimitExceeded,
    LoadedContainerNotAllowed,
    InserterDepositCapReached,
    CapacityExceeded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryInsertResult {
    pub accepted: Option<CoreItemStack>,
    pub rejected: Option<CoreItemStack>,
    pub rejection: Option<InventoryRejection>,
}

#[derive(Clone, Debug, Default)]
pub struct InventoryItemRules {
    pub weights: BTreeMap<ItemKindId, u32>,
    pub bulk: BTreeMap<ItemKindId, u32>,
    pub size_classes: BTreeMap<ItemKindId, CoreItemSizeClass>,
    pub tags: BTreeMap<ItemKindId, Vec<String>>,
}

impl InventoryItemRules {
    pub fn from_catalog(catalog: &crate::catalog::CoreCatalog) -> Self {
        Self {
            weights: catalog.item_weights(),
            bulk: catalog.item_bulk_units(),
            size_classes: catalog.item_size_classes(),
            tags: catalog.item_tags(),
        }
    }
}

impl SimInventory {
    pub fn from_def(def: &CoreInventoryDef) -> Self {
        Self {
            role: def.role,
            slots: vec![None; def.slots],
            slot_instances: vec![None; def.slots],
            max_stack: def.max_stack,
            stack_limits: def.stack_limits.clone(),
            comfortable_weight_limit_grams: def.comfortable_weight_limit_grams,
            hard_weight_limit_grams: def.hard_weight_limit_grams,
            max_bulk_units: def.max_bulk_units,
            max_item_size: def.max_item_size,
            accepts_tags: def.accepts_tags.clone(),
            rejects_tags: def.rejects_tags.clone(),
            accepts: def.accepts.clone(),
            rejects: def.rejects.clone(),
            pickup_priority: def.pickup_priority,
            quick_access: def.quick_access,
        }
    }

    pub fn from_container_policy(policy: &CoreContainerPolicy) -> Self {
        Self {
            role: CoreInventoryRole::Storage,
            slots: vec![None; policy.slots],
            slot_instances: vec![None; policy.slots],
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

    pub fn role(&self) -> CoreInventoryRole {
        self.role
    }

    pub fn accepts(&self, kind: ItemKindId) -> bool {
        !self.rejects.contains(&kind) && (self.accepts.is_empty() || self.accepts.contains(&kind))
    }

    pub fn has_filters(&self) -> bool {
        !self.accepts_tags.is_empty()
            || !self.rejects_tags.is_empty()
            || !self.accepts.is_empty()
            || !self.rejects.is_empty()
    }

    pub fn max_item_size(&self) -> CoreItemSizeClass {
        self.max_item_size
    }

    pub fn accepts_tags(&self) -> &[String] {
        &self.accepts_tags
    }

    pub fn rejects_tags(&self) -> &[String] {
        &self.rejects_tags
    }

    pub fn accepts_items(&self) -> &[ItemKindId] {
        &self.accepts
    }

    pub fn rejects_items(&self) -> &[ItemKindId] {
        &self.rejects
    }

    pub fn can_accept_stack(&self, stack: CoreItemStack, item_rules: &InventoryItemRules) -> bool {
        let mut candidate = self.clone();
        candidate
            .insert_with_mode(stack, InsertMode::PartialFit, item_rules)
            .accepted
            .is_some()
    }

    pub fn matches_def(&self, def: &CoreInventoryDef) -> bool {
        self.role == def.role
            && self.slots.len() == def.slots
            && self.max_stack == def.max_stack
            && self.stack_limits == def.stack_limits
            && self.comfortable_weight_limit_grams == def.comfortable_weight_limit_grams
            && self.hard_weight_limit_grams == def.hard_weight_limit_grams
            && self.max_bulk_units == def.max_bulk_units
            && self.max_item_size == def.max_item_size
            && self.accepts_tags == def.accepts_tags
            && self.rejects_tags == def.rejects_tags
            && self.accepts == def.accepts
            && self.rejects == def.rejects
            && self.pickup_priority == def.pickup_priority
            && self.quick_access == def.quick_access
    }

    pub fn matches_inventory_shape(&self, expected: &SimInventory) -> bool {
        self.role == expected.role
            && self.slots.len() == expected.slots.len()
            && self.max_stack == expected.max_stack
            && self.stack_limits == expected.stack_limits
            && self.comfortable_weight_limit_grams == expected.comfortable_weight_limit_grams
            && self.hard_weight_limit_grams == expected.hard_weight_limit_grams
            && self.max_bulk_units == expected.max_bulk_units
            && self.max_item_size == expected.max_item_size
            && self.accepts_tags == expected.accepts_tags
            && self.rejects_tags == expected.rejects_tags
            && self.accepts == expected.accepts
            && self.rejects == expected.rejects
            && self.pickup_priority == expected.pickup_priority
            && self.quick_access == expected.quick_access
    }

    #[allow(dead_code, reason = "used by inventory unit tests")]
    fn add(&mut self, stack: CoreItemStack) -> Option<CoreItemStack> {
        self.insert_with_mode(
            stack,
            InsertMode::PartialFit,
            &InventoryItemRules::default(),
        )
        .rejected
    }

    pub fn stack_limit_for(&self, kind: ItemKindId) -> u32 {
        self.stack_limits
            .iter()
            .find(|limit| limit.item == kind)
            .map_or(self.max_stack, |limit| limit.max_stack)
    }

    pub fn total_weight_grams(&self, item_weights: &BTreeMap<ItemKindId, u32>) -> u32 {
        self.slots
            .iter()
            .flatten()
            .map(|stack| {
                item_weights
                    .get(&stack.kind)
                    .copied()
                    .unwrap_or(0)
                    .saturating_mul(stack.amount)
            })
            .fold(0, u32::saturating_add)
    }

    pub fn weight_state(&self, item_weights: &BTreeMap<ItemKindId, u32>) -> InventoryWeightState {
        let Some(comfortable_limit) = self.comfortable_weight_limit_grams else {
            return InventoryWeightState::Normal;
        };
        let total_weight = self.total_weight_grams(item_weights);
        if total_weight > comfortable_limit
            && self
                .hard_weight_limit_grams
                .is_none_or(|hard_limit| total_weight <= hard_limit)
        {
            InventoryWeightState::Overburdened
        } else {
            InventoryWeightState::Normal
        }
    }

    pub fn insert_with_mode(
        &mut self,
        stack: CoreItemStack,
        mode: InsertMode,
        item_rules: &InventoryItemRules,
    ) -> InventoryInsertResult {
        match mode {
            InsertMode::AtomicAllOrNothing => {
                let mut candidate = self.clone();
                let result = candidate.insert_partial_fit(stack, item_rules);
                if result.rejected.is_none() {
                    *self = candidate;
                    result
                } else {
                    InventoryInsertResult {
                        accepted: None,
                        rejected: Some(stack),
                        rejection: result.rejection,
                    }
                }
            }
            InsertMode::PartialFit => self.insert_partial_fit(stack, item_rules),
        }
    }

    pub fn insert_entry_with_mode(
        &mut self,
        entry: InventorySlotEntry,
        mode: InsertMode,
        item_rules: &InventoryItemRules,
    ) -> InventoryInsertResult {
        if entry.instance.is_some() && entry.stack.amount != 1 {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(entry.stack),
                rejection: Some(InventoryRejection::StackLimitExceeded),
            };
        }
        if entry.instance.is_some() {
            return self.insert_instance_entry(entry, mode, item_rules);
        }
        self.insert_with_mode(entry.stack, mode, item_rules)
    }

    pub fn insert_into_slot_with_mode(
        &mut self,
        index: usize,
        stack: CoreItemStack,
        mode: InsertMode,
        item_rules: &InventoryItemRules,
    ) -> InventoryInsertResult {
        match mode {
            InsertMode::AtomicAllOrNothing => {
                let mut candidate = self.clone();
                let result = candidate.insert_into_slot_partial_fit(index, stack, item_rules);
                if result.rejected.is_none() {
                    *self = candidate;
                    result
                } else {
                    InventoryInsertResult {
                        accepted: None,
                        rejected: Some(stack),
                        rejection: result.rejection,
                    }
                }
            }
            InsertMode::PartialFit => self.insert_into_slot_partial_fit(index, stack, item_rules),
        }
    }

    fn insert_instance_entry(
        &mut self,
        entry: InventorySlotEntry,
        mode: InsertMode,
        item_rules: &InventoryItemRules,
    ) -> InventoryInsertResult {
        match mode {
            InsertMode::AtomicAllOrNothing => {
                let mut candidate = self.clone();
                let result = candidate.insert_instance_entry_partial_fit(entry, item_rules);
                if result.rejected.is_none() {
                    *self = candidate;
                    result
                } else {
                    InventoryInsertResult {
                        accepted: None,
                        rejected: Some(entry.stack),
                        rejection: result.rejection,
                    }
                }
            }
            InsertMode::PartialFit => self.insert_instance_entry_partial_fit(entry, item_rules),
        }
    }

    fn insert_instance_entry_partial_fit(
        &mut self,
        entry: InventorySlotEntry,
        item_rules: &InventoryItemRules,
    ) -> InventoryInsertResult {
        if !self.tag_filter_accepts(entry.stack.kind, item_rules) {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(entry.stack),
                rejection: Some(InventoryRejection::ItemNotAccepted),
            };
        }
        if !self.item_size_fits(entry.stack.kind, item_rules) {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(entry.stack),
                rejection: Some(InventoryRejection::ItemTooLarge),
            };
        }
        let physical_capacity = self
            .remaining_item_capacity_by_weight(entry.stack.kind, item_rules)
            .min(self.remaining_item_capacity_by_bulk(entry.stack.kind, item_rules));
        if physical_capacity == 0 {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(entry.stack),
                rejection: Some(self.rejection_for_unfit(entry.stack.kind, item_rules)),
            };
        }

        self.ensure_slot_instances_len();
        let Some(index) = self.slots.iter().position(Option::is_none) else {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(entry.stack),
                rejection: Some(InventoryRejection::CapacityExceeded),
            };
        };
        self.slots[index] = Some(entry.stack);
        self.slot_instances[index] = entry.instance;
        InventoryInsertResult {
            accepted: Some(entry.stack),
            rejected: None,
            rejection: None,
        }
    }

    fn insert_into_slot_partial_fit(
        &mut self,
        index: usize,
        stack: CoreItemStack,
        item_rules: &InventoryItemRules,
    ) -> InventoryInsertResult {
        if stack.amount == 0 {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(InventoryRejection::CapacityExceeded),
            };
        }
        if !self.tag_filter_accepts(stack.kind, item_rules) {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(InventoryRejection::ItemNotAccepted),
            };
        }
        if !self.item_size_fits(stack.kind, item_rules) {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(InventoryRejection::ItemTooLarge),
            };
        }
        if index >= self.slots.len() {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(InventoryRejection::SlotOutOfRange),
            };
        }
        self.ensure_slot_instances_len();

        let max_stack = self.stack_limit_for(stack.kind);
        let slot_capacity = match (&self.slots[index], self.slot_instances[index]) {
            (None, _) => max_stack,
            (Some(existing), None) if existing.kind == stack.kind => {
                max_stack.saturating_sub(existing.amount)
            }
            (Some(_), _) => {
                return InventoryInsertResult {
                    accepted: None,
                    rejected: Some(stack),
                    rejection: Some(InventoryRejection::CapacityExceeded),
                };
            }
        };
        let physical_capacity = self
            .remaining_item_capacity_by_weight(stack.kind, item_rules)
            .min(self.remaining_item_capacity_by_bulk(stack.kind, item_rules));
        let inserted = stack.amount.min(slot_capacity).min(physical_capacity);
        if inserted == 0 {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(self.rejection_for_unfit(stack.kind, item_rules)),
            };
        }

        let slot = &mut self.slots[index];
        match slot {
            Some(existing) => existing.amount += inserted,
            None => {
                *slot = Some(CoreItemStack {
                    kind: stack.kind,
                    amount: inserted,
                });
            }
        }

        let remaining = stack.amount - inserted;
        InventoryInsertResult {
            accepted: Some(CoreItemStack {
                kind: stack.kind,
                amount: inserted,
            }),
            rejected: (remaining > 0).then_some(CoreItemStack {
                kind: stack.kind,
                amount: remaining,
            }),
            rejection: (remaining > 0).then_some(self.rejection_for_unfit(stack.kind, item_rules)),
        }
    }

    fn insert_partial_fit(
        &mut self,
        stack: CoreItemStack,
        item_rules: &InventoryItemRules,
    ) -> InventoryInsertResult {
        if stack.amount == 0 {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(InventoryRejection::CapacityExceeded),
            };
        }
        if !self.tag_filter_accepts(stack.kind, item_rules) {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(InventoryRejection::ItemNotAccepted),
            };
        }
        if !self.item_size_fits(stack.kind, item_rules) {
            return InventoryInsertResult {
                accepted: None,
                rejected: Some(stack),
                rejection: Some(InventoryRejection::ItemTooLarge),
            };
        }

        let mut remaining = stack.amount;
        let mut accepted = 0;
        let mut rejection = None;
        self.fill_existing_stacks(stack.kind, &mut remaining, &mut accepted, item_rules);
        if remaining > 0 {
            self.fill_empty_slots(stack.kind, &mut remaining, &mut accepted, item_rules);
        }
        if remaining > 0 {
            rejection = Some(self.rejection_for_unfit(stack.kind, item_rules));
        }

        InventoryInsertResult {
            accepted: (accepted > 0).then_some(CoreItemStack {
                kind: stack.kind,
                amount: accepted,
            }),
            rejected: (remaining > 0).then_some(CoreItemStack {
                kind: stack.kind,
                amount: remaining,
            }),
            rejection,
        }
    }

    fn fill_existing_stacks(
        &mut self,
        kind: ItemKindId,
        remaining: &mut u32,
        accepted: &mut u32,
        item_rules: &InventoryItemRules,
    ) {
        let max_stack = self.stack_limit_for(kind);
        let mut physical_capacity = self
            .remaining_item_capacity_by_weight(kind, item_rules)
            .min(self.remaining_item_capacity_by_bulk(kind, item_rules));
        self.ensure_slot_instances_len();
        for (slot, instance) in self.slots.iter_mut().zip(&self.slot_instances) {
            if *remaining == 0 || physical_capacity == 0 {
                return;
            }
            if instance.is_some() {
                continue;
            }
            let Some(slot) = slot else {
                continue;
            };
            if slot.kind != kind || slot.amount >= max_stack {
                continue;
            }
            let slot_capacity = max_stack - slot.amount;
            let inserted = slot_capacity.min(*remaining).min(physical_capacity);
            slot.amount += inserted;
            *remaining -= inserted;
            *accepted += inserted;
            physical_capacity -= inserted;
        }
    }

    fn fill_empty_slots(
        &mut self,
        kind: ItemKindId,
        remaining: &mut u32,
        accepted: &mut u32,
        item_rules: &InventoryItemRules,
    ) {
        let max_stack = self.stack_limit_for(kind);
        let mut physical_capacity = self
            .remaining_item_capacity_by_weight(kind, item_rules)
            .min(self.remaining_item_capacity_by_bulk(kind, item_rules));
        self.ensure_slot_instances_len();
        for (slot, instance) in self.slots.iter_mut().zip(self.slot_instances.iter_mut()) {
            if *remaining == 0 || physical_capacity == 0 {
                return;
            }
            if slot.is_some() || instance.is_some() {
                continue;
            }
            let inserted = max_stack.min(*remaining).min(physical_capacity);
            if inserted == 0 {
                return;
            }
            *slot = Some(CoreItemStack {
                kind,
                amount: inserted,
            });
            *remaining -= inserted;
            *accepted += inserted;
            physical_capacity -= inserted;
        }
    }

    fn remaining_item_capacity_by_weight(
        &self,
        kind: ItemKindId,
        item_rules: &InventoryItemRules,
    ) -> u32 {
        let Some(hard_limit) = self.hard_weight_limit_grams else {
            return u32::MAX;
        };
        let item_weight = item_rules.weights.get(&kind).copied().unwrap_or(0);
        if item_weight == 0 {
            return u32::MAX;
        }
        let current_weight = self.total_weight_grams(&item_rules.weights);
        if current_weight >= hard_limit {
            return 0;
        }
        (hard_limit - current_weight) / item_weight
    }

    fn item_size_fits(&self, kind: ItemKindId, item_rules: &InventoryItemRules) -> bool {
        item_rules
            .size_classes
            .get(&kind)
            .is_none_or(|size_class| *size_class <= self.max_item_size)
    }

    fn tag_filter_accepts(&self, kind: ItemKindId, item_rules: &InventoryItemRules) -> bool {
        if !self.accepts(kind) {
            return false;
        }
        let item_tags = item_rules.tags.get(&kind);
        if item_tags.is_some_and(|tags| {
            tags.iter()
                .any(|tag| self.rejects_tags.iter().any(|rejected| rejected == tag))
        }) {
            return false;
        }
        if self.accepts_tags.is_empty() {
            return true;
        }
        item_tags.is_some_and(|tags| {
            tags.iter()
                .any(|tag| self.accepts_tags.iter().any(|accepted| accepted == tag))
        })
    }

    fn total_bulk_units(&self, item_bulk: &BTreeMap<ItemKindId, u32>) -> u32 {
        self.slots
            .iter()
            .flatten()
            .map(|stack| {
                item_bulk
                    .get(&stack.kind)
                    .copied()
                    .unwrap_or(0)
                    .saturating_mul(stack.amount)
            })
            .fold(0, u32::saturating_add)
    }

    fn remaining_item_capacity_by_bulk(
        &self,
        kind: ItemKindId,
        item_rules: &InventoryItemRules,
    ) -> u32 {
        let Some(max_bulk) = self.max_bulk_units else {
            return u32::MAX;
        };
        let item_bulk = item_rules.bulk.get(&kind).copied().unwrap_or(0);
        if item_bulk == 0 {
            return u32::MAX;
        }
        let current_bulk = self.total_bulk_units(&item_rules.bulk);
        if current_bulk >= max_bulk {
            return 0;
        }
        (max_bulk - current_bulk) / item_bulk
    }

    fn rejection_for_unfit(
        &self,
        kind: ItemKindId,
        item_rules: &InventoryItemRules,
    ) -> InventoryRejection {
        if self.remaining_item_capacity_by_weight(kind, item_rules) == 0 {
            return InventoryRejection::WeightLimitExceeded;
        }
        if self.remaining_item_capacity_by_bulk(kind, item_rules) == 0 {
            return InventoryRejection::BulkLimitExceeded;
        }
        if self
            .slots
            .iter()
            .all(|slot| matches!(slot, Some(existing) if existing.kind == kind))
        {
            InventoryRejection::StackLimitExceeded
        } else {
            InventoryRejection::CapacityExceeded
        }
    }

    pub fn remove(&mut self, stack: CoreItemStack) -> bool {
        if self.count_uninstanced(stack.kind) < stack.amount {
            return false;
        }
        let mut remaining = stack.amount;
        self.ensure_slot_instances_len();
        for (slot, instance) in self.slots.iter_mut().zip(&self.slot_instances) {
            if instance.is_some() {
                continue;
            }
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
        remaining == 0
    }

    pub fn take_first_matching(
        &mut self,
        accepts: impl Fn(ItemKindId) -> bool,
    ) -> Option<CoreItemStack> {
        self.ensure_slot_instances_len();
        for (slot, instance) in self.slots.iter_mut().zip(&self.slot_instances) {
            if instance.is_some() {
                continue;
            }
            let Some(existing) = slot else {
                continue;
            };
            if !accepts(existing.kind) {
                continue;
            }
            let taken = CoreItemStack {
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

    pub fn take_slot_amount(&mut self, index: usize, amount: u32) -> Option<CoreItemStack> {
        if amount == 0 {
            return None;
        }
        self.ensure_slot_instances_len();
        if self.slot_instances.get(index).copied().flatten().is_some() {
            return None;
        }
        let slot = self.slots.get_mut(index)?;
        let existing = slot.as_mut()?;
        if existing.amount < amount {
            return None;
        }
        let taken = CoreItemStack {
            kind: existing.kind,
            amount,
        };
        existing.amount -= amount;
        if existing.amount == 0 {
            *slot = None;
            self.slot_instances[index] = None;
        }
        Some(taken)
    }

    pub fn take_slot_entry(&mut self, index: usize, amount: u32) -> Option<InventorySlotEntry> {
        if amount == 0 {
            return None;
        }
        self.ensure_slot_instances_len();
        let instance = self.slot_instances.get(index).copied().flatten();
        if instance.is_some() && amount != 1 {
            return None;
        }
        let slot = self.slots.get_mut(index)?;
        let existing = slot.as_mut()?;
        if existing.amount < amount {
            return None;
        }
        let stack = CoreItemStack {
            kind: existing.kind,
            amount,
        };
        existing.amount -= amount;
        if existing.amount == 0 {
            *slot = None;
            self.slot_instances[index] = None;
        }
        Some(InventorySlotEntry { stack, instance })
    }

    pub fn count(&self, kind: ItemKindId) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|stack| stack.kind == kind)
            .map(|stack| stack.amount)
            .sum()
    }

    fn count_uninstanced(&self, kind: ItemKindId) -> u32 {
        self.slots
            .iter()
            .zip(self.aligned_slot_instances())
            .filter(|(_, instance)| instance.is_none())
            .filter_map(|(slot, _)| *slot)
            .filter(|stack| stack.kind == kind)
            .map(|stack| stack.amount)
            .sum()
    }

    pub fn drain(&mut self) -> Vec<CoreItemStack> {
        self.ensure_slot_instances_len();
        self.slots
            .iter_mut()
            .zip(&self.slot_instances)
            .filter_map(|(slot, instance)| instance.is_none().then(|| slot.take()).flatten())
            .collect()
    }

    pub fn drain_entries(&mut self) -> Vec<InventorySlotEntry> {
        self.ensure_slot_instances_len();
        self.slots
            .iter_mut()
            .zip(self.slot_instances.iter_mut())
            .filter_map(|(slot, instance)| {
                let stack = slot.take()?;
                let instance = instance.take();
                Some(InventorySlotEntry { stack, instance })
            })
            .collect()
    }

    pub fn snapshot(&self) -> SimInventorySnapshot {
        SimInventorySnapshot {
            role: self.role,
            slots: self.slots.clone(),
            slot_instances: self.aligned_slot_instances(),
        }
    }

    pub(crate) fn normalize_slot_instances(&mut self) {
        self.ensure_slot_instances_len();
        for (slot, instance) in self.slots.iter().zip(self.slot_instances.iter_mut()) {
            if slot.is_none() {
                *instance = None;
            }
        }
    }

    pub fn into_behavior_inventory(self) -> BehaviorInventory {
        BehaviorInventory {
            role: behavior_inventory_role(self.role),
            slots: self
                .slots
                .into_iter()
                .map(|slot| {
                    slot.map(|stack| BehaviorItemStack {
                        kind: stack.kind.0,
                        amount: stack.amount,
                    })
                })
                .collect(),
            max_stack: self.max_stack,
            accepts: self.accepts.into_iter().map(|kind| kind.0).collect(),
        }
    }

    pub fn from_behavior_inventory(inventory: BehaviorInventory) -> Self {
        let slot_count = inventory.slots.len();
        Self {
            role: core_inventory_role(inventory.role),
            slots: inventory
                .slots
                .into_iter()
                .map(|slot| {
                    slot.map(|stack| CoreItemStack {
                        kind: ItemKindId(stack.kind),
                        amount: stack.amount,
                    })
                })
                .collect(),
            slot_instances: vec![None; slot_count],
            max_stack: inventory.max_stack,
            stack_limits: Vec::new(),
            comfortable_weight_limit_grams: None,
            hard_weight_limit_grams: None,
            max_bulk_units: None,
            max_item_size: default_max_item_size(),
            accepts_tags: Vec::new(),
            rejects_tags: Vec::new(),
            accepts: inventory.accepts.into_iter().map(ItemKindId).collect(),
            rejects: Vec::new(),
            pickup_priority: 0,
            quick_access: false,
        }
    }

    #[cfg(test)]
    pub fn set_max_stack_for_tests(&mut self, max_stack: u32) {
        self.max_stack = max_stack;
    }

    #[cfg(test)]
    pub fn clear_slot_instances_for_tests(&mut self) {
        self.slot_instances.clear();
    }

    fn ensure_slot_instances_len(&mut self) {
        self.slot_instances.resize(self.slots.len(), None);
    }

    fn aligned_slot_instances(&self) -> Vec<Option<ItemInstanceId>> {
        let mut slot_instances = self.slot_instances.clone();
        slot_instances.resize(self.slots.len(), None);
        slot_instances
    }
}

fn default_max_item_size() -> CoreItemSizeClass {
    CoreItemSizeClass::Oversized
}

fn behavior_inventory_role(role: CoreInventoryRole) -> BehaviorInventoryRole {
    match role {
        CoreInventoryRole::Input => BehaviorInventoryRole::Input,
        CoreInventoryRole::Output => BehaviorInventoryRole::Output,
        CoreInventoryRole::Fuel => BehaviorInventoryRole::Fuel,
        CoreInventoryRole::Storage => BehaviorInventoryRole::Storage,
        CoreInventoryRole::InserterHand => BehaviorInventoryRole::InserterHand,
    }
}

fn core_inventory_role(role: BehaviorInventoryRole) -> CoreInventoryRole {
    match role {
        BehaviorInventoryRole::Input => CoreInventoryRole::Input,
        BehaviorInventoryRole::Output => CoreInventoryRole::Output,
        BehaviorInventoryRole::Fuel => CoreInventoryRole::Fuel,
        BehaviorInventoryRole::Storage => CoreInventoryRole::Storage,
        BehaviorInventoryRole::InserterHand => CoreInventoryRole::InserterHand,
    }
}

mod serde_core_inventory_role {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(role: &CoreInventoryRole, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = match role {
            CoreInventoryRole::Input => "input",
            CoreInventoryRole::Output => "output",
            CoreInventoryRole::Fuel => "fuel",
            CoreInventoryRole::Storage => "storage",
            CoreInventoryRole::InserterHand => "inserter_hand",
        };
        serializer.serialize_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<CoreInventoryRole, D::Error>
    where
        D: Deserializer<'de>,
    {
        match String::deserialize(deserializer)?.as_str() {
            "input" => Ok(CoreInventoryRole::Input),
            "output" => Ok(CoreInventoryRole::Output),
            "fuel" => Ok(CoreInventoryRole::Fuel),
            "storage" => Ok(CoreInventoryRole::Storage),
            "inserter_hand" => Ok(CoreInventoryRole::InserterHand),
            value => Err(serde::de::Error::unknown_variant(
                value,
                &["input", "output", "fuel", "storage", "inserter_hand"],
            )),
        }
    }
}

#[derive(Deserialize, Serialize)]
struct CoreItemStackSerde {
    kind: ItemKindId,
    amount: u32,
}

impl From<CoreItemStack> for CoreItemStackSerde {
    fn from(stack: CoreItemStack) -> Self {
        Self {
            kind: stack.kind,
            amount: stack.amount,
        }
    }
}

impl From<CoreItemStackSerde> for CoreItemStack {
    fn from(stack: CoreItemStackSerde) -> Self {
        Self {
            kind: stack.kind,
            amount: stack.amount,
        }
    }
}

mod serde_core_item_stack_slots {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(slots: &[Option<CoreItemStack>], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        slots
            .iter()
            .map(|slot| slot.map(CoreItemStackSerde::from))
            .collect::<Vec<_>>()
            .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Option<CoreItemStack>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(
            Vec::<Option<CoreItemStackSerde>>::deserialize(deserializer)?
                .into_iter()
                .map(|slot| slot.map(CoreItemStack::from))
                .collect(),
        )
    }
}

#[derive(Deserialize, Serialize)]
struct CoreItemStackLimitSerde {
    item: ItemKindId,
    max_stack: u32,
}

impl From<&CoreItemStackLimit> for CoreItemStackLimitSerde {
    fn from(limit: &CoreItemStackLimit) -> Self {
        Self {
            item: limit.item,
            max_stack: limit.max_stack,
        }
    }
}

impl From<CoreItemStackLimitSerde> for CoreItemStackLimit {
    fn from(limit: CoreItemStackLimitSerde) -> Self {
        Self {
            item: limit.item,
            max_stack: limit.max_stack,
        }
    }
}

mod serde_core_item_stack_limits {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(limits: &[CoreItemStackLimit], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        limits
            .iter()
            .map(CoreItemStackLimitSerde::from)
            .collect::<Vec<_>>()
            .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<CoreItemStackLimit>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Vec::<CoreItemStackLimitSerde>::deserialize(deserializer)?
            .into_iter()
            .map(CoreItemStackLimit::from)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::catalog::{
        CoreItemStackLimit, TEST_COAL, TEST_COPPER_ORE, TEST_COPPER_PLATE, TEST_IRON_ORE,
        TEST_IRON_PLATE, TEST_WOOD,
    };

    fn test_item_weights() -> BTreeMap<ItemKindId, u32> {
        BTreeMap::from([
            (TEST_IRON_ORE, 500),
            (TEST_COPPER_ORE, 500),
            (TEST_COAL, 500),
            (TEST_IRON_PLATE, 1_000),
            (TEST_COPPER_PLATE, 1_000),
        ])
    }

    fn test_item_rules() -> InventoryItemRules {
        InventoryItemRules {
            weights: test_item_weights(),
            ..InventoryItemRules::default()
        }
    }

    #[test]
    fn inventory_accepts_merges_and_rejects_by_role_definition() {
        let def = CoreInventoryDef {
            role: CoreInventoryRole::Fuel,
            slots: 1,
            max_stack: 2,
            stack_limits: Vec::new(),
            comfortable_weight_limit_grams: None,
            hard_weight_limit_grams: None,
            accepts: vec![TEST_COAL],
            ..CoreInventoryDef::new(CoreInventoryRole::Fuel, 1, 2)
        };
        let mut inventory = SimInventory::from_def(&def);

        assert_eq!(
            inventory.add(CoreItemStack {
                kind: TEST_COAL,
                amount: 1
            }),
            None
        );
        assert_eq!(
            inventory.add(CoreItemStack {
                kind: TEST_COAL,
                amount: 2
            }),
            Some(CoreItemStack {
                kind: TEST_COAL,
                amount: 1
            })
        );
        assert_eq!(
            inventory.add(CoreItemStack {
                kind: TEST_WOOD,
                amount: 1
            }),
            Some(CoreItemStack {
                kind: TEST_WOOD,
                amount: 1
            })
        );
        assert_eq!(inventory.count(TEST_COAL), 2);
    }

    #[test]
    fn inventory_add_uses_per_item_stack_limit() {
        let def = CoreInventoryDef {
            role: CoreInventoryRole::Storage,
            slots: 2,
            max_stack: 10,
            stack_limits: vec![CoreItemStackLimit {
                item: TEST_IRON_ORE,
                max_stack: 3,
            }],
            comfortable_weight_limit_grams: Some(1_000),
            hard_weight_limit_grams: Some(2_000),
            accepts: Vec::new(),
            ..CoreInventoryDef::new(CoreInventoryRole::Storage, 2, 10)
        };
        let mut inventory = SimInventory::from_def(&def);

        assert_eq!(
            inventory.add(CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 8,
            }),
            Some(CoreItemStack {
                kind: TEST_IRON_ORE,
                amount: 2,
            })
        );
        assert_eq!(inventory.count(TEST_IRON_ORE), 6);
    }

    #[test]
    fn removing_more_than_available_does_not_partially_mutate_inventory() {
        let def = CoreInventoryDef::new(CoreInventoryRole::Storage, 1, 100);
        let mut inventory = SimInventory::from_def(&def);
        assert_eq!(
            inventory.add(CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 2,
            }),
            None
        );

        assert!(!inventory.remove(CoreItemStack {
            kind: TEST_IRON_PLATE,
            amount: 3,
        }));
        assert_eq!(inventory.count(TEST_IRON_PLATE), 2);
    }

    #[test]
    fn stack_only_take_does_not_strip_item_instance() {
        let def = CoreInventoryDef::new(CoreInventoryRole::Storage, 1, 100);
        let mut inventory = SimInventory::from_def(&def);
        let entry = InventorySlotEntry {
            stack: CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 1,
            },
            instance: Some(ItemInstanceId(7)),
        };
        assert_eq!(
            inventory
                .insert_entry_with_mode(
                    entry,
                    InsertMode::AtomicAllOrNothing,
                    &InventoryItemRules::default()
                )
                .rejected,
            None
        );

        assert_eq!(inventory.take_slot_amount(0, 1), None);
        assert_eq!(
            inventory.snapshot().slot_instances[0],
            Some(ItemInstanceId(7))
        );
        assert_eq!(inventory.take_slot_entry(0, 1), Some(entry));
    }

    #[test]
    fn stack_only_drain_does_not_strip_item_instances() {
        let def = CoreInventoryDef::new(CoreInventoryRole::Storage, 2, 100);
        let mut inventory = SimInventory::from_def(&def);
        let instanced_entry = InventorySlotEntry {
            stack: CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 1,
            },
            instance: Some(ItemInstanceId(7)),
        };
        assert_eq!(
            inventory
                .insert_entry_with_mode(
                    instanced_entry,
                    InsertMode::AtomicAllOrNothing,
                    &InventoryItemRules::default()
                )
                .rejected,
            None
        );
        assert_eq!(
            inventory.add(CoreItemStack {
                kind: TEST_COPPER_PLATE,
                amount: 2,
            }),
            None
        );

        assert_eq!(
            inventory.drain(),
            vec![CoreItemStack {
                kind: TEST_COPPER_PLATE,
                amount: 2,
            }]
        );
        assert_eq!(inventory.take_slot_entry(0, 1), Some(instanced_entry));
    }

    #[test]
    fn entry_drain_preserves_item_instances() {
        let def = CoreInventoryDef::new(CoreInventoryRole::Storage, 1, 100);
        let mut inventory = SimInventory::from_def(&def);
        let entry = InventorySlotEntry {
            stack: CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 1,
            },
            instance: Some(ItemInstanceId(7)),
        };
        assert_eq!(
            inventory
                .insert_entry_with_mode(
                    entry,
                    InsertMode::AtomicAllOrNothing,
                    &InventoryItemRules::default()
                )
                .rejected,
            None
        );

        assert_eq!(inventory.drain_entries(), vec![entry]);
        assert_eq!(inventory.snapshot().slots, vec![None]);
        assert_eq!(inventory.snapshot().slot_instances, vec![None]);
    }

    #[test]
    fn per_item_stack_limit_overrides_inventory_default() {
        let def = CoreInventoryDef {
            role: CoreInventoryRole::Storage,
            slots: 1,
            max_stack: 100,
            stack_limits: vec![CoreItemStackLimit {
                item: TEST_IRON_PLATE,
                max_stack: 10,
            }],
            comfortable_weight_limit_grams: None,
            hard_weight_limit_grams: None,
            accepts: Vec::new(),
            ..CoreInventoryDef::new(CoreInventoryRole::Storage, 1, 100)
        };
        let mut inventory = SimInventory::from_def(&def);

        let result = inventory.insert_with_mode(
            CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 11,
            },
            InsertMode::PartialFit,
            &test_item_rules(),
        );

        assert_eq!(
            result,
            InventoryInsertResult {
                accepted: Some(CoreItemStack {
                    kind: TEST_IRON_PLATE,
                    amount: 10,
                }),
                rejected: Some(CoreItemStack {
                    kind: TEST_IRON_PLATE,
                    amount: 1,
                }),
                rejection: Some(InventoryRejection::StackLimitExceeded),
            }
        );
        assert_eq!(inventory.count(TEST_IRON_PLATE), 10);
    }

    #[test]
    fn player_inventory_reports_overburden_and_rejects_above_hard_cap() {
        let def = CoreInventoryDef {
            role: CoreInventoryRole::Storage,
            slots: 1,
            max_stack: 100,
            stack_limits: Vec::new(),
            comfortable_weight_limit_grams: Some(40_000),
            hard_weight_limit_grams: Some(46_000),
            accepts: Vec::new(),
            ..CoreInventoryDef::new(CoreInventoryRole::Storage, 1, 100)
        };
        let mut inventory = SimInventory::from_def(&def);
        let item_rules = test_item_rules();

        let first_insert = inventory.insert_with_mode(
            CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 45,
            },
            InsertMode::AtomicAllOrNothing,
            &item_rules,
        );
        assert_eq!(
            first_insert,
            InventoryInsertResult {
                accepted: Some(CoreItemStack {
                    kind: TEST_IRON_PLATE,
                    amount: 45,
                }),
                rejected: None,
                rejection: None,
            }
        );
        assert_eq!(
            inventory.weight_state(&item_rules.weights),
            InventoryWeightState::Overburdened
        );

        let rejected_insert = inventory.insert_with_mode(
            CoreItemStack {
                kind: TEST_IRON_PLATE,
                amount: 2,
            },
            InsertMode::AtomicAllOrNothing,
            &item_rules,
        );
        assert_eq!(
            rejected_insert,
            InventoryInsertResult {
                accepted: None,
                rejected: Some(CoreItemStack {
                    kind: TEST_IRON_PLATE,
                    amount: 2,
                }),
                rejection: Some(InventoryRejection::WeightLimitExceeded),
            }
        );
        assert_eq!(inventory.count(TEST_IRON_PLATE), 45);
    }

    #[test]
    fn inventory_definition_match_includes_policy_fields() {
        let def = CoreInventoryDef {
            role: CoreInventoryRole::Storage,
            slots: 1,
            max_stack: 10,
            stack_limits: vec![CoreItemStackLimit {
                item: TEST_IRON_ORE,
                max_stack: 3,
            }],
            comfortable_weight_limit_grams: Some(1_000),
            hard_weight_limit_grams: Some(2_000),
            max_bulk_units: Some(3),
            accepts: Vec::new(),
            ..CoreInventoryDef::new(CoreInventoryRole::Storage, 1, 10)
        };
        let inventory = SimInventory::from_def(&def);
        assert!(inventory.matches_def(&def));

        let mut changed_stack_limit = def.clone();
        changed_stack_limit.stack_limits[0].max_stack += 1;
        assert!(!inventory.matches_def(&changed_stack_limit));

        let mut changed_comfort_limit = def.clone();
        changed_comfort_limit.comfortable_weight_limit_grams = Some(1_001);
        assert!(!inventory.matches_def(&changed_comfort_limit));

        let mut changed_hard_limit = def.clone();
        changed_hard_limit.hard_weight_limit_grams = Some(2_001);
        assert!(!inventory.matches_def(&changed_hard_limit));

        let mut changed_bulk_limit = def;
        changed_bulk_limit.max_bulk_units = Some(4);
        assert!(!inventory.matches_def(&changed_bulk_limit));
    }
}
