"""Export a gallery and check geometry through the public API.

Run: uv run --all-packages python examples/render.py [--cache-dir PATH] [--offline]
"""

import argparse
import json
import struct
from pathlib import Path

from schemora import MinecraftData, Schematic, block


def read_glb(path):
    content = path.read_bytes()
    magic, version, length = struct.unpack_from("<4sII", content)
    assert magic == b"glTF" and version == 2 and length == len(content)
    json_length, chunk = struct.unpack_from("<I4s", content, 12)
    assert chunk == b"JSON"
    return json.loads(content[20 : 20 + json_length])


def triangles(document):
    return sum(
        document["accessors"][primitive["indices"]]["count"] // 3
        for node in document["nodes"]
        for primitive in document["meshes"][node["mesh"]]["primitives"]
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache-dir")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--version", default="1.21.1")
    args = parser.parse_args()
    data = MinecraftData(cache_dir=args.cache_dir, offline=args.offline)
    output = Path(__file__).parent / "output"
    output.mkdir(exist_ok=True)

    pair = Schematic.create(version=args.version, data=data)
    pair.region().set_many([((0, 0, 0), block("stone")), ((1, 0, 0), block("stone"))])
    assert not pair.export_glb(output / "pair.glb")
    assert triangles(read_glb(output / "pair.glb")) == 20

    cube = Schematic.create(version=args.version, data=data)
    cube.region().select(start=(0, 0, 0), size=(2, 2, 2)).fill(block("stone"))
    cube.export_glb(output / "cube.glb")
    cube.export_glb(output / "slice.glb", y=0)
    assert triangles(read_glb(output / "cube.glb")) == 48
    assert triangles(read_glb(output / "slice.glb")) == 32

    scene = Schematic.create(version=args.version, data=data)
    region = scene.region()
    region.select(start=(0, 0, 0), size=(16, 1, 8)).fill(block("stone_bricks"))
    entries = [
        block("oak_log", axis="x"),
        block("oak_log", axis="y"),
        block("oak_log", axis="z"),
        block("oak_stairs", facing="east", half="bottom", shape="straight"),
        block("oak_stairs", facing="south", half="top", shape="inner_left"),
        block("oak_slab", type="bottom"),
        block("oak_fence", north=True, south=True),
        block("oak_fence", east=True, west=True),
        block("redstone_wire", north="side", south="up", east="none", west="none"),
        block("glass"),
        block("red_stained_glass"),
        block("grass_block"),
        block("dandelion"),
        block("lever", face="floor", facing="east"),
        block("chest"),
        block("water"),
    ]
    region.set_many([((i, 1, 3), value) for i, value in enumerate(entries)])
    diagnostics = scene.export_glb(output / "gallery.glb")
    assert scene.export_png(output / "gallery.png", size=(960, 720)) == diagnostics
    png = (output / "gallery.png").read_bytes()
    assert png[:8] == b"\x89PNG\r\n\x1a\n"
    assert struct.unpack_from(">II", png, 16) == (960, 720)
    cube.export_png(output / "slice.png", region="main", y=0, camera="top_down")
    invalid = output / "invalid.png"
    invalid.unlink(missing_ok=True)
    for options in [
        {"size": (0, 100)},
        {"size": (True, 100)},
        {"size": (4097, 100)},
        {"y": (2, 1)},
        {"y": 100},
        {"region": "missing"},
        {"camera": "unknown"},
    ]:
        try:
            scene.export_png(invalid, **options)
        except ValueError:
            pass
        else:
            raise AssertionError(f"Accepted invalid render options: {options}")
        assert not invalid.exists()
    assert not any("placeholder" in message for message in diagnostics)
    assert any("Tint indices" in message for message in diagnostics)
    document = read_glb(output / "gallery.glb")
    assert {material["alphaMode"] for material in document["materials"]} == {
        "OPAQUE",
        "MASK",
        "BLEND",
    }
    for message in diagnostics:
        print(message)
    print(f"Gallery: {output / 'gallery.glb'} ({triangles(document)} triangles)")


if __name__ == "__main__":
    main()
