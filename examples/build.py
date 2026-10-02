import argparse
from pathlib import Path

from mcschemora import Schematic, bed, block, chest, item


def create_scene():
    scene = Schematic.create(version="1.21.1")
    region = scene.region()
    region.select(start=(0, 0, 0), size=(7, 1, 7)).fill(block("stone_bricks"))
    for x, z in ((0, 0), (6, 0), (0, 6), (6, 6)):
        region.select(start=(x, 1, z), size=(1, 3, 1)).fill(block("oak_log", axis="y"))
    region.select(start=(0, 4, 0), size=(7, 1, 7)).fill(block("oak_slab", type="bottom"))
    region.place(bed(color="red", head_toward="north"), at=(1, 1, 4))
    region.place(chest(items={0: item("stone", count=64)}), at=(4, 1, 4))
    region.set((4, 1, 2), block("crafting_table"))
    region.set((4, 1, 1), block("furnace", facing="west"))
    return scene


def main():
    parser = argparse.ArgumentParser(description="Build a small Minecraft workshop.")
    parser.add_argument("--output", type=Path, default=Path("examples/output"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    scene = create_scene()
    area = scene.region().select(start=(0, 0, 0), size=(7, 5, 7))
    print(area.describe_layer(y=1))
    print(scene.validate())
    scene.save(args.output / "workshop.schem")
    print("Saved workshop.schem")


if __name__ == "__main__":
    main()
