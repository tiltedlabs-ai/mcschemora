# Minecraft Wiki sprite sheets

All sprites sourced directly from the [wiki](https://minecraft.wiki/w/Module:SpriteFile)

- `blocks.png`: all imported `BlockSprite` files and their file redirects.
- `entities.png`: all imported `EntitySprite` files and their file redirects.
- `schematic.png`: the wiki's schematic sheet with its original layout.
- `sprites.json`: wiki identifiers, pixel rectangles, and source metadata.
- 

## Rebuild

From the repository root:

```sh
uv run --with pillow python scripts/import_wiki_sprites.py
```

## Source and attribution

Assets come from [Minecraft Wiki](https://minecraft.wiki/). Their rights remain
with their respective authors and rights holders; the repository's software
license does not relicense these images. Per-file description wikitext and URLs
are retained in `files` for the original credits and license declarations.

Conventions are defined by [SpriteFile](https://minecraft.wiki/w/Module:SpriteFile),
[SpriteGrid](https://minecraft.wiki/w/Module:SpriteGrid), and
[Schematic/data](https://minecraft.wiki/w/Module:Schematic/data). More details are in
[the import research](../../docs/minecraft-wiki-sprites.md).
