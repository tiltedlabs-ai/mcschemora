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

## Updating documentation

Public docstrings own API contracts. Python uses Google-style docstrings with types
and defaults in signatures; Rust uses documentation comments. Keep authored pages
focused on setup and workflows, with complete scripts and outputs in `examples/`.

After an API or docstring change:

```sh
make docs
make docs-check
```

Both commands rebuild the Python and WASM bindings. Use the Python version in
`.python-version` and the [browser build tools](../bindings/wasm/README.md#build).
Review and commit `docs/reference/api.md` with the code and affected workflows.
Edit source docstrings rather than the generated page.

The [Documentation workflow](../.github/workflows/docs.yml) runs `make docs-check`
on pull requests, pushes to `master`, and manual runs. Missing or stale references
fail the check; regenerate and commit them locally.

After the first run, make **Documentation freshness** a required status check to
enforce this before merging.
