# Official Java catalog snapshots

These datasets come from each exact version's official Java server runtime.

## Regeneration

```sh
python scripts/generate_snapshots.py
python scripts/generate_snapshots.py --check
```

The generator rebuilds every bundled snapshot, including recipes. Pass version
arguments to select snapshots or generate a new version. New versions also need
an entry in `src/catalog/snapshots.rs` to ship in the library.

To update only recipes in existing snapshots:

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

The importer reads vanilla recipe and item-tag resources from the runtime JAR,
verifying Mojang's SHA1 and the snapshot's recorded SHA256.

`recipeDefinitions` preserves every vanilla recipe JSON by namespaced recipe ID.
`itemTags` resolves nested tags to this snapshot's numeric item IDs. `recipes`
indexes fixed-input, fixed-output recipes by numeric output item ID using the
upstream `inShape`/`ingredients` and `result` representation. Ingredient
alternatives are enumerated once per distinct ingredient; repeated occurrences
share a choice. Full alternatives remain in the definitions and resolved tags.
Cooking and stonecutting entries follow other indexed recipes, with recipe IDs
determining the order within each group.

Special and component-dependent recipes remain in `recipeDefinitions` without
inventing fixed ingredient costs. Provenance records recipe-type counts, indexed
coverage, and the recipe IDs outside the numeric index.
