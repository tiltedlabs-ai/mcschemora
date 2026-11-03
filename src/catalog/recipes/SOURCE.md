# Official Java recipe tracker

`tracker.json.gz` is the single bundled recipe tracker for every supported Java
version. Shared recipes are stored once by item name, with explicit version lists
for each distinct variant. Definitions and resolved item tags use the same format.
Numeric item IDs are mapped from the requested version's catalog when loading.
Recipe order, ingredient order, output counts, and tag order are preserved.

```sh
python scripts/import_recipes.py
python scripts/import_recipes.py --check
```

The importer refreshes all supported versions by default. Version arguments
refresh only those versions within the tracker; `--check` compares deterministic
output without writing. Full snapshot generation also refreshes its versions in
the same tracker. No Java execution or third-party Python packages are required
for recipe-only imports.

The importer reads Mojang's version manifest, verifies downloaded version metadata
and server JAR SHA1 hashes, and extracts default vanilla recipe and item-tag JSON
from the runtime JAR. It unwraps bundled server JARs when present. Optional
experimental datapacks are excluded. Downloads are cached in the system temporary
directory under `schemora-recipe-sources`; `--cache-dir` selects another directory.
Remove the cache's `manifest.json` to refresh the version list.

Item names are validated against the matching official registry snapshot where
bundled, otherwise the pinned minecraft-data item catalog. Unknown required item
names and tag cycles fail the import. Older snapshots with inaccurate upstream
registries have matching official registry snapshots instead.

The tracker has two sections:

- `provenance`: version to official server URL, server/runtime SHA256 hashes,
  item-map SHA256, recipe-type counts, indexed coverage, and unindexed recipe IDs.
- `datasets`: `recipes`, `recipeDefinitions`, and `itemTags`. Each maps an output
  item name, recipe ID, or tag ID to variants of `{"versions": [...], "value": ...}`.
  A version can appear in at most one variant per entry. Missing entries stay
  absent for that version; there is no latest-version fallback.

The public datasets retain their existing formats. `recipes` maps numeric output
IDs to fixed-input, fixed-output recipes, including crafting, cooking,
stonecutting, smithing, and transmutation. `recipeDefinitions` preserves original
vanilla JSON. `itemTags` maps tag IDs to resolved numeric item IDs.

Ingredient alternatives are enumerated once per distinct ingredient; repeated
occurrences share a choice. Original definitions and tags retain all alternatives,
including mixed-material combinations. Special and component-dependent recipes
remain in the definitions without invented fixed costs. Recipes otherwise sort
by recipe ID, with cooking and stonecutting following other indexed recipes.

Compression omits timestamps and source filenames. Native and WASM builds embed
only this tracker, decompress it once on first use, and cache reconstructed
version datasets in `MinecraftData`. There is no runtime server download.
