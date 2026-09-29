"""Build, transform, and export a small technical Minecraft scene.

Run from the repository root: uv run --all-packages python examples/build.py
"""

from pathlib import Path

from schemora import Schematic, bed, block, chest, door, item, mob, sign, water_source

OUTPUT = Path(__file__).parent / "output"
OUTPUT.mkdir(exist_ok=True)

scene = Schematic.create(version="1.21.1")
region = scene.region("main")
region.select(start=(0, 0, 0), size=(12, 1, 12)).fill(block("stone_bricks"))
region.place(bed(color="red", head_toward="north"), at=(2, 1, 4))
region.place(door(facing="east"), at=(4, 1, 4))
region.place(chest(items={0: item("stone", count=64)}), at=(6, 1, 4))
region.place(sign(["Schemora", "Python + Rust"]), at=(8, 1, 4))
region.entities.add(mob("villager"), at=(9.5, 1.0, 7.5))

region.set((2, 1, 8), water_source())
region.set((3, 1, 8), block("oak_slab", type="bottom", waterlogged=True))
region.set((5, 1, 8), block("lever", face="floor", facing="north", powered=False))
region.set((6, 1, 8), block("repeater", facing="north", delay=2, powered=False, locked=False))
region.set(
    (7, 1, 8), block("redstone_wire", north="none", south="none", east="side", west="side", power=0)
)

module = region.select(start=(0, 0, 0), size=(12, 4, 12))
module.select(block="lever").patch(powered=True)
copy = module.duplicate(offset=(16, 0, 0))
copy.rotate(axis="y", steps=1)
copy.flip(axis="x")
copy.move(offset=(0, 0, 2))

print(module.describe_layer(y=1))
print("Validation:", scene.validate())
for extension in ("schem", "litematic", "nbt", "snbt"):
    path = OUTPUT / f"workshop.{extension}"
    scene.save(path)
    loaded = Schematic.load(path)
    print(f"{path.name}: {path.stat().st_size} bytes, regions={loaded.regions}")

# Portable materials for legacy and Bedrock formats.
portable = Schematic.create(version="1.21.1")
portable.region().select(start=(0, 0, 0), size=(4, 2, 4)).fill(block("stone"))
for extension in ("schematic", "mcstructure"):
    path = OUTPUT / f"foundation.{extension}"
    portable.save(path)
    print(f"{path.name}: {path.stat().st_size} bytes")
