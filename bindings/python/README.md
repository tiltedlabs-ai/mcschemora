# MCSchemora for Python

Python bindings for MCSchemora. Requires Python 3.10 or newer.

## Install from source

With [uv](https://docs.astral.sh/uv/) and a stable [Rust toolchain](https://rustup.rs/),
run from the repository root:

```sh
uv sync --all-packages
uv run --all-packages python examples/build.py
```

After changing Rust code, run `make rebuild` to refresh the native extension.

## Build a wheel

From the repository root:

```sh
uv run --all-packages maturin build --release --manifest-path bindings/python/Cargo.toml --out dist
```

Install the generated wheel from `dist/` with `python -m pip install <wheel-path>`
in the destination environment.

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

- [Usage guide](../../docs/guides/usage.md) · [Examples and outputs](../../examples/README.md).
- [API reference](../../docs/reference/api.md#python) · [Catalogs and offline preparation](../../docs/reference/minecraft-data.md).
- [Documentation maintenance](../../docs/index.md#updating-documentation).
