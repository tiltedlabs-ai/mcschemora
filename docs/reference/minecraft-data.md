# Minecraft catalogs and assets

## Version selection

Pin a Minecraft version for reproducible builds. Catalog validation uses that
version, while PNG/GLB appearance uses a shared Java 1.21.1 visual bundle.
Catalog loading does not upgrade imported files. For available versions, datasets,
and loading methods, see [MinecraftData](api.md#minecraftdata).

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
