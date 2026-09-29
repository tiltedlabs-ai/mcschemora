"""Check exported files with independent readers. Run build.py first."""

from pathlib import Path

import litemapy
import nbtlib
from schemora import Schematic

output = Path(__file__).parent / "output"
for extension in ("schem", "litematic", "nbt", "schematic"):
    name = "foundation" if extension == "schematic" else "workshop"
    root = nbtlib.load(output / f"{name}.{extension}")
    print(f"{extension}: NBT root keys: {', '.join(root.keys())}")

root = nbtlib.load(output / "foundation.mcstructure", byteorder="little")
print("mcstructure size:", list(root["size"]))
root = nbtlib.parse_nbt((output / "workshop.snbt").read_text())
print("SNBT blocks:", len(root["blocks"]))

external = litemapy.Schematic.load(str(output / "workshop.litematic"))
print("Litemapy regions:", list(external.regions))
for name, region in external.regions.items():
    print(name, "entities:", len(region.entities), "block entities:", len(region.tile_entities))

for path in sorted(output.iterdir()):
    if path.suffix not in (".schem", ".litematic", ".nbt", ".snbt", ".schematic", ".mcstructure"):
        continue
    document = Schematic.load(path)
    region = document.region(document.regions[0])
    before = region.select(start=region.bounds.start, size=region.bounds.size).counts()
    payload = document.to_bytes(format=path.suffix[1:])
    reread = Schematic.from_bytes(payload, format=path.suffix[1:])
    region = reread.region(reread.regions[0])
    after = region.select(start=region.bounds.start, size=region.bounds.size).counts()
    if before != after:
        raise RuntimeError(f"{path.name}: block states changed on round-trip")
    print(f"{path.name}: block-state round-trip passed")
