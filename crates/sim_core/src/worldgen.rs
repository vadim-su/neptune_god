//! Deterministic terrain/resource generation shared by render shells.

use std::collections::{BTreeMap, HashSet};

use crate::ids::TilePos;

pub const DEFAULT_WORLD_SEED: u64 = 0x6E65_7074_756E_6501;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedTerrainTile {
    pub pos: TilePos,
    pub terrain_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedResourceTile {
    pub pos: TilePos,
    pub item_def_id: String,
    pub amount: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedMapRegion {
    pub min: TilePos,
    pub max: TilePos,
    pub terrain_tiles: Vec<GeneratedTerrainTile>,
    pub resource_tiles: Vec<GeneratedResourceTile>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntRange {
    pub min: u32,
    pub max: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistanceCurveDef {
    pub start_distance: u32,
    pub end_distance: u32,
    pub multiplier_at_end: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResourceFrequencyDef {
    pub base: u32,
    pub distance_curve: DistanceCurveDef,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfileRangePair<T> {
    pub near: T,
    pub far: T,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartingAreaDef {
    pub radius: u32,
    pub terrain: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainLayerDef {
    pub terrain: String,
    pub threshold: f32,
    pub scale: f64,
    pub min_distance_from_spawn: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResourceRuleDef {
    pub resource: String,
    pub item_def_id: String,
    pub allowed_terrains: Vec<String>,
    pub patch_frequency: ResourceFrequencyDef,
    pub patch_radius: ProfileRangePair<IntRange>,
    pub richness: ProfileRangePair<IntRange>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartingResourceDef {
    pub resource: String,
    pub item_def_id: String,
    pub patch_count: u32,
    pub distance_range: IntRange,
    pub radius_range: IntRange,
    pub amount_range: IntRange,
    pub allowed_terrains: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldGenProfile {
    pub id: String,
    pub starting_area: StartingAreaDef,
    pub terrain_layers: Vec<TerrainLayerDef>,
    pub resources: Vec<ResourceRuleDef>,
    pub starting_resources: Vec<StartingResourceDef>,
}

pub struct WorldGenerator {
    seed: u64,
    profile: WorldGenProfile,
    terrain_fields: BTreeMap<String, NoiseField>,
}

impl WorldGenerator {
    pub fn new(seed: u64, profile: WorldGenProfile) -> Self {
        let terrain_fields = profile
            .terrain_layers
            .iter()
            .map(|layer| {
                (
                    layer.terrain.clone(),
                    NoiseField::new(seed, &layer.terrain, layer.scale),
                )
            })
            .collect();

        Self {
            seed,
            profile,
            terrain_fields,
        }
    }

    pub fn new_default(seed: u64) -> Self {
        Self::new(seed, default_profile())
    }

    pub fn generate_square_around_spawn(&self, radius: i32) -> GeneratedMapRegion {
        let radius = radius.max(1);
        self.generate_rect(TilePos::new(-radius, -radius), TilePos::new(radius, radius))
    }

    pub fn generate_rect(&self, min: TilePos, max: TilePos) -> GeneratedMapRegion {
        let raw_min = min;
        let raw_max = max;
        let min = TilePos::new(raw_min.x.min(raw_max.x), raw_min.y.min(raw_max.y));
        let max = TilePos::new(raw_min.x.max(raw_max.x), raw_min.y.max(raw_max.y));
        let mut terrain_tiles = Vec::with_capacity(
            ((max.x - min.x + 1) as usize).saturating_mul((max.y - min.y + 1) as usize),
        );
        let mut terrain_by_pos = BTreeMap::new();

        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let pos = TilePos::new(x, y);
                let terrain_id = self.terrain_at(pos);
                terrain_by_pos.insert(pos, terrain_id.clone());
                terrain_tiles.push(GeneratedTerrainTile { pos, terrain_id });
            }
        }

        let mut occupied_resources = HashSet::new();
        let mut resource_tiles =
            self.starting_resource_tiles(&terrain_by_pos, &mut occupied_resources, min, max);
        resource_tiles.extend(self.regular_resource_tiles(
            &terrain_by_pos,
            &mut occupied_resources,
            min,
            max,
        ));
        resource_tiles.sort_by_key(|tile| (tile.pos.x, tile.pos.y, tile.item_def_id.clone()));

        GeneratedMapRegion {
            min,
            max,
            terrain_tiles,
            resource_tiles,
        }
    }

    fn terrain_at(&self, pos: TilePos) -> String {
        if is_inside_starting_area(&self.profile, pos) {
            return self.profile.starting_area.terrain.clone();
        }

        let distance = distance_from_spawn(pos);
        for layer in &self.profile.terrain_layers {
            if distance < layer.min_distance_from_spawn as f32 {
                continue;
            }

            let Some(field) = self.terrain_fields.get(&layer.terrain) else {
                continue;
            };
            if field.sample(pos.x, pos.y) >= layer.threshold {
                return layer.terrain.clone();
            }
        }

        "ground".to_string()
    }

    fn starting_resource_tiles(
        &self,
        terrain_by_pos: &BTreeMap<TilePos, String>,
        occupied_resources: &mut HashSet<TilePos>,
        min: TilePos,
        max: TilePos,
    ) -> Vec<GeneratedResourceTile> {
        let mut tiles = Vec::new();
        for rule in &self.profile.starting_resources {
            for patch_index in 0..rule.patch_count {
                let center =
                    self.starting_patch_center(&rule.resource, patch_index, rule.distance_range);
                let radius = sample_range(
                    hash_coords(self.seed, center.x, center.y),
                    rule.radius_range,
                );
                let amount = sample_range(
                    hash_coords(self.seed.rotate_left(17), center.x, center.y),
                    rule.amount_range,
                );
                append_patch_tiles(
                    &mut tiles,
                    PatchPlacement {
                        terrain_by_pos,
                        occupied_resources,
                        center,
                        radius,
                        allowed_terrains: &rule.allowed_terrains,
                        item_def_id: &rule.item_def_id,
                        amount,
                        min,
                        max,
                    },
                );
            }
        }
        tiles
    }

    fn starting_patch_center(&self, key: &str, patch_index: u32, range: IntRange) -> TilePos {
        let seed = mix_seed_with_key(self.seed, key).wrapping_add(u64::from(patch_index));
        let distance = sample_range(seed, range) as i32;
        let lateral = sample_range(seed.rotate_left(13), IntRange { min: 0, max: 4 }) as i32;

        match seed % 3 {
            0 => TilePos::new(distance, lateral),
            1 => TilePos::new(lateral, distance),
            _ => TilePos::new(distance, distance),
        }
    }

    fn regular_resource_tiles(
        &self,
        terrain_by_pos: &BTreeMap<TilePos, String>,
        occupied_resources: &mut HashSet<TilePos>,
        min: TilePos,
        max: TilePos,
    ) -> Vec<GeneratedResourceTile> {
        let mut tiles = Vec::new();
        for rule in &self.profile.resources {
            let spacing = i64::from(rule.patch_frequency.base.max(1));
            let max_radius = i64::from(rule.patch_radius.far.max.max(rule.patch_radius.near.max));
            let start_x = div_floor(i64::from(min.x) - max_radius, spacing).saturating_sub(1);
            let end_x = div_floor(i64::from(max.x) + max_radius, spacing).saturating_add(1);
            let start_y = div_floor(i64::from(min.y) - max_radius, spacing).saturating_sub(1);
            let end_y = div_floor(i64::from(max.y) + max_radius, spacing).saturating_add(1);

            for cell_y in start_y..=end_y {
                for cell_x in start_x..=end_x {
                    let center = resource_patch_center(self.seed, rule, cell_x, cell_y, spacing);
                    let Some(sample) = self.resource_sample(rule, center) else {
                        continue;
                    };
                    append_patch_tiles(
                        &mut tiles,
                        PatchPlacement {
                            terrain_by_pos,
                            occupied_resources,
                            center,
                            radius: sample.radius,
                            allowed_terrains: &rule.allowed_terrains,
                            item_def_id: &rule.item_def_id,
                            amount: sample.amount,
                            min,
                            max,
                        },
                    );
                }
            }
        }
        tiles
    }

    fn resource_sample(&self, rule: &ResourceRuleDef, pos: TilePos) -> Option<ResourceSample> {
        let distance = distance_from_spawn(pos);
        let factor = DistanceCurve::new(rule.patch_frequency.distance_curve).factor_at(distance);
        let t = distance_curve_t(rule, factor);
        let radius_range = interpolated_range(rule.patch_radius.near, rule.patch_radius.far, t);
        let amount_range = interpolated_range(rule.richness.near, rule.richness.far, t);
        let seed = mix_seed_with_key(self.seed, &rule.resource);

        Some(ResourceSample {
            radius: sample_range(hash_coords(seed, pos.x, pos.y), radius_range),
            amount: sample_range(
                hash_coords(seed.rotate_left(23), pos.x, pos.y),
                amount_range,
            ),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResourceSample {
    radius: u32,
    amount: u32,
}

struct PatchPlacement<'a> {
    terrain_by_pos: &'a BTreeMap<TilePos, String>,
    occupied_resources: &'a mut HashSet<TilePos>,
    center: TilePos,
    radius: u32,
    allowed_terrains: &'a [String],
    item_def_id: &'a str,
    amount: u32,
    min: TilePos,
    max: TilePos,
}

fn append_patch_tiles(tiles: &mut Vec<GeneratedResourceTile>, placement: PatchPlacement<'_>) {
    let radius = i64::from(placement.radius);
    let radius_squared = i128::from(radius) * i128::from(radius);
    let center_x = i64::from(placement.center.x);
    let center_y = i64::from(placement.center.y);

    for y in (center_y - radius).max(i64::from(placement.min.y))
        ..=(center_y + radius).min(i64::from(placement.max.y))
    {
        for x in (center_x - radius).max(i64::from(placement.min.x))
            ..=(center_x + radius).min(i64::from(placement.max.x))
        {
            let dx = x - center_x;
            let dy = y - center_y;
            if i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy) > radius_squared {
                continue;
            }

            let Some(pos) = tile_pos_from_i64(x, y) else {
                continue;
            };
            let Some(terrain) = placement.terrain_by_pos.get(&pos) else {
                continue;
            };
            if !placement
                .allowed_terrains
                .iter()
                .any(|allowed| allowed == terrain)
            {
                continue;
            }
            if !placement.occupied_resources.insert(pos) {
                continue;
            }

            tiles.push(GeneratedResourceTile {
                pos,
                item_def_id: placement.item_def_id.to_string(),
                amount: placement.amount,
            });
        }
    }
}

#[derive(Clone, Debug)]
struct NoiseField {
    seed: u64,
    scale: f64,
}

impl NoiseField {
    fn new(seed: u64, key: &str, scale: f64) -> Self {
        Self {
            seed: mix_seed_with_key(seed, key),
            scale: scale.max(1.0),
        }
    }

    fn sample(&self, x: i32, y: i32) -> f32 {
        let sx = f64::from(x) / self.scale;
        let sy = f64::from(y) / self.scale;
        let x0 = sx.floor() as i64;
        let y0 = sy.floor() as i64;
        let tx = smoothstep(sx - x0 as f64);
        let ty = smoothstep(sy - y0 as f64);
        let a = unit_hash(self.seed, x0, y0);
        let b = unit_hash(self.seed, x0 + 1, y0);
        let c = unit_hash(self.seed, x0, y0 + 1);
        let d = unit_hash(self.seed, x0 + 1, y0 + 1);
        let x_a = lerp(a, b, tx);
        let x_b = lerp(c, d, tx);
        lerp(x_a, x_b, ty) as f32
    }
}

#[derive(Clone, Copy, Debug)]
struct DistanceCurve {
    start: f32,
    end: f32,
    multiplier_at_end: f32,
}

impl DistanceCurve {
    fn new(def: DistanceCurveDef) -> Self {
        Self {
            start: def.start_distance as f32,
            end: def.end_distance as f32,
            multiplier_at_end: def.multiplier_at_end,
        }
    }

    fn factor_at(self, distance: f32) -> f32 {
        if distance <= self.start {
            return 1.0;
        }
        if distance >= self.end {
            return self.multiplier_at_end;
        }
        let span = self.end - self.start;
        if span <= 0.0 {
            return self.multiplier_at_end;
        }
        let t = (distance - self.start) / span;
        1.0 + (self.multiplier_at_end - 1.0) * t
    }
}

pub fn default_profile() -> WorldGenProfile {
    WorldGenProfile {
        id: "default".to_string(),
        starting_area: StartingAreaDef {
            radius: 48,
            terrain: "ground".to_string(),
        },
        terrain_layers: vec![
            TerrainLayerDef {
                terrain: "water".to_string(),
                threshold: 0.78,
                scale: 96.0,
                min_distance_from_spawn: 64,
            },
            TerrainLayerDef {
                terrain: "stone".to_string(),
                threshold: 0.72,
                scale: 64.0,
                min_distance_from_spawn: 56,
            },
        ],
        resources: vec![
            resource_rule(
                "iron_ore_patch",
                "iron_ore",
                25,
                3.0,
                (3, 5),
                (8, 13),
                (4000, 9000),
                (30000, 90000),
            ),
            resource_rule(
                "copper_ore_patch",
                "copper_ore",
                20,
                3.0,
                (3, 5),
                (7, 12),
                (3500, 8000),
                (25000, 80000),
            ),
            resource_rule(
                "coal_patch",
                "coal",
                18,
                2.6,
                (2, 4),
                (6, 11),
                (3000, 7000),
                (22000, 70000),
            ),
        ],
        starting_resources: vec![
            starting_resource("iron_ore_patch", "iron_ore", (8, 20), (4, 6), (8000, 14000)),
            starting_resource(
                "copper_ore_patch",
                "copper_ore",
                (12, 28),
                (4, 6),
                (7000, 13000),
            ),
            starting_resource("coal_patch", "coal", (10, 26), (3, 5), (6000, 12000)),
        ],
    }
}

fn resource_rule(
    resource: &str,
    item_def_id: &str,
    frequency: u32,
    multiplier_at_end: f32,
    near_radius: (u32, u32),
    far_radius: (u32, u32),
    near_richness: (u32, u32),
    far_richness: (u32, u32),
) -> ResourceRuleDef {
    ResourceRuleDef {
        resource: resource.to_string(),
        item_def_id: item_def_id.to_string(),
        allowed_terrains: vec!["ground".to_string()],
        patch_frequency: ResourceFrequencyDef {
            base: frequency,
            distance_curve: DistanceCurveDef {
                start_distance: 64,
                end_distance: 512,
                multiplier_at_end,
            },
        },
        patch_radius: ProfileRangePair {
            near: range(near_radius),
            far: range(far_radius),
        },
        richness: ProfileRangePair {
            near: range(near_richness),
            far: range(far_richness),
        },
    }
}

fn starting_resource(
    resource: &str,
    item_def_id: &str,
    distance: (u32, u32),
    radius: (u32, u32),
    amount: (u32, u32),
) -> StartingResourceDef {
    StartingResourceDef {
        resource: resource.to_string(),
        item_def_id: item_def_id.to_string(),
        patch_count: 1,
        distance_range: range(distance),
        radius_range: range(radius),
        amount_range: range(amount),
        allowed_terrains: vec!["ground".to_string()],
    }
}

fn range((min, max): (u32, u32)) -> IntRange {
    IntRange { min, max }
}

pub fn distance_from_spawn(pos: TilePos) -> f32 {
    let x = f64::from(pos.x);
    let y = f64::from(pos.y);
    (x.mul_add(x, y * y).sqrt()) as f32
}

pub fn is_inside_starting_area(profile: &WorldGenProfile, pos: TilePos) -> bool {
    distance_from_spawn(pos) <= profile.starting_area.radius as f32
}

fn resource_patch_center(
    seed: u64,
    rule: &ResourceRuleDef,
    cell_x: i64,
    cell_y: i64,
    spacing: i64,
) -> TilePos {
    let key_seed = mix_seed_with_key(seed, &rule.resource);
    let cell_seed = hash_coords_i64(key_seed, cell_x, cell_y);
    let offset_x = (cell_seed % spacing as u64) as i64;
    let offset_y = (cell_seed.rotate_left(17) % spacing as u64) as i64;
    let x = cell_x.saturating_mul(spacing).saturating_add(offset_x);
    let y = cell_y.saturating_mul(spacing).saturating_add(offset_y);

    tile_pos_from_i64(x, y).unwrap_or(TilePos::new(0, 0))
}

fn distance_curve_t(rule: &ResourceRuleDef, factor: f32) -> f32 {
    let end = rule.patch_frequency.distance_curve.multiplier_at_end;
    if end <= 1.0 {
        return 0.0;
    }
    ((factor - 1.0) / (end - 1.0)).clamp(0.0, 1.0)
}

fn interpolated_range(near: IntRange, far: IntRange, t: f32) -> IntRange {
    IntRange {
        min: scale_value(near.min, far.min, t),
        max: scale_value(near.max, far.max, t),
    }
}

fn scale_value(near: u32, far: u32, t: f32) -> u32 {
    let value = near as f32 + (far as f32 - near as f32) * t;
    value.round().max(1.0) as u32
}

pub fn mix_seed_with_key(seed: u64, key: &str) -> u64 {
    let mut hash = seed ^ 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    mix_hash(hash)
}

fn hash_coords(seed: u64, x: i32, y: i32) -> u64 {
    hash_coords_i64(seed, i64::from(x), i64::from(y))
}

fn hash_coords_i64(seed: u64, x: i64, y: i64) -> u64 {
    let hash = seed
        ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
    mix_hash(hash)
}

fn mix_hash(mut hash: u64) -> u64 {
    hash ^= hash >> 30;
    hash = hash.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    hash ^= hash >> 27;
    hash = hash.wrapping_mul(0x94d0_49bb_1331_11eb);
    hash ^ (hash >> 31)
}

fn unit_hash(seed: u64, x: i64, y: i64) -> f64 {
    let value = hash_coords_i64(seed, x, y) >> 11;
    value as f64 / ((1_u64 << 53) - 1) as f64
}

fn sample_range(seed: u64, range: IntRange) -> u32 {
    let min = range.min.min(range.max);
    let max = range.min.max(range.max);
    let width = max - min + 1;
    min + (seed % u64::from(width)) as u32
}

fn div_floor(value: i64, divisor: i64) -> i64 {
    value.div_euclid(divisor)
}

fn tile_pos_from_i64(x: i64, y: i64) -> Option<TilePos> {
    Some(TilePos::new(i32::try_from(x).ok()?, i32::try_from(y).ok()?))
}

fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_and_region_are_deterministic() {
        let generator = WorldGenerator::new_default(123);
        let a = generator.generate_square_around_spawn(32);
        let b = generator.generate_square_around_spawn(32);

        assert_eq!(a, b);
    }

    #[test]
    fn starting_region_contains_guaranteed_resources() {
        let generator = WorldGenerator::new_default(123);
        let generated = generator.generate_square_around_spawn(32);
        let mut items = generated
            .resource_tiles
            .iter()
            .map(|resource| resource.item_def_id.as_str())
            .collect::<Vec<_>>();
        items.sort_unstable();
        items.dedup();

        assert!(items.contains(&"iron_ore"));
        assert!(items.contains(&"copper_ore"));
        assert!(items.contains(&"coal"));
    }

    #[test]
    fn resources_are_only_placed_on_allowed_terrain() {
        let generator = WorldGenerator::new_default(123);
        let generated = generator.generate_square_around_spawn(96);
        let terrain_by_pos = generated
            .terrain_tiles
            .iter()
            .map(|tile| (tile.pos, tile.terrain_id.as_str()))
            .collect::<BTreeMap<_, _>>();

        for resource in generated.resource_tiles {
            assert_eq!(terrain_by_pos.get(&resource.pos), Some(&"ground"));
        }
    }

    #[test]
    fn terrain_generation_can_produce_non_ground_outside_start() {
        let generator = WorldGenerator::new_default(123);
        let generated = generator.generate_square_around_spawn(128);

        assert!(
            generated
                .terrain_tiles
                .iter()
                .any(|tile| tile.terrain_id == "water" || tile.terrain_id == "stone")
        );
    }
}
