import argparse
from pathlib import Path

from build import create_scene
from mcschemora import block


def main():
    parser = argparse.ArgumentParser(description="Copy, rotate, and restyle the workshop.")
    parser.add_argument("--output", type=Path, default=Path("examples/output"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    scene = create_scene()
    area = scene.region().select(start=(0, 0, 0), size=(7, 5, 7))
    copy = area.duplicate(offset=(10, 0, 0))
    copy.rotate(steps=1)
    copy.replace("oak_log", block("birch_log", axis="y"))
    print(f"Original bounds: {area.bounds}")
    print(f"Copy bounds: {copy.bounds}")
    print(f"Birch logs: {copy.counts()['minecraft:birch_log[axis=y]']}")
    print(scene.validate())
    scene.save(args.output / "workshops.schem")
    print("Saved workshops.schem")


if __name__ == "__main__":
    main()
