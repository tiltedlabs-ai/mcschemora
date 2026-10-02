import argparse
import random
from pathlib import Path

from mcschemora import Schematic, block, mob

GLYPHS = {
    "M": ("10001", "11011", "10101", "10101", "10001", "10001", "10001"),
    "C": ("0111", "1000", "1000", "1000", "1000", "1000", "0111"),
    "S": ("0111", "1000", "1000", "0110", "0001", "0001", "1110"),
    "c": ("0000", "0000", "0111", "1000", "1000", "1000", "0111"),
    "h": ("1000", "1000", "1010", "1101", "1001", "1001", "1001"),
    "e": ("0000", "0000", "0110", "1001", "1111", "1000", "0111"),
    "m": ("00000", "00000", "11010", "10101", "10101", "10101", "10101"),
    "o": ("0000", "0000", "0110", "1001", "1001", "1001", "0110"),
    "r": ("000", "000", "101", "110", "100", "100", "100"),
    "a": ("0000", "0000", "0110", "0001", "0111", "1001", "0111"),
}


def create_banner():
    scene = Schematic.create(version="1.21.1")
    region = scene.region()
    rng = random.Random(42)
    cells = {}
    text = "MCSchemora"
    width = sum(len(GLYPHS[character][0]) + 1 for character in text) - 1

    def put(x, y, depth, material, **states):
        cells[(x, y, depth - x)] = block(material, **states)

    for x in range(-11, width + 13):
        inset = max(0, 5 - min(x + 11, width + 12 - x))
        edge = rng.randrange(3)
        for depth in range(-7 + inset + edge, 16 - inset - edge):
            put(x, 0, depth, "grass_block")
            put(x, -1, depth, "dirt")
            if -4 + inset < depth < 12 - inset:
                put(x, -2, depth, rng.choice(("stone", "andesite", "mossy_cobblestone")))
            if -1 + inset < depth < 9 - inset and -8 < x < width + 10:
                put(x, -3, depth, "stone")
            if 2 < depth < 6 and x % 5 != 0 and -4 < x < width + 7:
                put(x, -4, depth, "deepslate")
            if 9 <= depth <= 10:
                put(x, -1, depth, "clay")
                put(x, 0, depth, "water", level=0)
            if depth == 6:
                put(x, 0, depth, rng.choice(("cobblestone", "stone_bricks")))

    for x in range(-2, width + 3):
        for depth in range(-1, 4):
            put(x, 0, depth, "polished_deepslate")
            put(x, 1, depth, "stone_bricks")
            if depth in (0, 1, 2):
                put(x, 2, depth, "polished_deepslate")
        put(x, 1, 4, "waxed_oxidized_cut_copper")

    cursor = 0
    perched_mobs = {
        "M": (0, "fox", 45),
        "C": (2, "chicken", 225),
        "S": (2, "creeper", 45),
        "m": (1, "wolf", 45),
        "a": (2, "bee", 225),
    }
    for index, character in enumerate(text):
        for row, pixels in enumerate(GLYPHS[character]):
            for column, pixel in enumerate(pixels):
                if pixel == "1":
                    x, y = cursor + column, 9 - row
                    face = (
                        "gold_block"
                        if index < 2
                        else ("diamond_block" if row < 4 else "light_blue_concrete")
                    )
                    put(x, y, 2, face)
                    for depth in (0, 1):
                        put(x, y, depth, "orange_concrete" if index < 2 else "dark_prismarine")
        if character in perched_mobs:
            column, identifier, yaw = perched_mobs[character]
            top_row = next(
                row for row, pixels in enumerate(GLYPHS[character]) if pixels[column] == "1"
            )
            x = cursor + column
            region.entities.add(
                mob(identifier, nbt=f"{{Rotation:[{yaw}.0f,0.0f]}}"),
                at=(x + 0.5, 10.0 - top_row, 1.5 - x),
            )
        if character == "o":
            region.entities.add(
                mob("cat", nbt="{Rotation:[225.0f,0.0f]}"),
                at=(cursor + 2.0, 4.0, -float(cursor)),
            )
        cursor += len(GLYPHS[character][0]) + 1

    for x in range(5, width, 12):
        put(x, 1, 6, "stone_brick_wall")
        put(x, 2, 6, "lantern")

    for x in (8, width // 2, width - 7):
        for bridge_x in (x, x + 1):
            for depth in range(8, 13):
                put(bridge_x, 1, depth, "oak_slab", type="bottom")
        for depth in (8, 12):
            put(x - 1, 1, depth, "oak_fence")
            put(x + 2, 1, depth, "oak_fence")

    for tree_x, tree_depth in ((-7, 1), (-5, -4), (width + 10, 7)):
        for y in range(1, 6):
            put(tree_x, y, tree_depth, "oak_log", axis="y")
        for y, radius in ((3, 2), (4, 2), (5, 1), (6, 1)):
            for x in range(tree_x - radius, tree_x + radius + 1):
                for z_offset in range(-radius, radius + 1):
                    corner = abs(x - tree_x) == radius and abs(z_offset) == radius
                    if corner and (y == 6 or rng.randrange(2) == 0):
                        continue
                    if x == tree_x and z_offset == 0 and y <= 5:
                        continue
                    depth = tree_depth + x - tree_x + z_offset
                    put(x, y, depth, "oak_leaves", persistent=True)

    tower_x = width + 6

    def tower_put(x, y, z, material, **states):
        put(tower_x + x, y, -1 + x + z, material, **states)

    for x in range(-2, 3):
        for z in range(-2, 3):
            tower_put(x, 1, z, "cobblestone")
            tower_put(x, 5, z, "spruce_planks")
            tower_put(x, 8, z, "spruce_planks")
            if abs(x) == 2 or abs(z) == 2:
                for y in range(1, 8):
                    if abs(x) == 2 and abs(z) == 2 and y >= 3:
                        tower_put(x, y, z, "stripped_spruce_log", axis="y")
                    elif y <= 2:
                        material = "mossy_cobblestone" if (x + z + y) % 3 == 0 else "cobblestone"
                        tower_put(x, y, z, material)
                    else:
                        tower_put(x, y, z, "stone_bricks")

    for y in (4, 5):
        for side in (-2, 2):
            tower_put(side, y, 0, "light_blue_stained_glass")
        for x in (-1, 1):
            tower_put(x, y, 2, "light_blue_stained_glass")
    for y, half in ((1, "lower"), (2, "upper")):
        tower_put(0, y, 2, "spruce_door", facing="south", half=half, hinge="left")
    tower_put(0, 1, 3, "stone_brick_slab", type="bottom")
    tower_put(0, 6, 3, "spruce_planks")
    tower_put(0, 5, 3, "lantern", hanging=True)

    for z in range(-2, 3):
        for x in (-2, -1, 1, 2):
            tower_put(
                x,
                11 - abs(x),
                z,
                "spruce_stairs",
                facing="east" if x < 0 else "west",
                half="bottom",
                shape="straight",
            )
        tower_put(0, 11, z, "spruce_slab", type="bottom")
    for z in (-2, 2):
        tower_put(0, 9, z, "spruce_planks")
        tower_put(0, 10, z, "spruce_planks")
    tower_put(0, 12, 0, "spruce_fence", east=True)
    tower_put(1, 12, 0, "red_wool")

    for x in range(-8, width + 11, 4):
        for depth in (-5, 13):
            if cells.get((x, 0, depth - x)) == block("grass_block"):
                put(x, 1, depth, rng.choice(("poppy", "dandelion", "flowering_azalea")))

    region.set_many(cells.items())
    region.entities.add(mob("sheep"), at=(-2.5, 1.0, 15.5))
    region.entities.add(mob("pig"), at=(width + 1.5, 1.0, 11.5 - width))
    return scene


def main():
    parser = argparse.ArgumentParser(description="Build and render the MCSchemora block banner.")
    parser.add_argument("--output", type=Path, default=Path("examples/output"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    scene = create_banner()
    scene.save(args.output / "banner.schem")
    image_path = args.output / "banner.png"
    diagnostics = scene.export_png(image_path, size=(3840, 960))
    print(f"Saved banner.schem and banner.png ({image_path.stat().st_size / 1024:.0f} KiB)")
    for diagnostic in diagnostics:
        print(diagnostic)


if __name__ == "__main__":
    main()
