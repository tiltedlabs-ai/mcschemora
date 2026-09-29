"""Check structural validation through the public Python API; no game server needed."""

from time import perf_counter

from schemora import Schematic, bed, block, chest, door, item


def scene(version="1.21.1"):
    doc = Schematic.create(version=version)
    region = doc.region()
    region.select(start=(-4, -2, -4), size=(12, 12, 12)).fill(block("air"))
    return doc, region


def valid(doc):
    report = doc.validate()
    assert report.ok, str(report)
    return report


def error(doc, rule):
    report = doc.validate()
    assert any(rule in message for message in report.errors), str(report)
    return report


# Both portal axes, optional frame corners, holes, and malformed frames.
for axis in ("x", "z"):
    doc, region = scene()
    pos = (lambda x, y: (x, y, 0)) if axis == "x" else (lambda x, y: (0, y, x))
    for x in range(4):
        for y in range(5):
            edge_x, edge_y = x in (0, 3), y in (0, 4)
            if edge_x and edge_y:
                continue
            region.set(
                pos(x, y),
                block("obsidian") if edge_x or edge_y else block("nether_portal", axis=axis),
            )
    valid(doc)
    region.set(pos(1, 2), block("air"))
    error(doc, "portal.nether")
    region.set(pos(1, 2), block("nether_portal", axis=axis))
    region.set(pos(0, 1), block("crying_obsidian"))
    error(doc, "portal.nether")

# Entry portal orientation and eyes; exit fountain uses a different shape.
doc, region = scene()
for x in range(3):
    for z in range(3):
        region.set((x, 0, z), block("end_portal"))
for i in range(3):
    for pos, facing in (
        ((i, 0, -1), "south"),
        ((i, 0, 3), "north"),
        ((-1, 0, i), "east"),
        ((3, 0, i), "west"),
    ):
        region.set(pos, block("end_portal_frame", facing=facing, eye=True))
valid(doc)
region.patch((0, 0, -1), facing="north")
error(doc, "portal.end")
region.patch((0, 0, -1), facing="south", eye=False)
error(doc, "portal.end")

doc, region = scene()
for x in range(-3, 4):
    for z in range(-3, 4):
        d = x * x + z * z
        if d < 12.25:
            region.set((x, 0, z), block("bedrock" if d > 6.25 or d == 0 else "end_portal"))
valid(doc)

# Attachment faces use state-specific geometry, not bounding-box labels.
for version in ("1.13", "1.20.4", "1.21.1"):
    doc, region = scene(version)
    region.set((0, 0, 0), block("oak_slab", type="top"))
    region.set((0, 1, 0), block("rail"))
    valid(doc)
    region.patch((0, 0, 0), type="bottom")
    error(doc, "support")
    region.set((0, 0, 0), block("stone"))
    region.set((0, 1, 0), block("wall_torch", facing="east"))
    error(doc, "support")
    region.set((-1, 1, 0), block("stone"))
    valid(doc)

# Floating beds are legal. Doors need a floor and consistent halves.
doc, region = scene()
region.place(bed(head_toward="east"), at=(0, 1, 0))
valid(doc)
region.set((1, 1, 0), block("air"))
error(doc, "bed.pair")
doc, region = scene()
region.place(door(), at=(0, 0, 0))
error(doc, "support")
region.set((0, -1, 0), block("stone"))
valid(doc)
region.patch((0, 1, 0), open=True)
error(doc, "door.pair")

# Matching halves can be in different regions. A missing region is unknown.
doc = Schematic.create(version="1.21.1")
doc.region().set((0, 0, 0), block("red_bed", facing="east", part="foot"))
assert doc.validate().unknown and not doc.validate().errors
doc.add_region("head", origin=(1, 0, 0)).set(
    (0, 0, 0), block("red_bed", facing="east", part="head")
)
valid(doc)

# Sparse omissions are unknown; explicitly supplied air is a known failure.
doc = Schematic.from_bytes(
    b'{DataVersion:3955,size:[3,1,1],palette:[{Name:"minecraft:ladder",Properties:{facing:"east"}}],blocks:[{pos:[1,0,0],state:0}],entities:[]}',
    format="snbt",
)
assert doc.validate().unknown and not doc.validate().errors
doc.region().set((0, 0, 0), block("air"))
error(doc, "support")

# Dense and sparse indexes must agree, including after edits and at shifted origins.
for origin in ((0, 0, 0), (-100, 20, 30)):
    doc = Schematic.create(version="1.21.1")
    region = doc.add_region("checks", origin=origin)
    region.select(start=(-2, -2, -2), size=(3, 3, 3)).fill(block("stone"))
    valid(doc)
    region.set((-1, -1, -1), block("torch"))
    valid(doc)
    region.set((-1, -2, -1), block("air"))
    dense = error(doc, "support")
    region.set((100, 0, 0), block("stone"))  # Switch to a sparse position index.
    sparse = error(doc, "support")
    assert (dense.errors, dense.warnings, dense.unknown) == (
        sparse.errors,
        sparse.warnings,
        sparse.unknown,
    )

# Plant ground, paired plants, sugar cane water, and gravity warnings.
doc, region = scene()
region.set((0, 0, 0), block("stone"))
region.set((0, 1, 0), block("wheat"))
error(doc, "plant.soil")
region.set((0, 0, 0), block("farmland"))
valid(doc)
region.set((0, 0, 0), block("dirt"))
region.set((0, 1, 0), block("sugar_cane"))
error(doc, "plant.water")
region.set((1, 0, 0), block("water"))
valid(doc)
region.set((0, 1, 0), block("sunflower", half="lower"))
error(doc, "plant.pair")
region.set((0, 2, 0), block("sunflower", half="upper"))
valid(doc)
doc, region = scene()
region.set((0, 1, 0), block("sand"))
assert valid(doc).warnings
region.set((0, 1, 0), block("water", level=5))
assert not valid(doc).issues  # Fluid behavior is out of scope.

# Chest handedness and extended pistons.
doc, region = scene()
region.set((0, 0, 0), block("chest", facing="north", type="left"))
error(doc, "chest.pair")
region.set((1, 0, 0), block("chest", facing="north", type="right"))
valid(doc)
doc, region = scene()
region.set((0, 0, 0), block("sticky_piston", facing="up", extended=True))
error(doc, "piston.base")
region.set((0, 1, 0), block("piston_head", facing="up", type="sticky"))
valid(doc)

# Neighbor geometry, player-selectable redstone dots, and stale connections.
doc, region = scene()
region.set((0, 0, 0), block("oak_fence", east=True))
error(doc, "connection.side")
region.set((1, 0, 0), block("oak_fence", west=True))
valid(doc)
doc, region = scene()
region.set((0, 0, 0), block("cobblestone_wall"))
valid(doc)
region.patch((0, 0, 0), up=False)
error(doc, "wall.height")
doc, region = scene()
region.set((0, 0, 0), block("oak_stairs", facing="north", shape="outer_left"))
error(doc, "stairs.shape")
region.set((0, 0, -1), block("oak_stairs", facing="west", shape="straight"))
valid(doc)
doc, region = scene()
region.set((0, 0, 0), block("stone"))
region.set((0, 1, 0), block("redstone_wire"))
valid(doc)
region.patch((0, 1, 0), north="side", south="side", east="side", west="side")
valid(doc)
region.patch((0, 1, 0), north="up")
error(doc, "redstone.up")

# Validate retained inventory content rather than relying on authoring setters.
doc, region = scene()
region.place(chest(items={0: item("stone", count=3)}), at=(0, 0, 0))
valid(doc)
region.block_entities.set(
    (0, 0, 0),
    '{id:"minecraft:chest",Items:[{Slot:0b,id:"minecraft:stone",count:1},{Slot:0b,id:"minecraft:dirt",count:1}]}',
)
error(doc, "duplicate inventory slot")

# A large repeated-state scene exercises the palette cache and shared index.
doc = Schematic.create(version="1.21.1")
doc.region().select(start=(0, 0, 0), size=(100, 10, 100)).fill(block("stone"))
start = perf_counter()
valid(doc)
print(f"Validation checks passed; 100,000 blocks: {perf_counter() - start:.3f}s")
