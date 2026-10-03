# Official Java registry snapshots

These datasets were extracted from the corresponding official Java server runtime. Each JSON records its version, data version, runtime SHA256 and extraction method.

Blocks, items and entity IDs come from the official data generator's `--reports` output. For every block, every reported state was decoded using the sorted property names and ordered property values and checked against the reported global state ID. Defaults are the explicitly marked runtime default states. Entity living classification comes from the actual generic entity class of each static `EntityType` entry and `LivingEntity.isAssignableFrom`.

Biome IDs come from vanilla registry lookup;1.19.1 uses the generated vanilla worldgen biome files and1.14.2 uses its generated biome registry report. Official mappings resolve reflection names where available. Java1.14.2 predates official server mappings, so its runtime classes were identified from the official data-generator registry writer and class inheritance (`fm` registry, `aim` entity type, `air` living entity, `gi` data-generator bootstrap).