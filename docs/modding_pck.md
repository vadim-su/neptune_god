# PCK mod loading

Runtime layout:

```text
neptune_god.exe
neptune_god.pck
mods/
  main.pck
  extra_ores.pck
  logistics_plus.pck
```

`neptune_god.pck` is the bootstrap pack. It must contain the bootstrap scene,
the Godot extension config, and the native extension library required by the
simulation bridge.

`main.pck` is the official game mod. It is loaded before every other pack and
must contain:

```text
res://mods/main/mod.json
```

Example manifest:

```json
{
  "id": "main",
  "name": "Neptune Main",
  "version": "0.1.0",
  "api_version": 1,
  "entry_scene": "res://game/main/main.tscn",
  "catalogs": {
    "items": ["res://assets/catalog/items.json"],
    "buildings": ["res://assets/catalog/buildings.json"],
    "recipes": ["res://assets/catalog/recipes.json"],
    "terrain": ["res://assets/catalog/terrain.json"],
    "resources": ["res://assets/catalog/resources.json"],
    "worldgen": ["res://assets/catalog/worldgen.json"],
    "player": ["res://assets/catalog/player.json"]
  },
  "dependencies": []
}
```

User mods follow the same convention. For `mods/extra_ores.pck`, the manifest
must be available after mounting at:

```text
res://mods/extra_ores/mod.json
```

Packs are mounted with replacement enabled, so later packs can override
resources loaded by earlier packs.

The loader always mounts `main.pck` before user mods. For deterministic
dependency ordering between user mods, add a sidecar manifest next to the PCK:

```text
mods/
  main.pck
  extra_ores.pck
  extra_ores.json
  logistics_plus.pck
  logistics_plus.json
```

The sidecar can be the same JSON object as the internal manifest, but only
`id`, `api_version`, and `dependencies` are needed for load ordering. Internal
manifests are still required because they are the source of truth after the PCK
is mounted.

This sidecar exists because Godot does not expose the contents of a `.pck`
before `ProjectSettings.load_resource_pack()` mounts it. Without a sidecar, the
loader can validate dependencies only after the pack has already been mounted.

Dependency rules:

- `main` is loaded first when present.
- Every user mod implicitly depends on `main` when `main.pck` exists.
- Sidecar dependencies are used for topological sorting before mount.
- Missing dependencies and dependency cycles fail boot.
- `api_version` must be positive and not greater than the bootstrap-supported
  API version.

## Catalog merge

After all packs are mounted, the bootstrap builds a merged Godot-side catalog
from every loaded mod manifest. The current runtime wires these catalog kinds
into the game before the main scene starts:

```text
items
buildings
recipes
terrain
resources
worldgen
player
```

Catalog paths are read from the manifest `catalogs` object in mod load order:

```json
{
  "catalogs": {
    "items": ["res://mods/extra_ores/catalog/items.json"],
    "recipes": ["res://mods/extra_ores/catalog/recipes.json"]
  }
}
```

Each catalog file is a JSON object with an array named after the catalog kind:

```json
{
  "items": [
    {
      "id": "tin_ore",
      "display_name": "Tin ore"
    }
  ]
}
```

Rows are merged by `id`. Later mods override earlier mods field by field, with
recursive merging for nested dictionaries. Arrays merge by stable element keys
when available: dictionary elements with `id` or `resource` update matching
entries from earlier mods, and new elements are appended. Scalar array values
are appended without duplicates. Rows without an `id` are preserved but cannot
override anything.

`items`, `buildings`, and `recipes` are loaded into Godot UI catalog scripts.
The merged rows are also passed to the Rust `NeptuneSim` bridge. Rust then
builds the active `CoreCatalog` and `WorldGenProfile` from mod data instead of
using hardcoded definitions.

Building rows keep UI fields at the top level and put simulation fields under
`sim`:

```json
{
  "id": "basic_belt",
  "display_name": "Belt",
  "ui_type": "Transport",
  "walkable": true,
  "sim": {
    "kind": "Transport",
    "footprint": {"rectangle": [1, 1]},
    "rotate_footprint": true,
    "outputs": [
      {"role": "BeltLane", "side": "OutputDirection", "offsets": [0]}
    ],
    "behavior": {"driver": "transport", "speed_units_per_tick": 4},
    "power": {"type": "none"}
  }
}
```

Supported simulation fields:

- `kind`: `Machine`, `Transport`, `Passive`, or `Inserter`.
- `footprint`: either `{"rectangle": [width, height]}` or explicit tile pairs
  like `[[0, 0], [0, 1]]`.
- `rotate_footprint`: whether footprint tiles rotate with placement direction.
- `inputs` / `outputs`: port rows with `role`, `side`, optional `offsets`, and
  optional item-id `accepts`.
- `inventories`: inventory rows with `role`, `slots`, `max_stack`, optional
  item filters, tags, weight, bulk, and item size limits.
- `behavior.driver`: `noop`, `transport`, `underground`, `splitter`,
  `inserter`, or `behavior_host`.
- `behavior_host` rows can define `role`, `recipes`, and `work_area`. If
  `recipes` is omitted, Rust derives the recipe list from recipe rows whose
  `machines` array contains the building id.
- `power` accepts `{"type": "none"}` for the current runtime schema.

`mods/main.pck` is required. The bootstrap does not start the game scene without
the main mod mounted and the catalog registry populated.

## Sample Mod

`mods/extra_ores` is a small user content mod used to validate the pack flow. It
adds:

- `tin_ore` and `tin_plate` item definitions.
- `tin_plate` recipe.
- `tin_furnace` building with its own simulation inventory and behavior config.
- `tin_ore_patch` resource and default worldgen profile overrides that add tin
  ore to regular and starting-area resource generation.

The sidecar `mods/extra_ores.json` declares the dependency on `main` so the
bootstrap can sort the PCK before mounting it.

## Development build

`mods/main.pck` is required even in development. The `Rust Build` editor plugin
builds dev mod packs before Godot starts the project, so the normal editor Play
button can boot through the same mod loader as the exported layout.

The same workflow is available from the command line:

```sh
tools/dev_build_mods.sh
```

This command builds the Rust GDExtension, exports the PCK layout to
`build/dev_dist`, and copies generated mod packs back into `mods/*.pck` for
editor runs.

To build and run the packed bootstrap directly:

```sh
tools/dev_run_pck.sh
```

The editor plugin also exposes two Project menu actions:

- `Neptune: Build Dev Mod Packs`
- `Neptune: Build & Run PCK Layout`

## Building the current layout

Use:

```sh
tools/build_pck_layout.sh
```

The script creates temporary staging projects and exports:

```text
build/pck_dist/
  neptune_god.pck
  target/
    release/
      libneptune_godot.so
  mods/
    main.pck
    extra_ores.pck
    extra_ores.json
```

The native GDExtension library is copied as a loose file because Godot loads
platform dynamic libraries from the filesystem, not from a mounted mod pack.

Every directory matching `mods/<id>/mod.json` is exported as
`build/pck_dist/mods/<id>.pck`, except `mods/main`, which is exported as the
official `main.pck`. Optional sidecars named `mods/<id>.json` or
`mods/<id>.mod.json` are copied next to the generated PCK.

The script also copies `.godot/imported` and `.godot/uid_cache.bin` into the
temporary staging projects before export. Make sure the project has been opened
or imported by Godot on the build machine first; otherwise the exported PCKs can
miss generated `.ctex` and `.scn` resources for textures and Blender models.

You can pass a different output directory:

```sh
tools/build_pck_layout.sh /tmp/neptune_dist
```

Set `GODOT_BIN` to use a specific Godot binary:

```sh
GODOT_BIN=godot4 tools/build_pck_layout.sh
```
