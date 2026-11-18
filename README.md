# MCSchemora

Create, edit, convert, and render Minecraft schematics with Python, Rust, or
browser WASM.

![Floating islands built and rendered with MCSchemora](https://raw.githubusercontent.com/tiltedlabs-ai/mcschemora/main/docs/assets/examples/banner.png)

[Documentation](https://tiltedlabs-ai.github.io/mcschemora/) ·
[Developer quickstart](https://tiltedlabs-ai.github.io/mcschemora/quickstart/developers/) ·
[Agent quickstart](https://tiltedlabs-ai.github.io/mcschemora/quickstart/agents/) ·
[Full Markdown docs](https://tiltedlabs-ai.github.io/mcschemora/llms.txt) ·
[Examples](https://tiltedlabs-ai.github.io/mcschemora/examples/)

## Install

To get started, make sure [uv](https://docs.astral.sh/uv/getting-started/installation/) is installed, run
the following commands:

```sh
uv init --python 3.12
uv add mcschemora
```

If you are working with an existing uv project, use `uv add mcschemora`. Run scripts with `uv run build.py`.
MCSchemora supports Python 3.10 or later. Catalogs download on first use;
textured rendering also needs visual assets. See [versions and offline use](https://tiltedlabs-ai.github.io/mcschemora/sdk/minecraft-data/).

## Build a schematic

```python
from mcschemora import Schematic, block

scene = Schematic.create(version="1.21.1")
scene.region().select(start=(0, 0, 0), size=(7, 1, 7)).fill(block("stone_bricks"))
print(scene.validate())
scene.save("floor.schem")
```

The [developer quickstart](https://tiltedlabs-ai.github.io/mcschemora/quickstart/developers/)
builds a workshop and renders a preview. The
[agent quickstart](https://tiltedlabs-ai.github.io/mcschemora/quickstart/agents/)
provides a build request and instructions for checking and delivering the result.

## What it does

- Author blocks with version-checked states, bulk placement, and named regions.
- Select, copy, rotate, mirror, and restyle existing structures.
- Validate supported Java game rules and inspect blocks, inventories, and entities.
- Estimate construction materials needed and compare their fixed material values.
- Read and write `.schem`, `.litematic`, Java structure NBT/SNBT, `.mcstructure`, and wiki blueprints.
- Export textured PNGs, GLB models, sprite layers, and wiki markup in Python and Rust.

Support depends on edition and format. Browser WASM exposes authoring,
conversion, validation, and material estimates; see
[formats and support](https://tiltedlabs-ai.github.io/mcschemora/reference/formats/)
for binding and edition limits.

Read the SDK chapters on
[palettes and profiles](https://tiltedlabs-ai.github.io/mcschemora/sdk/palettes/) and
[materials and costs](https://tiltedlabs-ai.github.io/mcschemora/sdk/materials-and-costs/),
or run concrete examples for
[conversion](https://tiltedlabs-ai.github.io/mcschemora/examples/litematic-to-schem/),
[cost comparison](https://tiltedlabs-ai.github.io/mcschemora/examples/compare-palettes/), and
[layer diagrams](https://tiltedlabs-ai.github.io/mcschemora/examples/layer-diagrams/).

For development, use the [contributor guide](https://tiltedlabs-ai.github.io/mcschemora/contributing/).

## License

Project code is [MIT licensed](https://github.com/tiltedlabs-ai/mcschemora/blob/main/LICENSE).
Data and assets have [their own sources and terms](https://tiltedlabs-ai.github.io/mcschemora/reference/sources/).
MCSchemora is independent of Mojang, Microsoft, and PrismarineJS.
