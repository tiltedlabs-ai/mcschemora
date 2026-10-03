# Java version conversion

Conversion module to switch between versions from 1.13 to 26.3

## Organization

| Module | Responsibility |
| --- | --- |
| `mod.rs` | Route selection, intermediate versions, source/target context and diagnostics |
| `blocks.rs`, `spatial.rs` | Block-state validation, changed properties and retained spatial data |
| `items.rs`, `legacy_items.rs` | Item-stack schemas and nested item traversal |
| `components.rs`, `component_changes.rs` | Component patches and changes to component structures |
| `entities.rs`, `entity_changes.rs`, `block_entities.rs` | Typed owner fields and version-dependent behavior |
| `text.rs`, `profiles.rs`, `attributes.rs`, `tooltips.rs`, `item_variants.rs`, `pottery.rs` | Shared domain transformations used by multiple owners |
| Other domain modules | Effects, particles, equipment, maps, signs, UUIDs and specialized owners |
| `data/` | Factual identifiers, version boundaries, defaults and registry snapshots |

## Implemented behavior

Conversion covers forward and representable inverse item components, nested
inventories, entities and block entities, rich text and hover payloads, profiles,
UUIDs, attributes, effects, particles, equipment, signs, food and consumption,
registry renames, block-state changes, item block-state overrides, adventure
state predicates and Sponge biome palettes. Recipe references share one walker
across knowledge books, furnaces and player recipe books. Specialized
rules handle trial-spawner configurations, boats, cloud effects, locks, maps,
furnaces, brewing, variant components and Litematic scheduled ticks.

Later schemas include structured text, tooltip consolidation, per-attribute
display, expanded registry references, named pot-decoration stacks, split item
animations, sign components and changed block-state NBT keys. Available older
representations are used; independent new behaviors without an older equivalent
produce specific errors.

Explicit gameplay values are preserved when their identifiers and units stay
the same. Vanilla balancing changes, such as wolf maximum-health increases,
are not copied into schematic conversion.

