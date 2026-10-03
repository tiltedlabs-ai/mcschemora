# MCSchemora for Python

Rust-based programmatic tools for editing, converting, and rendering Minecraft schematics.

## Install

```sh
python -m pip install mcschemora
```

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
uv run --all-packages maturin build --release --locked --manifest-path bindings/python/Cargo.toml --out dist
```

Install the generated wheel from `dist/` with `python -m pip install <wheel-path>`
in the destination environment.

## Usage

```python
from mcschemora import Schematic, block

schematic = Schematic.create(version="1.21.1")
floor = schematic.region().select(start=(0, 0, 0), size=(7, 1, 7))
floor.fill(block("stone_bricks"))
print(schematic.validate())
schematic.save("floor.schem")
```

## Documentation

- [Usage guide](https://github.com/whuang37/schemora/blob/master/docs/guides/usage.md) · [Examples and outputs](https://github.com/whuang37/schemora/blob/master/examples/README.md).
- [API reference](https://github.com/whuang37/schemora/blob/master/docs/reference/api.md#python) · [Catalogs and offline preparation](https://github.com/whuang37/schemora/blob/master/docs/reference/minecraft-data.md).
- [Release workflow](https://github.com/whuang37/schemora/blob/master/.github/workflows/python-release.yml).

## Licensing

Project code is MIT licensed. Bundled geometry and sprites have separate terms;
each installed package includes notices and attribution under `mcschemora/licenses/`.
MCSchemora is an independent project and is not affiliated with Mojang or Microsoft.
