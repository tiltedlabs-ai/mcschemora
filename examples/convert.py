import argparse
from pathlib import Path

from mcschemora import Schematic


def main():
    parser = argparse.ArgumentParser(description="Inspect and convert a schematic.")
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--version", help="Target Minecraft Java version")
    parser.add_argument("--flatten", action="store_true")
    parser.add_argument("--allow-loss", action="store_true")
    args = parser.parse_args()
    schematic = Schematic.load(args.source)
    print(f"Edition: {schematic.edition}; version: {schematic.version}")
    print(f"Regions: {', '.join(schematic.regions)}")
    format_name = (
        "blueprint" if args.destination.suffix == ".wiki" else args.destination.suffix.lstrip(".")
    )
    report = schematic.check_export(format=format_name, version=args.version, flatten=args.flatten)
    print(report)
    if report.errors or (report.losses and not args.allow_loss):
        raise SystemExit("Export blocked; resolve errors or explicitly accept the reported losses.")
    args.destination.parent.mkdir(parents=True, exist_ok=True)
    schematic.save(
        args.destination, version=args.version, flatten=args.flatten, allow_loss=args.allow_loss
    )
    print(f"Saved {args.destination.name}")


if __name__ == "__main__":
    main()
