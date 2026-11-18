---
description: Load a downloaded Litematica moss farm, inspect its blocks and inventory, and render a preview with MCSchemora.
---

# Importing a farm

You can inspect and preview a downloaded build in MCSchemora without running
Minecraft. This example uses AChromoss v0.4, a `.litematic` moss-based bonemeal
farm by [OpenBagTwo](https://github.com/OpenBagTwo/Schematics), based on
[Chromoss by maidofmoss](https://www.youtube.com/watch?v=eNYZiuVNBe0).

Download [the original schematic](../assets/examples/imported-farm/achromoss-v0.4.litematic)
and [import_farm.py](downloads/import_farm.py) into the same directory, then run:

```sh
uv run --with mcschemora import_farm.py achromoss-v0.4.litematic
```

## Load the farm

Load the file with an unpacked-size limit, then inspect its version and regions:

```python
from mcschemora import Schematic

scene = Schematic.load("achromoss-v0.4.litematic", max_unpacked_bytes=64 * 1024 * 1024)
print(scene.version)
print(scene.inspect()["regions"])
```

The file identifies itself as Java 1.21.4. Its single region is 15 × 14 × 15
blocks and contains 1,487 non-air blocks. Loading preserves the region origin,
block states, inventories, and entity data. Choose the decompression limit for
the files your application accepts.

`scene.inspect()` separates placed blocks from stored inventory items. This farm
has 157 hoppers and 44 composters; its inventories hold 3,632 bone meal. The
report records inspection completeness and diagnostics to help identify
unsupported payloads.

For this file, `scene.validate()` found no structural errors or warnings. One
sign-support check remains unknown because it needs surrounding blocks. These
checks do not simulate redstone or measure farm output, so they cannot establish
whether the farm will operate correctly.

## Preview

Render the loaded scene and inspect the returned diagnostics:

```python
diagnostics = scene.export_png("farm.png", size=(1400, 1200))
for diagnostic in diagnostics:
    print(diagnostic)
```

Rendering leaves the schematic unchanged and uses the shared Java 1.21.6 visual
assets. This preview has four diagnostics: three grass states lack evaluated
tint colors, and the item-frame entity is outside the renderer's supported
entity coverage. Check the [complete report](../assets/examples/imported-farm/report.json)
for those diagnostics alongside inspection and validation results.

<details markdown="1">
<summary>Read the complete script</summary>

```python title="import_farm.py"
--8<-- "examples/import_farm.py"
```

</details>

![MCSchemora render of the imported AChromoss moss farm](../assets/examples/imported-farm/preview.png)

[Download the schematic](../assets/examples/imported-farm/achromoss-v0.4.litematic){ .md-button }
[Download the PNG](../assets/examples/imported-farm/preview.png){ .md-button }
