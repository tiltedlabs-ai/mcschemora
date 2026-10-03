import argparse
from pathlib import Path

from build import create_schematic


def main():
    parser = argparse.ArgumentParser(description="Render the workshop and export a wiki blueprint.")
    parser.add_argument("--output", type=Path, default=Path("examples/output"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    schematic = create_schematic()
    outputs = (
        ("workshop.png", schematic.export_png, {"size": (800, 600), "y": (0, 3)}),
        ("workshop-layer.png", schematic.export_sprites, {"y": 1, "grid": True}),
        ("workshop.glb", schematic.export_glb, {}),
        ("workshop.wiki", schematic.export_blueprint, {"name": "Workshop"}),
    )
    for name, export, options in outputs:
        diagnostics = export(args.output / name, **options)
        print(f"Saved {name}")
        for diagnostic in diagnostics:
            print(f"  {diagnostic}")


if __name__ == "__main__":
    main()
