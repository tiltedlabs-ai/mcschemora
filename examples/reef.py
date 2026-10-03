import argparse
from math import atan2, cos, sin, tau
from pathlib import Path

from mcschemora import Schematic, block

COLORS = (
    "purple",
    "blue",
    "light_blue",
    "cyan",
    "green",
    "lime",
    "yellow",
    "orange",
    "red",
    "magenta",
)


def create_reef(diameter=64, periods=2.5, thickness=0.35):
    if not 16 <= diameter <= 128:
        raise ValueError("Diameter must be between 16 and 128 blocks.")
    if not 1 <= periods <= diameter / 8:
        raise ValueError("Periods must be between 1 and diameter / 8.")
    if not 0 < thickness < 1:
        raise ValueError("Thickness must be between 0 and 1, exclusive.")

    scene = Schematic.create(version="1.21.1")
    palette = tuple(block(f"{color}_concrete") for color in COLORS)
    foundation = block("polished_blackstone")
    dark = block("black_concrete")
    center = (diameter - 1) / 2
    radius = diameter / 2
    radius_squared = radius * radius
    interior_radius_squared = (radius * 0.8) ** 2
    scale = tau * periods / diameter
    samples = tuple(
        (
            (coordinate - center) ** 2,
            sin((coordinate - center) * scale),
            cos((coordinate - center) * scale),
        )
        for coordinate in range(diameter)
    )
    bottom = diameter // 8
    twists = tuple(0.9 * (y - center) / diameter for y in range(diameter))

    def placements():
        for y, margin, material in ((0, 3, foundation), (1, 3, foundation), (2, 2, dark)):
            for x in range(-margin, diameter + margin):
                for z in range(-margin, diameter + margin):
                    yield (x, y, z), material

        for x, (x_squared, sx, cx) in enumerate(samples):
            for z, (z_squared, sz, cz) in enumerate(samples):
                remaining = radius_squared - x_squared - z_squared
                if remaining < 0:
                    continue
                interior_remaining = interior_radius_squared - x_squared - z_squared
                angle = atan2(z - center, x - center) / tau
                term = sx * cz
                for y in range(bottom, diameter):
                    y_squared, sy, cy = samples[y]
                    if y_squared <= remaining and abs(term + sy * cx + sz * cy) < thickness:
                        index = int(((angle + twists[y]) % 1) * len(palette))
                        material = dark if y_squared <= interior_remaining else palette[index]
                        yield (x, y - bottom + 3, z), material

    scene.region().set_many(placements())
    return scene


def main():
    parser = argparse.ArgumentParser(description="Build and render a rainbow gyroid reef.")
    parser.add_argument("--output", type=Path, default=Path("examples/output"))
    parser.add_argument("--diameter", type=int, default=64)
    parser.add_argument("--periods", type=float, default=2.5)
    parser.add_argument("--thickness", type=float, default=0.35)
    args = parser.parse_args()
    try:
        scene = create_reef(args.diameter, args.periods, args.thickness)
    except ValueError as error:
        parser.error(str(error))
    args.output.mkdir(parents=True, exist_ok=True)
    scene.save(args.output / "reef.schem")
    image_path = args.output / "reef.png"
    diagnostics = scene.export_png(image_path, size=(1600, 1600))
    print(f"Saved reef.schem and reef.png ({image_path.stat().st_size / 1024:.0f} KiB)")
    for diagnostic in diagnostics:
        print(diagnostic)


if __name__ == "__main__":
    main()
