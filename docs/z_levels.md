# Z-Level Terrain Design

## Model

The world remains tile-based. `TilePos` is still the horizontal `(x, y)` tile
key, while `surface_z` stores the discrete elevation of that tile's buildable
surface.

Flat worlds and old saves use `surface_z = 0`.

Use `TileCoord3` when a rule needs a concrete surface coordinate:

```text
TileCoord3 = x, y, z
```

Chunking remains horizontal. A chunk is still addressed by `(x, y)` and can
contain tiles with different `surface_z` values.

## Initial Invariants

- Missing elevation means `surface_z = 0`.
- Generated terrain starts at `surface_z = 0` until elevation generation is added.
- A multi-tile building footprint must occupy one surface level.
- Ordinary terrain buildability is still checked before elevation rules.
- Save snapshots preserve non-zero `surface_z` entries and omit default-height tiles.
- Placed buildings and building ports store their explicit `surface_z`.

## Transport Rules

These are the intended rules for the next implementation phases.

- A normal belt occupies one `(x, y, z)` surface coordinate.
- A normal belt only connects to adjacent belts on the same `z`.
- Cross-level transport must use an explicit connector.
- The first connector should be a conveyor lift, not a ramp.
- Belt ramps should be separate connector entities with strict input/output sides.

## Implementation Tasks

- [x] Store and round-trip `surface_z`.
- [x] Enforce flat multi-tile building footprints.
- [x] Give placed buildings and building ports an explicit surface `z`.
- [x] Keep ordinary belt topology and inserter inventory interactions on one surface level.
- [x] Render terrain, resource instances, building fallback meshes, and build ghosts at `surface_z`.
- [x] Expose z-aware terrain picking for placement and cursor hover.
- [x] Add a first cross-level connector as a conveyor lift.
- [x] Add save/schema migration coverage for old snapshots at API boundaries.
- [x] Add cliff/edge visuals for height margins; ramp visuals stay tied to future connector rules.
- [x] Add map/minimap height shading and current z-level filtering.
- [x] Add authoring tools or worldgen rules that actually produce non-zero `surface_z`.
