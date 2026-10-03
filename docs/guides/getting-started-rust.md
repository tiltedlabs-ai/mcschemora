# Getting started with Rust

Use a stable Rust toolchain. The Rust example runs without Python.

## Run from this checkout

From the repository root:

```sh
cargo run --example build
```

The [complete example](../../examples/build.rs) loads a catalog, builds a floor,
validates it, and writes a schematic. See the [example gallery](../../examples/README.md#build-with-rust) for output.

Catalogs download on first use. The cache must be writable; see
[catalogs and offline setup](../reference/minecraft-data.md#cache-and-offline-use).

## Use the crate in an application

Add these dependencies to your application's `Cargo.toml`, replacing the path
with the location of this checkout:

```toml
[dependencies]
mcschemora = { path = "/path/to/mcschemora" }
pollster = "0.4"
```

Copy [examples/build.rs](../../examples/build.rs) to your application's `src/main.rs`
and run `cargo run`.
→ encoding flow, including error handling and filesystem I/O.

## Render a PNG

Inside the example's async block, after creating the schematic:

```rust
let assets = data.geometry_assets(&schematic.version)?;
let prepared = assets.prepare(&schematic, &mcschemora::render::SceneOptions::default())?;
let png = mcschemora::render::png::encode(&prepared, &mcschemora::render::png::Options::default())?;
fs::write(output.join("floor.png"), png)?;
println!("Visual diagnostics: {:?}", prepared.diagnostics);
```

Geometry preparation downloads visual assets on first use. Inspect its diagnostics
for approximations. See [catalogs and assets](../reference/minecraft-data.md) for offline
preparation and the [Rust reference](../reference/api.md#rust) for API documentation.
