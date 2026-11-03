# Minecraft catalogs and assets

## Version selection

Pin a Minecraft version for reproducible builds. Catalog validation uses that
version, while PNG/GLB appearance uses a shared Java 1.21.6 visual bundle.
Rendering converts a temporary palette to 1.21.6 using the block-state conversion
rules. For available versions, datasets,
and loading methods, see [MinecraftData](api.md#minecraftdata).

## Recipe data

Recipes load on demand through the same catalog provider:

```python
from mcschemora import MinecraftData

data = MinecraftData()
recipes = data.dataset("1.21.1", "recipes")
items = data.dataset("1.21.1", "items")
```

The dataset keeps the upstream JSON format, keyed by numeric output item ID.
Use the same version's `items` dataset to resolve ingredient and output IDs.

Every bundled runtime snapshot includes recipes from its matching official server
JAR. It also provides `recipeDefinitions`, containing every original vanilla recipe
keyed by recipe ID, and `itemTags`, mapping item-tag IDs to resolved numeric item IDs.
Special and component-dependent recipes, including brewing, remain available in
`recipeDefinitions`; the numeric `recipes` index covers fixed-input, fixed-output
crafting, cooking, stonecutting, smithing, and transmutation recipes.

The index enumerates alternatives once per distinct ingredient, so repeated uses
of the same ingredient share a choice. Original definitions and resolved tags
preserve the full ingredient alternatives, including mixed-material combinations.
Crafting recipes appear before cooking/stonecutting recipes; entries are otherwise
ordered by recipe ID. 

## Generating bundled snapshots

`python scripts/generate_snapshots.py` regenerates all bundled snapshots from the
official runtimes, including blocks, items, entity classification, biomes, and
recipes. It shares the recipe importer's download cache and requires JDK 25 for
the currently bundled versions. Version arguments select individual versions;
`--check` compares generated output without writing snapshots. See the
[generation instructions and sources](../../src/catalog/data/SOURCE.md).

## Cache and offline use

Catalogs download on first use. Default native cache locations:

| Platform | Directory |
| --- | --- |
| Linux | `$XDG_CACHE_HOME/mcschemora` when the variable is an absolute path; otherwise `~/.cache/mcschemora` |
| macOS | `~/Library/Caches/mcschemora` |
| Windows | `%LOCALAPPDATA%/mcschemora` |

Choose a writable cache and load the required data before going offline:

```python
from mcschemora import MinecraftData, Schematic, block

data = MinecraftData(cache_dir="./minecraft-cache")
data.load("1.21.1")
data.load_visuals("1.21.1")

offline = MinecraftData(cache_dir=data.cache_dir, offline=True)
schematic = Schematic.create(version="1.21.1", data=offline)
schematic.region().set((0, 0, 0), block("stone"))
schematic.export_png("offline.png")
```

Skip `load_visuals` if only authoring or converting files. Offline cache misses
and invalid cache entries fail explicitly. To load an imported file offline,
preload its version as well.

The prepared visual bundle records its sources and limitations in a manifest;
see the [bundle specification](../../src/catalog/storage/native/SOURCE.md).
Grass blocks and supported foliage use default colors from the bundle's colormaps;
native renders do not evaluate per-position biomes. Unsupported tints emit diagnostics.
Sprite diagrams use bundled wiki sheets and need no visual download.

Browser bindings fetch catalogs through browser `fetch` and keep downloaded bytes
in memory for that `MinecraftData` instance. They do not expose the native
filesystem cache or PNG/GLB rendering. See the [WASM README](../../bindings/wasm/README.md).

## Asset attribution

The root [LICENSE](../../LICENSE) covers project code. Asset sources and terms
remain documented alongside the assets:

- [Runtime catalogs and visual assets](../../src/catalog/storage/native/SOURCE.md).
- [Special block models](../../data/block-models/SOURCE.md) and their [license](../../data/block-models/LICENSE).
- [Entity models](../../data/entity-models/README.md).
- [Wiki sprite sheets](../../data/wiki-sprites/SOURCE.md).

Keep these source records when updating or redistributing assets.
