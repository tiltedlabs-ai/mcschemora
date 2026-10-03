# MCSchemora documentation

MCSchemora authors Minecraft schematics through Rust, Python, and browser WASM interfaces.
Choose a language quickstart, then use the guide or runnable examples for a workflow.

| Goal | Read |
| --- | --- |
| Get started with Python | [Python binding](../bindings/python/README.md) |
| Build, load, and render from Rust | [Getting started with Rust](guides/getting-started-rust.md) |
| Use MCSchemora in the browser | [WASM binding](../bindings/wasm/README.md) |
| Edit, convert, render, or make a blueprint | [Usage guide](guides/usage.md) |
| Find an API contract | [Generated reference](reference/api.md) |
| Select a Minecraft version or work offline | [Catalogs and assets](reference/minecraft-data.md) |
| Run complete workflows and see their results | [Examples](../examples/README.md) |
| Build and publish Python distributions | [Release workflow](../.github/workflows/python-release.yml) |

## Updating documentation

After an API or docstring change:

```sh
make docs
make docs-check
```