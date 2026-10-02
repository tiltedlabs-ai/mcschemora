# MCSchemora for Python

Python bindings for MCSchemora. Requires Python 3.10 or newer.

## Install from source

With [uv](https://docs.astral.sh/uv/) and a stable [Rust toolchain](https://rustup.rs/),
run from the repository root:

```sh
uv sync --all-packages
uv run --all-packages python examples/build.py
```

To build and install a platform-specific wheel, see [development](../../docs/development.md#build-a-wheel).

## Usage

```python
from mcschemora import Schematic, block

scene = Schematic.create(version="1.21.1")
floor = scene.region().select(start=(0, 0, 0), size=(7, 1, 7))
floor.fill(block("stone_bricks"))
print(scene.validate())
scene.save("floor.schem")
```

## Documentation

- [Getting started](../../docs/guides/getting-started.md).
- [Runnable examples and outputs](../../examples/README.md).
- [Python API](../../docs/reference/python-api.md).
- [Formats and conversion](../../docs/reference/formats.md).
- [Catalogs, offline use, and asset attribution](../../docs/reference/minecraft-data.md).
- [Development and rebuilds](../../docs/development.md).
