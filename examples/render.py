import argparse
from pathlib import Path

from build import create_scene


def main():
    parser = argparse.ArgumentParser(description="Render the workshop and export a wiki blueprint.")
    parser.add_argument("--output", type=Path, default=Path("examples/output"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    scene = create_scene()
    outputs = (
        ("workshop.png", scene.export_png, {"size": (800, 600), "y": (0, 3)}),
        ("workshop-layer.png", scene.export_sprites, {"y": 1, "grid": True}),
        ("workshop.glb", scene.export_glb, {}),
        ("workshop.wiki", scene.export_blueprint, {"name": "Workshop"}),
    )
    for name, export, options in outputs:
        diagnostics = export(args.output / name, **options)
        print(f"Saved {name}")
        for diagnostic in diagnostics:
            print(f"  {diagnostic}")


if __name__ == "__main__":
    main()
