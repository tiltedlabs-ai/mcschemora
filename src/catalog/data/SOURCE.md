# Official Java catalog snapshots

These datasets come from each exact version's official Java server runtime.

## Regeneration

```sh
python scripts/generate_snapshots.py
python scripts/generate_snapshots.py --check
```

The generator rebuilds every bundled registry snapshot and its entries in the shared recipe tracker. Pass version
arguments to select snapshots or generate a new version. New versions also need
an entry in `src/catalog/snapshots.rs` to ship in the library.

To update official recipes for every supported version:

```sh
python scripts/import_recipes.py
python scripts/import_recipes.py --check
```

Full generation requires a JDK capable of running the requested Minecraft
versions; JDK 25 runs all currently bundled versions. `--java` and `--javac`
select executables. The small `scripts/SnapshotRuntime.java` helper reads entity
class metadata after the official data generator runs. Reports and JVM output
are saved under the cache's `reports/<version>` directory. Recipe-only import
reads JAR resources without executing Java.

## Registry data

Blocks, items, and entity IDs come from the official data generator's `--reports`
output. Every block state is checked against its reported global state ID using
sorted property names and the runtime's ordered property values. Default states
are explicitly marked by the runtime.

Living-entity classification uses each static `EntityType` entry's actual generic
entity class and `LivingEntity.isAssignableFrom`. Official mappings resolve
reflection names where available; unobfuscated runtimes use their original class
names. Java 1.14.2 predates official server mappings, so its known runtime names
are `fm` for the registry, `aim` for entity types, and `air` for living entities.

Biome names come from vanilla biome resources where available. Java 1.19.1 uses
the server data generator's worldgen biome output; Java 1.14.2 uses its biome
registry report.

## Recipe data

Recipes are stored separately from the registry snapshots in
[`src/catalog/recipes`](../recipes/SOURCE.md). All supported versions use official
server-JAR recipe data, including versions whose block registries come from
minecraft-data. Full snapshot generation refreshes the matching tracker entries
using the newly generated item IDs to validate recipe names.
