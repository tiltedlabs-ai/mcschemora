# MCSchemora

![Banner built and rendered with MCSchemora](docs/assets/banner.png)

Author, edit, convert, and render Minecraft schematics in Rust with Python, and WASM bindings.
Supports `.schem`, `.litematic`, `.nbt`, `.snbt`, `.schematic`, `.mcstructure`, and wiki blueprints.

[Reproduce the banner build](examples/README.md#build-the-banner).

## Capabilities

- Build with version-checked block states, bulk edits, and named regions.
- Filter selections; copy, move, rotate, and mirror builds.
- Check Java game rules and inspect conversion losses before exporting.
- Render textured PNGs, GLB models, sprite diagrams, and layered wiki blueprints.

## Get started

### Python

With Python 3.10+, [uv](https://docs.astral.sh/uv/), and a stable
[Rust toolchain](https://rustup.rs/), run from this checkout:

```sh
uv sync --all-packages
uv run --all-packages python examples/build.py
uv run --all-packages python examples/render.py
```

Example schematic output, rendered with the roof hidden to show the interior:

![Workshop example output](docs/assets/workshop.png)

Or create a schematic directly:

```python
from mcschemora import Schematic, block

scene = Schematic.create(version="1.21.1")
floor = scene.region().select(start=(0, 0, 0), size=(7, 1, 7))
floor.fill(block("stone_bricks"))
print(scene.validate())
scene.save("floor.schem")
```

### Rust

With a stable Rust toolchain, run from this checkout:

```sh
cargo run --example build
```

## Documentation

- [Start here](docs/index.md) — setup, workflows, and reference.
- [Getting started: Python](docs/guides/getting-started.md) · [Rust](docs/guides/getting-started-rust.md).
- [Runnable examples and outputs](examples/README.md) — build, edit, convert, render.
- [Python API](docs/reference/python-api.md) · [WASM API](docs/reference/wasm-api.md) · [File formats](docs/reference/formats.md).
- [Python binding](bindings/python/README.md) · [Browser WASM binding](bindings/wasm/README.md).
- [Development](docs/development.md).

## License

Project code is [MIT licensed](LICENSE). Bundled and downloaded assets have
[their own attribution and terms](docs/reference/minecraft-data.md#asset-attribution).
