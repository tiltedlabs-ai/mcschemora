import argparse
import hashlib
import json
from pathlib import Path
from urllib.request import urlopen


REVISION = "d78e38c4f12bf6f5eda94de1dea413a61c9eb728"
REPOSITORY = "https://github.com/PrismarineJS/prismarine-viewer"
SOURCE_PATH = "viewer/lib/entity/entities.json"
SOURCE_SHA256 = "e34240a776f0a6205fccc82dfb04bc07bbc3facb6ac711fea1f52332ef4154b7"
DESTINATION = Path(__file__).resolve().parents[1] / "src/mc_data/visual/entities.json"


def encode(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def prepare(raw):
    if hashlib.sha256(raw).hexdigest() != SOURCE_SHA256:
        raise ValueError("Entity source does not match the pinned revision")
    entities = {}
    models = {}
    for name, entity in sorted(json.loads(raw).items()):
        if name in {"player", "player_slim"}:
            continue
        variants = {}
        for variant, geometry in sorted(entity["geometry"].items()):
            identity = hashlib.sha256(encode(geometry)).hexdigest()
            models[identity] = geometry
            variants[variant] = identity
        textures = {}
        for slot, path in entity["textures"].items():
            if not path.startswith("textures/"):
                raise ValueError(f"Unexpected texture path: {path}")
            textures[slot] = "minecraft:" + path.removeprefix("textures/").removesuffix(".png")
        entities[f"minecraft:{name}"] = {
            "source_identifier": entity["identifier"],
            "models": variants,
            "textures": textures,
        }
    return {
        "format": 1,
        "source": {
            "repository": REPOSITORY,
            "revision": REVISION,
            "path": SOURCE_PATH,
            "sha256": SOURCE_SHA256,
            "license": (DESTINATION.parent / "PRISMARINE-LICENSE").read_text(),
        },
        "entities": entities,
        "models": models,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if args.source:
        raw = args.source.read_bytes()
    else:
        url = f"https://raw.githubusercontent.com/PrismarineJS/prismarine-viewer/{REVISION}/{SOURCE_PATH}"
        with urlopen(url, timeout=60) as response:
            raw = response.read()
    catalog = prepare(raw)
    output = encode(catalog) + b"\n"
    if args.check:
        if DESTINATION.read_bytes() != output:
            raise SystemExit("Bundled entity models are out of date")
    else:
        DESTINATION.write_bytes(output)
    print(f"{len(catalog['entities'])} entities, {len(catalog['models'])} models, {len(output)} bytes")


if __name__ == "__main__":
    main()
