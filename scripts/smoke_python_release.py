import argparse
import json
import struct
from importlib.metadata import version
from importlib.resources import files
from pathlib import Path
from tempfile import TemporaryDirectory

import mcschemora
from mcschemora import Schematic, block, chest, mob


def main():
    parser = argparse.ArgumentParser(description="Verify an installed MCSchemora distribution.")
    parser.add_argument("--render", action="store_true")
    args = parser.parse_args()
    assert mcschemora.__version__ == version("mcschemora")
    package = files("mcschemora")
    for name in ("py.typed", "_core.pyi", "licenses/LICENSE", "licenses/NOTICE"):
        assert package.joinpath(name).is_file(), name
    for name in ("block-models/LICENSE", "block-models/SOURCE.md", "runtime-data/SOURCE.md"):
        assert package.joinpath("licenses", name).read_text(encoding="utf-8"), name
    for name in ("entity-models", "wiki-sprites"):
        attribution = json.loads(
            package.joinpath("licenses", name, "ATTRIBUTION.json").read_bytes()
        )
        assert attribution["source"], name

    with TemporaryDirectory(prefix="mcschemora-smoke-") as directory:
        output = Path(directory)
        schematic = Schematic.create(version="1.21.1")
        region = schematic.region()
        region.select(start=(0, 0, 0), size=(3, 1, 3)).fill(block("stone_bricks"))
        region.place(chest(facing="north"), at=(0, 1, 0))
        region.entities.add(mob("pig"), at=(1.5, 1.0, 1.5))
        assert not schematic.validate().issues
        for extension in ("schem", "litematic", "nbt", "snbt"):
            path = output / f"build.{extension}"
            schematic.save(path)
            restored = Schematic.load(path)
            assert restored.region().get((1, 0, 1)) == block("stone_bricks"), extension
            assert restored.region().get((0, 1, 0)).id == "minecraft:chest", extension
        sprites = output / "sprites.png"
        schematic.export_sprites(sprites, y=0, cell_size=16)
        assert sprites.read_bytes().startswith(b"\x89PNG\r\n\x1a\n")
        if args.render:
            preview = output / "preview.png"
            assert not schematic.export_png(preview, size=(256, 256))
            assert struct.unpack(">II", preview.read_bytes()[16:24]) == (256, 256)
            model = output / "preview.glb"
            assert not schematic.export_glb(model)
            assert model.read_bytes().startswith(b"glTF")
    print(f"MCSchemora {mcschemora.__version__}: installed package checks passed")


if __name__ == "__main__":
    main()
