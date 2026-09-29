"""Check atomic edits, copies, versioned helpers, and retained file data."""

from gzip import compress, decompress
from io import BytesIO

import nbtlib as n
from schemora import Schematic, bed, block, chest, item, mob, sign


def fails(call):
    try:
        call()
    except (ValueError, TypeError, OverflowError):
        return
    raise AssertionError("Expected operation to fail")


scene = Schematic.create(version="1.21.1")
r = scene.region()
r.set((0, 0, 0), block("stone"))
bounds = r.bounds
fails(
    lambda: r.set_many([((1, 0, 0), block("stone")), ((2, 0, 0), block("stone", facing="north"))])
)
assert r.bounds == bounds and r.get((1, 0, 0)).id == "minecraft:air"
r.set((1, 0, 0), block("lever"))
a = r.select(start=(0, 0, 0), size=(2, 1, 1))
fails(lambda: a.patch(powered=True))
assert r.get((1, 0, 0)).states["powered"] == "false"
a.move(offset=(1, 0, 0))
assert r.get((0, 0, 0)).id == "minecraft:air" and r.get((1, 0, 0)).id == "minecraft:stone"
assert a.bounds.start == (1, 0, 0)
r.set((4, 0, 0), block("stone"))
before = a.bounds
fails(lambda: a.move(offset=(2, 0, 0)))
assert a.bounds == before and r.get((1, 0, 0)).id == "minecraft:stone"
r.set((5, 0, 0), block("stone"))
fails(lambda: r.place(bed(head_toward="east"), at=(4, 0, 0)))
assert r.get((4, 0, 0)).id == "minecraft:stone"

# Selected air overwrites destinations; a filtered copy skips unselected cells.
fragment = r.select(start=(0, 0, 0), size=(2, 1, 1)).copy()
r.set((10, 0, 0), block("stone"))
r.set((10, 0, 0), fragment)
assert r.get((10, 0, 0)).id == "minecraft:air"
filtered = r.select(start=(0, 0, 0), size=(2, 1, 1)).select(block="stone").copy()
r.set((12, 0, 0), filtered)
assert r.get((12, 0, 0)).id == "minecraft:stone"

ref = r.entities.add(mob("pig", nbt="{UUID:[I;1,2,3,4],UUIDMost:1L,UUIDLeast:2L}"), at=(20.5, 0, 0))
a = r.select(start=(20, 0, 0), size=(1, 1, 1))
a.duplicate(offset=(1, 0, 0))
r.set((22, 0, 0), a.copy())
refs = list(r.entities)
assert len(set(refs)) == 3
for new_ref, x in zip(refs[1:], (21.5, 22.5), strict=True):
    entity = r.entities.get(new_ref)
    assert entity.position == (x, 0.0, 0.0)
    assert not any(k.startswith("UUID") for k in n.parse_nbt(entity.nbt))
r.entities.update(ref, nbt='{id:"minecraft:pig",TileY:5}')
for call in (lambda: a.duplicate(offset=(5, 0, 0)), lambda: r.set((25, 0, 0), a.copy())):
    fails(call)
    assert list(r.entities) == refs
fails(lambda: r.entities.update(ref, position=(float("nan"), 0, 0)))
assert r.entities.get(ref).position == (20.5, 0.0, 0.0)

for version in ("1.13", "1.20.4", "1.20.5", "1.21.1", "1.21.5", "latest"):
    d = Schematic.create(version=version)
    region = d.region()
    region.place(chest(items={0: item("stone", count=3)}), at=(0, 0, 0))
    region.place(sign(["hello"]), at=(1, 0, 0))
    region.set((1, -1, 0), block("stone"))
    inventory = n.parse_nbt(region.block_entities.get((0, 0, 0)))["Items"][0]
    assert ("count" in inventory) == (d.data_version >= 3837)
    text = n.parse_nbt(region.block_entities.get((1, 0, 0)))
    assert ("front_text" in text) == (d.data_version >= 3463)
    if d.data_version >= 3463:
        assert isinstance(text["front_text"]["messages"][0], n.Compound) == (d.data_version >= 4325)
    assert d.validate().ok

sparse = Schematic.from_bytes(
    b'{DataVersion:3955,size:[3,1,1],palette:[{Name:"minecraft:air"},{Name:"minecraft:stone"}],blocks:[{pos:[0,0,0],state:1},{pos:[2,0,0],state:0}],entities:[]}',
    format="snbt",
)
sparse.add_region("other", origin=(5, 0, 0)).set((0, 0, 0), block("stone"))
report = sparse.check_export(format="snbt", flatten=True)
assert not report.errors and report.losses
out = n.parse_nbt(sparse.to_bytes(format="snbt", flatten=True, allow_loss=True).decode())
assert {tuple(map(int, b["pos"])) for b in out["blocks"]} == {(0, 0, 0), (2, 0, 0), (5, 0, 0)}
sparse.add_region("overlap", origin=(2, 0, 0)).set((0, 0, 0), block("air"))
assert sparse.check_export(format="schem", flatten=True).errors
fails(lambda: sparse.to_bytes(format="schem", flatten=True, allow_loss=True))

unmapped = Schematic.create(version="1.21.1")
unmapped.region().set((0, 0, 0), block("sculk"))
assert unmapped.check_export(format="schematic").errors
fails(lambda: unmapped.to_bytes(format="schematic", allow_loss=True))

root = n.parse_nbt(
    """{format_version:1,size:[2,1,1],structure_world_origin:[10,0,0],structure:{block_indices:[[0,-1],[1,-1]],entities:[{id:"minecraft:pig",Pos:[10.5f,0f,0f]}],palette:{default:{block_palette:[{name:"minecraft:chest",states:{facing_direction:2},version:18153472},{name:"minecraft:water",states:{liquid_depth:0b},version:18153472}],block_position_data:{"0":{block_entity_data:{id:"minecraft:chest",Items:[]},keep:7b}}}}}}"""
)
buf = BytesIO()
n.File(root).write(buf, byteorder="little")
d = Schematic.from_bytes(buf.getvalue(), format="mcstructure")
d.region().block_entities.remove((0, 0, 0))
assert d.check_export(format="mcstructure").ok
out = n.File.parse(BytesIO(d.to_bytes(format="mcstructure")), byteorder="little")
pal = out["structure"]["palette"]["default"]
assert "block_entity_data" not in pal["block_position_data"]["0"]
assert pal["block_position_data"]["0"]["keep"] == n.Byte(7)
water = pal["block_palette"][int(out["structure"]["block_indices"][1][0])]
assert water["name"] == "minecraft:water" and isinstance(water["states"]["liquid_depth"], n.Byte)
assert out["structure"]["block_indices"][0][1] == -1
assert isinstance(out["structure"]["entities"][0]["Pos"][0], n.Float)
assert out["structure"]["entities"][0]["Pos"][0] == 10.5

# Full rotation/reflection cycles preserve rail, connection, and standing states.
d = Schematic.create(version="1.21.1")
r = d.region()
for i, b in enumerate(
    (
        block("rail", shape="north_east"),
        block("rail", shape="ascending_east"),
        block("oak_sign", rotation=3),
        block("redstone_wire", east="side"),
    )
):
    r.set((i, 0, 0), b)
original = [r.get((i, 0, 0)) for i in range(4)]
a = r.select(start=(0, 0, 0), size=(4, 1, 4))
a.rotate()
assert r.get((0, 0, 3)).states["shape"] == "north_west"
assert r.get((0, 0, 2)).states["shape"] == "ascending_north"
for _ in range(3):
    a.rotate()
assert [r.get((i, 0, 0)) for i in range(4)] == original
for _ in range(2):
    a.flip(axis="x")
assert [r.get((i, 0, 0)) for i in range(4)] == original
# Bulk fills preserve attached data for unchanged block types and leave mobs alone.
d = Schematic.create(version="1.21.1")
r = d.region()
r.place(chest(items={0: item("stone", count=3)}), at=(0, 0, 0))
r.set((4, 0, 0), block("stone"))
ref = r.entities.add(mob("pig"), at=(0.5, 0.0, 0.5))
inventory = r.block_entities.get((0, 0, 0))
single = r.select(start=(0, 0, 0), size=(1, 1, 1))
single.fill(block("chest", facing="east"))
assert r.block_entities.get((0, 0, 0)) == inventory
assert r.get((4, 0, 0)).id == "minecraft:stone"
whole = r.select(start=(0, 0, 0), size=(5, 1, 1))
whole.fill(block("chest"))
assert r.block_entities.get((0, 0, 0)) == inventory
whole.fill(block("stone"))
assert r.block_entities.get((0, 0, 0)) is None and list(r.entities) == [ref]

# Filtered fills affect only their captured cells; invalid growth is atomic.
selected = whole.select(block="stone")
r.set((6, 0, 0), block("stone"))
selected.fill(block("glass"))
assert all(r.get((x, 0, 0)).id == "minecraft:glass" for x in range(5))
assert r.get((6, 0, 0)).id == "minecraft:stone"
before = n.parse_nbt(d.to_bytes(format="snbt").decode())
fails(lambda: r.select(start=(1_000_000, 1_000_000, 0), size=(1, 1, 1)).fill(block("stone")))
assert n.parse_nbt(d.to_bytes(format="snbt").decode()) == before
selected.fill(block("air"))
assert all(r.get((x, 0, 0)).id == "minecraft:air" for x in range(5))
assert r.get((6, 0, 0)).id == "minecraft:stone" and list(r.entities) == [ref]
whole.select(block="diamond_block").fill(block("gold_block"))  # Empty filter.

# Retained scheduled ticks prevent resizing before any fill writes occur.
root = n.File.parse(BytesIO(decompress(d.to_bytes(format="litematic"))))
root["Regions"]["main"]["PendingBlockTicks"] = n.List[n.Compound]([n.Compound({"x": n.Int(6)})])
buf = BytesIO()
root.write(buf)
retained = Schematic.from_bytes(compress(buf.getvalue()), format="litematic")
rr = retained.region()
before_bounds = rr.bounds
fails(lambda: rr.select(start=(0, 0, 0), size=(8, 1, 1)).fill(block("gold_block")))
assert rr.bounds == before_bounds and rr.get((6, 0, 0)).id == "minecraft:stone"

# Sparse fill records explicit air only inside the selected area.
d = Schematic.from_bytes(
    b'{DataVersion:3955,size:[4,1,1],palette:[{Name:"minecraft:stone"}],blocks:[{pos:[0,0,0],state:0}],entities:[]}',
    format="snbt",
)
d.region().select(start=(1, 0, 0), size=(2, 1, 1)).fill(block("air"))
out = n.parse_nbt(d.to_bytes(format="snbt").decode())
assert {tuple(map(int, b["pos"])) for b in out["blocks"]} == {(0, 0, 0), (1, 0, 0), (2, 0, 0)}

print(
    "Atomic edits, bulk fills, overlap, copies, versioned helpers, sparse exports, Bedrock data, and transform cycles passed."
)
