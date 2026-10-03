# Java version conversion

`check_export`, `save`, and `to_bytes` share export preparation. Selecting a
Minecraft version runs the required ordered conversion steps on an isolated
export copy. The source schematic retains its original version and payloads.
The public API does not need a separate conversion operation.

All 51 stable endpoints from 1.13 through 26.3 have matching catalogs and admit
implemented payload subsets. This is not complete release-schema certification.
Errors identify the affected owner and field. `allow_loss` accepts reported
losses; it never overrides an unsupported conversion.

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

Format codecs in `formats/` read and write their containers. They call shared
export preparation and do not duplicate gameplay-data conversion. Missing
catalog releases use official runtime snapshots through the existing catalog
loader. Unsupported auxiliary catalog datasets fail explicitly.

Rendering resolves palette names against the visual registry without converting
or copying the complete schematic and its payloads.

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

## Verification and limits

`scripts/verify_conversion.py` checks all 2,601 ordered pairs of the 51 releases
through NBT, SNBT, Sponge and Litematic: 10,404 format routes and their inverses.
The route specimen checks item counts, exact opaque NBT types, target data
versions and source preservation. The detailed corpus includes 11 positive item
fixtures, 40 historical, five attribute, 87 modern and eight particle reference
cases, all 46 named potion IDs, plus variant, text, owner, loss, malformed-data
and failed-export cases.

`scripts/verify_conversion_rules.py` enumerates every rename-table entry and its
listed owner kinds in both directions at the nearest release boundaries. It also
checks legacy recipe roundtrips and all 16 input/output codec combinations in
both directions. Its block-property matrix scans all 51 catalogs and checks all
39 adjacent schema changes across the six declared rule families, including
every nondefault inverse value and experimental-content guards. Fixtures record
official runtime provenance and intentional differences from vanilla's data fixer.

Run both scripts with `--cache-dir <prepared-cache>` after rebuilding the Python
extension. Both use offline catalogs. Build checks are workspace Clippy with
warnings denied and the WebAssembly library check.

These checks do not establish complete target-game loading coverage. Remaining
unsupported cases include command grammar branches, nonempty exact-NBT predicates,
legacy two-dimensional biome layouts, several default-dependent item inverses,
and conversions requiring external registries, resource packs, world time or
spawning context. New behavior without an older equivalent is also rejected.
See the [audit ledger](../../docs/research/java-conversion-audit.md) and
[conversion inventory](../../docs/research/java-conversion-inventory.md).

The transformations are independently authored. No external converter is
bundled; runtime-derived tables contain factual schema and registry data.
