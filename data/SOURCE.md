# Minecraft data

Schemora reads [PrismarineJS/minecraft-data](https://github.com/PrismarineJS/minecraft-data)
from the `data/minecraft-data` Git submodule or a user-supplied checkout. There are
no manually copied version catalogs and no compile-time version allowlist.

## Setup and updates

From the repository root:

```sh
git submodule update --init --depth 1
```

To fetch the current upstream revision:

```sh
make data-update
```

Commit the updated submodule reference when you want other checkouts to use it.
The Git reference identifies a data snapshot containing many Minecraft versions.
Python users can override it with `MinecraftData(path)` or `SCHEMORA_DATA`.
Use a new `MinecraftData` object after updating the checkout to refresh caches.

## Inputs

The loader reads `data/dataPaths.json`, then resolves the requested Java version's
`blocks.json`, `items.json`, and `entities.json`. It also reads
`data/pc/common/protocolVersions.json` for the game data version and
`data/pc/common/legacy.json` for classic numeric block mappings.

Java 1.13 is the minimum. An unavailable catalog is an error, with no fallback to
another Minecraft version. Installed wheels and the Rust crate do not embed the
data repository; the application supplies a checkout at runtime.

These catalogs describe registries. They do not supply a complete codec for
saved entities, inventory NBT, or every block entity. The helper code handles the
known schema changes it uses. Relevant upstream release notes:

- [Java 1.20.5 item components](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-20-5)
- [Java 1.21.5 NBT text components](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-21-5)

## License

The upstream [README](minecraft-data/README.md#license) declares MIT and records
its extraction sources. Keep the submodule's attribution with distributed data.
