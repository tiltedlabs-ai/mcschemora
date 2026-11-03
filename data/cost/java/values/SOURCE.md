# Material valuation profiles

`emc.json` and `essentialsx.json` are fixed unit-value maps keyed by canonical
Minecraft IDs. They are shared across game versions and applied only after
`material_cost()` expands a build into primitives. Missing prices remain
unpriced. Zero is a valid price. 

## EMC

Source: [ProjectE mc1.21.1](https://github.com/sinkillerj/ProjectE/tree/f432b0c66837759fb0731c9144dc53176b949c5d),
commit `f432b0c66837759fb0731c9144dc53176b949c5d`.

- [Default values and conversions](https://github.com/sinkillerj/ProjectE/blob/f432b0c66837759fb0731c9144dc53176b949c5d/src/datagen/generated/data/projecte/pe_custom_conversions/defaults.json),
  SHA256 `5d9b3decb598c00e9bf3e2e06ec9aeb7d7c02843cc2f3ed5c7f743c0ee31f2b5`.
- [Metal values and conversions](https://github.com/sinkillerj/ProjectE/blob/f432b0c66837759fb0731c9144dc53176b949c5d/src/datagen/generated/data/projecte/pe_custom_conversions/metals.json),
  SHA256 `c94de2d5d68bdebb752047e25d24474f65c602abc19809b9525ff2679731332d`.

The profile adapts these values for vanilla Minecraft. Generation starts with
ProjectE's fixed `values.before` entries, expands vanilla item tags using the
bundled Java 1.21.1 catalog, and applies its declared conversions and that
version's vanilla recipes. Known inputs are summed and divided by the output
count; the lowest resolved value is retained. Fixed seed values are preserved,
and `values.conversion` outputs use their declared conversion rather than vanilla
recipes. Calculations use exact fractions, then truncate positive item values
to integers. Unresolved values and component-specific entries are omitted.

The official 1.21.1 runtime SHA256 is
`c301de10f575027d13eac18c7f34409d60648cf56a35d566aa1f530ff617840a`;
its recipes and resolved item tags come from the bundled recipe tracker.

Vanilla members of common tags are mapped explicitly:

| Common tag | Vanilla item |
| --- | --- |
| `c:seeds/wheat` | `wheat_seeds` |
| `c:seeds/beetroot` | `beetroot_seeds` |
| `c:crops/wheat` | `wheat` |
| `c:crops/nether_wart` | `nether_wart` |
| `c:pumpkins/normal` | `pumpkin` |
| `c:crops/carrot` | `carrot` |
| `c:crops/beetroot` | `beetroot` |
| `c:crops/potato` | `potato` |
| `c:rods/blaze` | `blaze_rod` |
| `c:rods/breeze` | `breeze_rod` |
| `c:gems/quartz` | `quartz` |
| `c:gems/emerald` | `emerald` |
| `c:nether_stars` | `nether_star` |
| `c:rods/wooden` | `stick` |
| `c:gems/diamond` | `diamond` |
| `c:dusts/redstone` | `redstone` |
| `c:dusts/glowstone` | `glowstone_dust` |
| `c:ingots/iron` | `iron_ingot` |
| `c:gems/amethyst` | `amethyst_shard` |
| `c:ingots/gold` | `gold_ingot` |
| `c:ingots/copper` | `copper_ingot` |

Water source blocks have value 0 from ProjectE's free-water rule. Lava source
blocks have value 64, using 1,000 fluid units per source block and the declared
obsidian-to-lava conversion. Filled buckets retain their container cost.

## EssentialsX

Source: [EssentialsX 2.x worth.yml](https://github.com/EssentialsX/Essentials/blob/e70bdb817c758474acb3a23a37cf961b2f716a50/Essentials/src/main/resources/worth.yml),
commit `e70bdb817c758474acb3a23a37cf961b2f716a50`.
SHA256 `458da19b300a8cd2cce4537faf900179c8bab73daa2a5a248fa64619b641ab48`.

These are the official default server sell prices. Names from the
Java 1.21.1 item/block catalog and the primitive policy are matched to scalar
`worth` keys after removing the namespace and underscores, matching EssentialsX's
[price lookup](https://github.com/EssentialsX/Essentials/blob/e70bdb817c758474acb3a23a37cf961b2f716a50/Essentials/src/main/java/com/earth2me/essentials/Worth.java).