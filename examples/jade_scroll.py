import argparse
import random
from math import floor, sin, sqrt
from pathlib import Path

from mcschemora import Schematic, block


# inspired by shan shui https://shan-shui-inf.lingdong.works/
def make_noise(seed):
    permutation = list(range(256))
    random.Random(seed).shuffle(permutation)
    permutation *= 2
    gradients = (
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (0.7, 0.7),
        (-0.7, 0.7),
        (0.7, -0.7),
        (-0.7, -0.7),
    )

    def noise(x, z):
        ix, iz = floor(x), floor(z)
        dx, dz = x - ix, z - iz
        fx = dx * dx * dx * (dx * (dx * 6 - 15) + 10)
        fz = dz * dz * dz * (dz * (dz * 6 - 15) + 10)
        values = []
        for oz in (0, 1):
            for ox in (0, 1):
                gx, gz = gradients[
                    permutation[permutation[(ix + ox) & 255] + ((iz + oz) & 255)] & 7
                ]
                values.append(gx * (dx - ox) + gz * (dz - oz))
        near = values[0] + fx * (values[1] - values[0])
        far = values[2] + fx * (values[3] - values[2])
        return near + fz * (far - near)

    return noise


def create_scroll(seed=42):
    schematic = Schematic.create(version="1.21.1")
    noise = make_noise(seed)
    rng = random.Random(seed)
    cells = {}
    surfaces = {}
    materials = {}

    def material(name, **properties):
        key = (name, tuple(properties.items()))
        if key not in materials:
            materials[key] = block(name, **properties)
        return materials[key]

    def put(x, y, depth, name, **properties):
        cells[(x, y, depth - x)] = material(name, **properties)

    for x in range(-10, 203):
        taper = sqrt(max(0, 1 - ((x - 96) / 108) ** 2))
        bend = 4 * sin(x * 0.035)
        half = int(38 * taper + 3 * noise(x * 0.08, 0))
        for depth in range(int(bend) - half, int(bend) + half + 1):
            put(x, -2, depth, "deepslate")
            put(x, -1, depth, "cyan_terracotta")
            put(x, 0, depth, "water", level=0)

    islands = (
        (27, -22, 21, 10, 1, 34, "far"),
        (68, -8, 27, 13, 1, 67, "middle"),
        (137, -23, 25, 10, 1, 43, "far"),
        (185, -3, 12, 8, 1, 23, "middle"),
        (24, 20, 16, 9, 17, 8, "near"),
        (105, 19, 20, 11, 26, 10, "near"),
        (163, 16, 17, 10, 19, 17, "near"),
        (134, 32, 7, 5, 12, 4, "near"),
    )
    palettes = {
        "far": ("light_gray_concrete", "diorite", "calcite"),
        "middle": ("waxed_oxidized_copper", "tuff", "mossy_cobblestone"),
        "near": ("tuff", "dark_prismarine", "mossy_cobblestone"),
    }

    for cx, cd, rx, rd, base, height, layer in islands:
        peaks = (
            (
                -rx * rng.uniform(0.15, 0.4),
                -rd * 0.14,
                rx * rng.uniform(0.28, 0.42),
                rd * rng.uniform(0.5, 0.75),
                1,
            ),
            (
                rx * rng.uniform(0.15, 0.4),
                rd * rng.uniform(-0.1, 0.3),
                rx * rng.uniform(0.2, 0.3),
                rd * 0.48,
                rng.uniform(0.45, 0.75),
            ),
            (rx * 0.58, -rd * rng.uniform(0.1, 0.45), rx * 0.23, rd * 0.38, rng.uniform(0.2, 0.5)),
        )
        for x in range(cx - rx - 4, cx + rx + 5):
            for depth in range(cd - rd - 3, cd + rd + 4):
                u = x - cx + 4 * noise(x * 0.09, depth * 0.09)
                v = depth - cd + 2 * noise(x * 0.07 + 11, depth * 0.11)
                radial = (u / rx) ** 2 + (v / rd) ** 2
                if radial >= 1:
                    continue
                shelf = 3 + 3 * (1 - radial) + 2 * noise(x * 0.16, depth * 0.16)
                crest = max(
                    amplitude * max(0, 1 - ((u - px) / sx) ** 2 - ((v - pd) / sd) ** 2) ** 0.85
                    for px, pd, sx, sd, amplitude in peaks
                )
                top = int(base + shelf + height * crest * (1 + 0.16 * noise(x * 0.23, depth * 0.2)))
                bottom = 0 if base == 1 else max(5, int(base - 12 * (1 - radial) ** 0.7))
                surfaces[(x, depth)] = top
                rocks = tuple(
                    palettes[layer][
                        max(
                            0,
                            min(
                                2,
                                int(
                                    (0.5 + noise(x * 0.09 + y * 0.027, depth * 0.11 - y * 0.018))
                                    * 3
                                ),
                            ),
                        )
                    ]
                    for y in range(bottom, top + 1, 4)
                )
                for y in range(bottom, top + 1):
                    if y == top:
                        rock = "moss_block" if layer != "far" or top < base + 8 else "calcite"
                    elif y < base - 2:
                        rock = "deepslate" if y < base - 6 else "dark_prismarine"
                    else:
                        rock = rocks[(y - bottom) // 4]
                    put(x, y, depth, rock)

    def pine(x, depth, height):
        ground = surfaces.get((x, depth))
        if ground is None:
            return
        lean = rng.choice((-1, 1))
        for y in range(1, height + 1):
            shift = lean if y > height // 2 else 0
            put(x + shift, ground + y, depth, "spruce_log", axis="y")
        put(x + lean, ground + height + 1, depth, "azalea_leaves", persistent=True)
        for rise in range(3, height + 1, 2):
            radius = max(1, (height - rise) // 3 + 1)
            for dx in range(-radius, radius + 1):
                for dz in range(-radius, radius + 1):
                    if abs(dx) + abs(dz) > radius + 1 or (dx == 0 and dz == 0):
                        continue
                    shift = lean if rise > height // 2 else 0
                    put(x + shift + dx, ground + rise, depth + dz, "azalea_leaves", persistent=True)

    for x, depth, height in (
        (18, 21, 10),
        (29, 17, 8),
        (15, 16, 6),
        (92, 20, 11),
        (111, 24, 8),
        (116, 16, 7),
        (158, 20, 10),
        (174, 17, 7),
        (131, 32, 5),
        (58, -2, 8),
        (78, -3, 7),
        (180, 1, 6),
        (37, -19, 5),
    ):
        pine(x, depth, height)

    def waterfall(x, depth):
        edge = depth
        while (x, edge + 1) in surfaces:
            edge += 1
        level = max(
            surfaces.get((px, pd), 0)
            for px in range(x - 1, x + 2)
            for pd in range(edge - 4, edge + 1)
        )
        for px in range(x - 1, x + 2):
            for pd in range(edge - 4, edge + 1):
                ground = surfaces.get((px, pd), level - 1)
                for y in range(ground + 1, level + 1):
                    put(px, y, pd, "mossy_cobblestone")
                put(px, level + 1, pd, "water", level=0)
        for px in (x, x + 1):
            for y in range(1, level + 2):
                put(px, y, edge + 1, "water", level=8)
        for px in range(x - 2, x + 4):
            put(px, -1, edge + 2, "prismarine")

    waterfall(30, 20)
    waterfall(113, 20)
    waterfall(165, 18)

    pavilion_x, pavilion_d = 100, 18
    floor_y = (
        max(
            surfaces.get((x, depth), 0)
            for x in range(pavilion_x - 3, pavilion_x + 4)
            for depth in range(pavilion_d - 3, pavilion_d + 4)
        )
        + 1
    )
    for x in range(pavilion_x - 3, pavilion_x + 4):
        for depth in range(pavilion_d - 3, pavilion_d + 4):
            ground = surfaces.get((x, depth), floor_y - 1)
            for y in range(ground + 1, floor_y):
                put(x, y, depth, "mossy_cobblestone")
            put(x, floor_y, depth, "dark_oak_planks")
    for dx in (-2, 2):
        for dd in (-2, 2):
            for y in range(floor_y + 1, floor_y + 6):
                put(pavilion_x + dx, y, pavilion_d + dd, "stripped_dark_oak_log", axis="y")
    for offset in range(-2, 3):
        for side in (-2, 2):
            put(pavilion_x + offset, floor_y + 6, pavilion_d + side, "dark_oak_planks")
            put(pavilion_x + side, floor_y + 6, pavilion_d + offset, "dark_oak_planks")
    for dx in range(-4, 5):
        for dd in range(-4, 5):
            distance = max(abs(dx), abs(dd))
            y = floor_y + 9 - distance
            if distance == 4:
                y += 1
            put(pavilion_x + dx, y, pavilion_d + dd, "dark_prismarine")
    put(pavilion_x, floor_y + 10, pavilion_d, "dark_prismarine")
    put(pavilion_x, floor_y + 4, pavilion_d, "lantern", hanging=True)
    put(pavilion_x, floor_y + 5, pavilion_d, "dark_oak_planks")

    for index in range(3):
        cx, cd = 86 + index * 4, 26 + index
        level = 11 + index * 2
        for y in range(level - 4, level):
            put(cx, y, cd, "deepslate")
        for dx in range(-2, 3):
            for dd in range(-1, 2):
                put(cx + dx, level, cd + dd, "moss_block")

    schematic.region().set_many(cells.items())
    return schematic


def main():
    parser = argparse.ArgumentParser(
        description="Build and render a jade mountain scroll with floating islands."
    )
    parser.add_argument("--output", type=Path, default=Path("examples/output"))
    parser.add_argument("--seed", type=int, default=42)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    schematic = create_scroll(args.seed)
    schematic.save(args.output / "jade-scroll.schem")
    image_path = args.output / "jade-scroll.png"
    diagnostics = schematic.export_png(image_path, size=(3840, 1600))
    print(
        f"Saved jade-scroll.schem and jade-scroll.png ({image_path.stat().st_size / 1024:.0f} KiB)"
    )
    for diagnostic in diagnostics:
        print(diagnostic)


if __name__ == "__main__":
    main()
