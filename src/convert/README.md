# Java version conversion
## Current coverage

Supported release data versions are 1.20.2, 1.20.3, 1.20.4, 1.20.5, and 1.20.6.
Routes involving 1.20.2 require documents without entities or block entities.
The main schema migration is 1.20.3/1.20.4 to 1.20.5/1.20.6: legacy item tags become
components, including nested inventories, equipment, and associated entity changes.
Unconsumed item tags become `minecraft:custom_data`.

Component-to-tag downgrades and other release routes are not implemented. Known
unsupported payloads include arrows, parameterized particles, map decorations,
complex adventure predicates, embedded commands or item hover events, and hiding
default attributes without explicit modifiers. Retained spatial data other than
scheduled ticks is rejected. These are blocking errors even with `allow_loss`.
This is an initial converter, not complete coverage of Minecraft 1.13 onward.

## Schema references

The mappings and transformations are authored here from Minecraft schema changes;
no external converter is bundled.

- [Java 1.20.5 release notes](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-20-5)
- [24w09a: item components](https://www.minecraft.net/en-us/article/minecraft-snapshot-24w09a)
- [24w05a: entity data changes](https://www.minecraft.net/en-us/article/minecraft-snapshot-24w05a)
