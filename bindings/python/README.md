# Schemora Python bindings

From the repository root:

```sh
git submodule update --init --depth 1
uv sync --all-packages
uv run --all-packages python examples/build.py
```

For an installed wheel, supply a minecraft-data checkout with `SCHEMORA_DATA`
or `MinecraftData(path)`. Data is loaded at runtime, not bundled in the wheel.
Java authoring starts at 1.13. See the repository README for formats and examples.
