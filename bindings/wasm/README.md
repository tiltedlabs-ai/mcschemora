# MCSchemora for the browser

WASM bindings for MCSchemora.

## Build

With stable Rust and npm, run from the repository root:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
npm --prefix bindings/wasm run build
```

## Usage

```javascript
import init, { MinecraftData } from "./pkg/mcschemora.js";

await init();
const data = new MinecraftData();
const scene = await data.create("1.21.1");
try {
  scene.setBlock("main", 0, 0, 0, "minecraft:stone_bricks");
  console.log(scene.getBlock("main", 0, 0, 0));
  console.log(scene.validate().errors);
  const bytes = await scene.toBytes("schem", false, false);
  const file = new Blob([bytes], { type: "application/octet-stream" });
  console.log(file.size > 0);
} finally {
  scene.free();
  data.free();
}
```

```text
minecraft:stone_bricks
[]
true
```

## Import files

For ordinary file import, pass `undefined` as the options argument:

```javascript
const scene = await data.fromBytes(new Uint8Array(buffer), "litematic", undefined);
```

See the generated [API reference](../../docs/reference/api.md#browser-wasm) for methods,
arguments, and behavior. The build also produces `pkg/mcschemora.d.ts` for editors
and TypeScript consumers.

See [conversion](../../docs/guides/usage.md#inspect-and-convert) and
[catalogs and assets](../../docs/reference/minecraft-data.md) for shared workflows.
