import argparse
import gzip
import json
import struct
from copy import deepcopy
from io import BytesIO
from pathlib import Path
from tempfile import TemporaryDirectory

import nbtlib
from mcschemora import MinecraftData, Schematic

CODECS = ("nbt", "snbt", "schem", "litematic")

ITEMS = {
    "damage": '{id:"minecraft:diamond_sword",Count:1b,tag:{Damage:7,RepairCost:4,CustomModelData:2,Unbreakable:1b}}',
    "enchantment": '{id:"minecraft:diamond_sword",Count:1b,tag:{Enchantments:[{id:"minecraft:sweeping",lvl:2s}],HideFlags:1}}',
    "custom": '{id:"minecraft:stone",Count:4b,tag:{foo:{bar:[I;1,2],id:"minecraft:scute",Count:1b}}}',
    "lore": """{id:"minecraft:stone",Count:1b,tag:{display:{Name:'{"text":"Name"}',Lore:['{"text":"Lore"}']}}}""",
    "fireworks": '{id:"minecraft:firework_rocket",Count:2b,tag:{Fireworks:{Flight:3b,Explosions:[{Type:2b,Colors:[I;123],Trail:1b}]}}}',
    "lodestone": '{id:"minecraft:compass",Count:1b,tag:{LodestonePos:{X:4,Y:5,Z:6},LodestoneDimension:"minecraft:overworld",LodestoneTracked:0b}}',
    "book": """{id:"minecraft:written_book",Count:1b,tag:{title:"Notes",author:"Builder",pages:['{"text":"Hello"}'],filtered_pages:{"0":'{"text":"Filtered"}'}}}""",
    "attributes": '{id:"minecraft:diamond_sword",Count:1b,tag:{HideFlags:2,AttributeModifiers:[]}}',
    "profile": '{id:"minecraft:player_head",Count:1b,tag:{SkullOwner:{Name:"Builder",Properties:{textures:[{Value:"texture",Signature:"signature"}]}}}}',
    "container": '{id:"minecraft:shulker_box",Count:1b,tag:{BlockEntityTag:{Items:[{Slot:3b,id:"minecraft:scute",Count:4b}]}}}',
    "potion": '{id:"minecraft:potion",Count:1b,tag:{Potion:"minecraft:water",CustomPotionColor:123}}',
    "predicates": """{id:"minecraft:diamond_pickaxe",Count:1b,tag:{CanDestroy:['minecraft:chest[facing=north]{CustomName:"x",nested:{deeper:{n:1}}}']}}""",
}


def load(root, data):
    stream = BytesIO()
    nbtlib.File(root).write(stream)
    return Schematic.from_bytes(stream.getvalue(), format="nbt", data=data)


def decode(schematic, version=None, *, allow_loss=False):
    content = schematic.to_bytes(format="nbt", version=version, allow_loss=allow_loss)
    return nbtlib.File.parse(BytesIO(gzip.decompress(content)))


def export_load(source, version, codec, data, *, allow_loss=False):
    return Schematic.from_bytes(
        source.to_bytes(format=codec, version=version, allow_loss=allow_loss),
        format=codec,
        data=data,
    )


def chest(item, data_version):
    root = nbtlib.parse_nbt("""{
        DataVersion:3700,size:[1,1,1],palette:[{Name:"minecraft:chest"}],
        blocks:[{pos:[0,0,0],state:0,nbt:{id:"minecraft:chest",Items:[]}}],entities:[]
    }""")
    root["DataVersion"] = nbtlib.Int(data_version)
    if data_version >= 5006:
        root["palette"][0]["id"] = root["palette"][0].pop("Name")
    item["Slot"] = nbtlib.Byte(0)
    root["blocks"][0]["nbt"]["Items"] = nbtlib.List[nbtlib.Compound]([item])
    return root


def same_nbt(actual, expected, path):
    assert actual.tag_id == expected.tag_id, (path, type(actual), type(expected))
    if isinstance(expected, nbtlib.Compound):
        assert actual.keys() == expected.keys(), (path, actual, expected)
        for key in expected:
            same_nbt(actual[key], expected[key], f"{path}.{key}")
    elif isinstance(expected, nbtlib.List):
        assert len(actual) == len(expected), path
        for index, (left, right) in enumerate(zip(actual, expected, strict=True)):
            same_nbt(left, right, f"{path}[{index}]")
    elif isinstance(expected, nbtlib.Float):
        assert struct.pack(">f", actual) == struct.pack(">f", expected), (path, actual, expected)
    else:
        assert actual == expected, (path, actual, expected)


def verify_items(data):
    reference = json.loads(
        (Path(__file__).parent / "fixtures/conversion/item-components-1.20.5.json").read_text()
    )["expected"]
    for name, snbt in ITEMS.items():
        original = nbtlib.parse_nbt(snbt)
        source = load(chest(original, 3700), data)
        snapshot = decode(source)
        if name == "predicates":
            for codec in CODECS:
                report = source.check_export(format=codec, version="1.20.5")
                assert report.errors and "predicate.nbt" in str(report.errors), codec
            same_nbt(decode(source), snapshot, "predicate.source_unchanged")
            continue
        modern_nbt = decode(source, "1.20.5")
        modern_item = modern_nbt["blocks"][0]["nbt"]["Items"][0]
        if name in reference:
            same_nbt(modern_item, nbtlib.parse_nbt(reference[name]), f"{name}.vanilla_dfu")
        assert modern_nbt["DataVersion"] == 3837, name
        assert "Count" not in modern_item and "tag" not in modern_item, name
        assert isinstance(modern_item["count"], nbtlib.Int), name
        if name == "custom":
            custom = modern_item["components"]["minecraft:custom_data"]["foo"]
            assert custom["id"] == "minecraft:scute", name
            assert isinstance(custom["Count"], nbtlib.Byte), name
            assert isinstance(custom["bar"], nbtlib.IntArray), name
        if name == "enchantment":
            assert modern_item["components"]["minecraft:enchantments"]["levels"] == {
                "minecraft:sweeping_edge": nbtlib.Int(2)
            }, name
        if name == "container":
            nested = modern_item["components"]["minecraft:container"][0]
            assert nested["slot"] == 3, name
            assert nested["item"]["id"] == "minecraft:turtle_scute", name
        for codec in CODECS:
            modern = export_load(source, "1.20.5", codec, data)
            assert not modern.check_export(format=codec, version="1.20.4").issues, name
            restored = decode(modern, "1.20.4", allow_loss=True)
            assert restored["DataVersion"] == 3700, (name, codec)
            actual = restored["blocks"][0]["nbt"]["Items"][0]
            if name == "container":
                actual["tag"]["BlockEntityTag"].pop("id", None)
            assert actual == original, (name, codec, actual, original)
            assert modern.version == "1.20.5", (name, codec)
        assert decode(source) == snapshot and source.version == "1.20.4", name


def verify_failures(data):
    cases = {
        "collision": '{"minecraft:damage":2,"minecraft:custom_data":{Damage:3}}',
        "modern_only": '{"minecraft:max_stack_size":2}',
        "removed_default": '{"!minecraft:attribute_modifiers":{}}',
        "malformed_scalar": '{"minecraft:damage":{unknown:2}}',
    }
    with TemporaryDirectory(prefix="schemora-conversion-corpus-") as directory:
        path = Path(directory) / "target.nbt"
        for name, components in cases.items():
            item = nbtlib.parse_nbt(
                '{id:"minecraft:diamond_sword",count:1,components:' + components + "}"
            )
            source = load(chest(item, 3837), data)
            snapshot = decode(source)
            report = source.check_export(format="nbt", version="1.20.4")
            assert report.errors, name
            path.write_bytes(b"existing file")
            try:
                source.save(path, version="1.20.4", allow_loss=True)
            except ValueError:
                pass
            else:
                raise AssertionError(f"{name}: unsupported conversion accepted")
            assert path.read_bytes() == b"existing file", name
            assert decode(source) == snapshot, name
            assert len(list(path.parent.iterdir())) == 1, name


def verify_historical(data):
    def extract(root, path):
        for key in path:
            root = root[key]
        return root

    fixtures = json.loads(
        (Path(__file__).parent / "fixtures/conversion/historical.json").read_text()
    )["cases"]
    versions = {3337: "1.19.4", 3463: "1.20", 3465: "1.20.1", 3578: "1.20.2"}
    obsolete = {
        "sign": ["Text1", "Text2", "Text3", "Text4", "Color", "GlowingText", "FilteredText1"],
        "effects": ["ActiveEffects"],
        "beacon": ["Primary", "Secondary"],
        "beacon_empty": ["Primary", "Secondary"],
        "mooshroom": ["EffectId", "EffectDuration"],
        "potion": [],
    }
    for fixture in fixtures:
        name = fixture["name"]
        if name not in obsolete:
            continue
        original = nbtlib.parse_nbt(fixture["input"])
        expected = nbtlib.parse_nbt(fixture["expected"])
        for key in obsolete[name]:
            expected.pop(key)
        if fixture["type"] == "ITEM_STACK":
            root = chest(original, fixture["source_data_version"])
            expected["Slot"] = nbtlib.Byte(0)
            path = ("blocks", 0, "nbt", "Items", 0)
        else:
            root = nbtlib.parse_nbt("""{
                DataVersion:3337,size:[1,1,1],palette:[{Name:"minecraft:stone"}],
                blocks:[{pos:[0,0,0],state:0}],entities:[]
            }""")
            root["DataVersion"] = nbtlib.Int(fixture["source_data_version"])
            if fixture["type"] == "ENTITY":
                entity = nbtlib.parse_nbt("{pos:[0.5d,0d,0.5d],blockPos:[0,0,0]}")
                entity["nbt"] = original
                root["entities"] = nbtlib.List[nbtlib.Compound]([entity])
                expected["Pos"] = entity["pos"]
                path = ("entities", 0, "nbt")
            else:
                block = "minecraft:oak_sign" if name == "sign" else "minecraft:beacon"
                root["palette"][0]["Name"] = nbtlib.String(block)
                root["blocks"][0]["nbt"] = original
                path = ("blocks", 0, "nbt")
        source = load(root, data)
        snapshot = decode(source)
        for codec in CODECS:
            target = versions[fixture["target_data_version"]]
            content = source.to_bytes(format=codec, version=target)
            converted = Schematic.from_bytes(content, format=codec, data=data)
            actual = extract(decode(converted, allow_loss=True), path)
            same_nbt(actual, expected, f"{name}.{codec}.vanilla_dfu")
            if name != "beacon_empty":
                restored = extract(
                    decode(converted, versions[fixture["source_data_version"]], allow_loss=True),
                    path,
                )
                for key, value in original.items():
                    same_nbt(restored[key], value, f"{name}.{codec}.inverse.{key}")
        same_nbt(decode(source), snapshot, f"{name}.source_unchanged")


def verify_attributes(data):
    fixtures = json.loads(
        (Path(__file__).parent / "fixtures/conversion/attributes-1.21.json").read_text()
    )["cases"]
    for fixture in fixtures:
        original = nbtlib.parse_nbt(fixture["input"])
        expected = nbtlib.parse_nbt(fixture["expected"])
        entity = fixture["type"] == "ENTITY"
        if entity:
            root = nbtlib.parse_nbt("""{
                DataVersion:3837,size:[1,1,1],palette:[{Name:"minecraft:stone"}],
                blocks:[{pos:[0,0,0],state:0}],
                entities:[{pos:[0.5d,0d,0.5d],blockPos:[0,0,0],nbt:{}}]
            }""")
            root["entities"][0]["nbt"] = original
            expected.pop("Attributes")
            expected["Pos"] = root["entities"][0]["pos"]
        else:
            root = chest(original, fixture["source_data_version"])
            expected["Slot"] = nbtlib.Byte(0)
        source = load(root, data)
        snapshot = decode(source)
        report = source.check_export(format="nbt", version="1.21.1")
        assert not report.errors and report.issues, fixture["name"]
        assert any("modifiers[0].name" in issue for issue in report.issues)
        for codec in CODECS:
            converted = export_load(source, "1.21.1", codec, data, allow_loss=True)
            output = decode(converted, allow_loss=True)
            actual = (
                output["entities"][0]["nbt"] if entity else output["blocks"][0]["nbt"]["Items"][0]
            )
            same_nbt(actual, expected, f"{fixture['name']}.{codec}.vanilla_dfu")
        same_nbt(decode(source), snapshot, f"{fixture['name']}.source_unchanged")


def verify_earlier_routes(data):
    fixtures = json.loads(
        (Path(__file__).parent / "fixtures/conversion/historical-owners.json").read_text()
    )["cases"]
    versions = {
        1631: "1.13.2",
        1976: "1.14.4",
        2230: "1.15.2",
        2567: "1.16.1",
        2730: "1.17",
        2975: "1.18.2",
        3105: "1.19",
        3120: "1.19.2",
        3337: "1.19.4",
    }
    obsolete = {
        "uuids-next-cloud": ["OwnerUUIDMost", "OwnerUUIDLeast"],
        "uuids-next-fox": ["TrustedUUIDs"],
        "uuids-next-zvillager": ["ConversionPlayerMost", "ConversionPlayerLeast"],
        "uuids-next-pearl": ["owner"],
        "uuids-next-conduit": ["target_uuid"],
        "historical-more-potion": ["Potion"],
        "historical-more-furnace": [
            "RecipesUsedSize",
            "RecipeLocation0",
            "RecipeLocation1",
            "RecipeAmount0",
            "RecipeAmount1",
        ],
        "historical-more-jigsaw": ["target_pool", "attachement_type"],
        "historical-more-cat": ["CatType"],
        "historical-more-painting": ["Motive"],
        "last-historical-pickup": ["player"],
    }
    for fixture in fixtures:
        name = fixture["name"]
        original = nbtlib.parse_nbt(fixture["input"])
        expected = nbtlib.parse_nbt(fixture["expected"])
        kind = fixture["type"]
        root = nbtlib.parse_nbt(
            '{DataVersion:2230,size:[1,1,1],palette:[{Name:"minecraft:stone"}],'
            "blocks:[{pos:[0,0,0],state:0}],entities:[]}"
        )
        root["DataVersion"] = nbtlib.Int(fixture["source_data_version"])
        if kind == "ITEM_STACK":
            root = chest(original, fixture["source_data_version"])
            expected["Slot"] = nbtlib.Byte(0)
            path = ("blocks", 0, "nbt", "Items", 0)
        elif kind == "ENTITY":
            entry = nbtlib.parse_nbt("{pos:[0d,0d,0d],blockPos:[0,0,0]}")
            entry["nbt"] = original
            root["entities"] = nbtlib.List[nbtlib.Compound]([entry])
            expected["Pos"] = entry["pos"]
            path = ("entities", 0, "nbt")
        elif kind == "BLOCK_ENTITY":
            root["palette"][0]["Name"] = original["id"]
            root["blocks"][0]["nbt"] = original
            path = ("blocks", 0, "nbt")
        else:
            root["palette"][0] = original
            path = ()
        for key in obsolete.get(name, []):
            expected.pop(key, None)
        if name == "history-next-attrs":
            for attribute in expected["Attributes"]:
                if ":" not in attribute["Name"]:
                    attribute["Name"] = nbtlib.String("minecraft:" + attribute["Name"])
        if name == "last-historical-shulker":
            expected["Rotation"] = nbtlib.List[nbtlib.Float](
                [float(value) for value in expected["Rotation"]]
            )
        if name == "block-history-jigsaw_north":
            expected.pop("Properties")
        source = load(root, data)
        snapshot = decode(source)
        for codec in CODECS:
            target = versions[fixture["target_data_version"]]
            converted = export_load(source, target, codec, data, allow_loss=True)
            output = decode(converted, allow_loss=True)
            if kind == "BLOCK_STATE":
                actual = output["palette"][output["blocks"][0]["state"]]
            else:
                actual = output
                for key in path:
                    actual = actual[key]
            same_nbt(actual, expected, f"{name}.{codec}.vanilla_dfu")
            if name in {
                "historical-more-cat",
                "historical-more-painting",
                "historical-more-goat",
                "last-historical-factor",
            }:
                restored = decode(
                    converted, versions[fixture["source_data_version"]], allow_loss=True
                )["entities"][0]["nbt"]
                for key, value in original.items():
                    same_nbt(restored[key], value, f"{name}.{codec}.inverse.{key}")
        same_nbt(decode(source), snapshot, name)


def verify_biomes(data):
    root = nbtlib.parse_nbt("""{Schematic:{Version:3,DataVersion:3337,
        Width:2s,Height:1s,Length:1s,
        Blocks:{Palette:{"minecraft:stone":0},Data:[B;0b,0b]},
        Biomes:{Palette:{"minecraft:plains":0,"minecraft:forest":130},Data:[B;0b,-126b,1b]}
    }}""")
    stream = BytesIO()
    nbtlib.File(root).write(stream)
    source = Schematic.from_bytes(stream.getvalue(), format="schem", data=data)
    snapshot = nbtlib.File.parse(BytesIO(gzip.decompress(source.to_bytes(format="schem"))))
    output = nbtlib.File.parse(
        BytesIO(gzip.decompress(source.to_bytes(format="schem", version="1.21.1")))
    )
    same_nbt(output["Schematic"]["Biomes"], root["Schematic"]["Biomes"], "biomes")
    same_nbt(
        nbtlib.File.parse(BytesIO(gzip.decompress(source.to_bytes(format="schem")))),
        snapshot,
        "biomes.source_unchanged",
    )
    for values in ([0, 1], [0], [0, 0, 0], [0, -128], [0, -1, -1, -1, -1, 8]):
        root["Schematic"]["Biomes"]["Data"] = nbtlib.ByteArray(values)
        stream = BytesIO()
        nbtlib.File(root).write(stream)
        invalid = Schematic.from_bytes(stream.getvalue(), format="schem", data=data)
        assert invalid.check_export(format="schem", version="1.21.1").errors, values


def verify_opaque_entity(data):
    root = nbtlib.parse_nbt("""{
        DataVersion:3700,size:[1,1,1],palette:[{Name:"minecraft:stone"}],
        blocks:[{pos:[0,0,0],state:0}],
        entities:[{pos:[0.5d,0d,0.5d],blockPos:[0,0,0],nbt:{id:"minecraft:boat",Type:"oak",
        Inventory:[{id:"minecraft:scute",Count:1b}],Items:[{anything:1}],
        Attributes:[{Name:"custom",garbage:1}],ActiveEffects:[{custom:2}],
        Offers:{Recipes:[{opaque:1}]},BlockState:{Name:"custom:unknown"}}}]
    }""")
    source = load(root, data)
    output = decode(source, "1.21.1")["entities"][0]["nbt"]
    original = root["entities"][0]["nbt"]
    for field in ("Inventory", "Items", "Attributes", "ActiveEffects", "Offers", "BlockState"):
        same_nbt(output[field], original[field], f"opaque.{field}")


def verify_payloads(data, filename, source_version, target_version):
    fixtures = json.loads((Path(__file__).parent / "fixtures/conversion" / filename).read_text())[
        "cases"
    ]
    for fixture in fixtures:
        original = nbtlib.parse_nbt(fixture["input"])
        expected = nbtlib.parse_nbt(fixture["expected"])
        for key, value in fixture.get("expected_adjustments", {}).items():
            expected[key] = nbtlib.parse_nbt(value)
        for key in fixture.get("obsolete_fields", []):
            expected.pop(key, None)
        for key in fixture.get("obsolete_components", []):
            expected["components"].pop(key, None)
        for field in fixture.get("set_fields", []):
            value = expected
            for key in field:
                value = value[key]
            value.sort(key=str)
        item = fixture["type"] == "ITEM_STACK"
        entity = fixture["type"] == "ENTITY"
        if item:
            root = chest(original, fixture["source_data_version"])
            expected["Slot"] = nbtlib.Byte(0)
            path = ("blocks", 0, "nbt", "Items", 0)
        else:
            root = nbtlib.parse_nbt(
                """{DataVersion:3955,size:[1,1,1],palette:[{Name:"minecraft:chest"}],blocks:[{pos:[0,0,0],state:0}],entities:[]}"""
            )
            root["DataVersion"] = nbtlib.Int(fixture["source_data_version"])
            if entity:
                placement = nbtlib.parse_nbt("{pos:[0d,0d,0d],blockPos:[0,0,0]}")
                placement["nbt"] = original
                root["entities"] = nbtlib.List[nbtlib.Compound]([placement])
                expected["Pos"] = placement["pos"]
                path = ("entities", 0, "nbt")
            else:
                root["blocks"][0]["nbt"] = original
                path = ("blocks", 0, "nbt")

        def extract(root, path=path):
            for key in path:
                root = root[key]
            return root

        source = load(root, data)
        snapshot = decode(source)
        report = source.check_export(format="nbt", version=target_version)
        assert not report.errors, (fixture["name"], report.errors)
        assert bool(report.issues) == fixture["loss"], (fixture["name"], report.issues)
        for codec in CODECS:
            converted = export_load(source, target_version, codec, data, allow_loss=fixture["loss"])
            same_nbt(
                extract(decode(converted, allow_loss=True)), expected, fixture["name"] + "." + codec
            )
            restored = export_load(converted, source_version, codec, data, allow_loss=True)
            same_nbt(
                extract(decode(restored, target_version, allow_loss=True)),
                expected,
                fixture["name"] + ".inverse_forward",
            )
        same_nbt(decode(source), snapshot, fixture["name"] + ".source_unchanged")


def verify_block_defaults(data):
    for state in ('{Name:"minecraft:oak_log"}', '{Name:"minecraft:oak_log",Properties:{axis:"y"}}'):
        root = nbtlib.parse_nbt(
            "{DataVersion:3700,size:[1,1,1],palette:["
            + state
            + "],blocks:[{pos:[0,0,0],state:0}],entities:[]}"
        )
        source = load(root, data)
        output = decode(source, "1.21.1")
        actual = output["palette"][int(output["blocks"][0]["state"])]
        same_nbt(actual, root["palette"][0], "block.omitted_defaults")


def verify_block_routes(data):
    versions = ("1.13", "1.16.5", "1.17.1", "1.21.5", "1.21.11")
    root = nbtlib.parse_nbt(
        '{DataVersion:1519,size:[1,1,1],palette:[{Name:"minecraft:stone"}],blocks:[{pos:[0,0,0],state:0}],entities:[]}'
    )
    source = load(root, data)
    for version in versions:
        for codec in CODECS:
            target = export_load(source, version, codec, data)
            output = decode(target, "1.13", allow_loss=True)
            actual = output["palette"][int(output["blocks"][0]["state"])]
            same_nbt(actual, root["palette"][0], "block_only." + version + "." + codec)


def verify_owner_defaults(data):
    def entity(snbt, version):
        root = nbtlib.parse_nbt(
            '{DataVersion:3105,size:[1,1,1],palette:[{Name:"minecraft:stone"}],blocks:[{pos:[0,0,0],state:0}],entities:[{pos:[0d,0d,0d],blockPos:[0,0,0],nbt:{}}]}'
        )
        root["DataVersion"] = nbtlib.Int(version)
        root["entities"][0]["nbt"] = nbtlib.parse_nbt(snbt)
        return load(root, data)

    original = entity('{id:"minecraft:allay"}', 3105)
    output = decode(original, "1.19.2")["entities"][0]["nbt"]
    assert "CanDuplicate" not in output and "DuplicationCooldown" not in output
    inactive = entity('{id:"minecraft:allay",CanDuplicate:0b,DuplicationCooldown:0L}', 3120)
    output = decode(inactive, "1.19")["entities"][0]["nbt"]
    assert "CanDuplicate" not in output and "DuplicationCooldown" not in output
    for snbt in (
        '{id:"minecraft:allay",CanDuplicate:1b}',
        '{id:"minecraft:allay",DuplicationCooldown:20L}',
    ):
        assert entity(snbt, 3120).check_export(format="nbt", version="1.19").errors
    assert (
        not entity('{id:"minecraft:wolf"}', 3837)
        .check_export(format="nbt", version="1.20.4")
        .errors
    )
    wolf = entity(
        '{id:"minecraft:wolf",Owner:[I;1,2,3,4],Health:40f,Attributes:[{Name:"minecraft:generic.max_health",Base:40d},{Name:"minecraft:generic.attack_damage",Base:4d}]}',
        3837,
    )
    assert wolf.check_export(format="nbt", version="1.20.4").errors
    healthy = entity(
        '{id:"minecraft:wolf",Owner:[I;1,2,3,4],Health:20f,Attributes:[{Name:"minecraft:generic.max_health",Base:20d},{Name:"minecraft:generic.attack_damage",Base:4d}]}',
        3837,
    )
    output = decode(healthy, "1.20.4")["entities"][0]["nbt"]
    assert output["Health"] == 20 and output["Attributes"][0]["Base"] == 20
    modern = decode(load(decode(healthy, "1.20.4"), data), "1.20.5")["entities"][0]["nbt"]
    assert modern["Health"] == 20 and modern["Attributes"][0]["Base"] == 20
    injured = entity(
        '{id:"minecraft:wolf",Owner:[I;1,2,3,4],Health:10f,Attributes:[{Name:"minecraft:generic.max_health",Base:40d}]}',
        3837,
    )
    assert injured.check_export(format="nbt", version="1.20.4").errors


def verify_hover_text(data):
    tag = nbtlib.parse_nbt("""{Damage:3,display:{Name:'{"text":"Named"}'},bar:2s,foo:[B;1b,2b]}""")
    item_hover = {
        "action": "show_item",
        "contents": {"id": "minecraft:diamond_sword", "count": 1, "tag": tag.snbt()},
    }
    entity_hover = {
        "action": "show_entity",
        "contents": {
            "type": "minecraft:pig",
            "id": "12345678-1234-5678-1234-567812345678",
            "name": {"text": "Pig", "hoverEvent": item_hover},
        },
    }
    for name, hover in (("item", item_hover), ("entity", entity_hover)):
        item = nbtlib.parse_nbt('{id:"minecraft:stone",Count:1b,tag:{display:{}}}')
        component = {"text": "Hover", "hoverEvent": hover}
        item["tag"]["display"]["Name"] = nbtlib.String(json.dumps(component))
        source = load(chest(item, 3700), data)
        snapshot = decode(source)
        for target in ("1.15.2", "1.20.5"):
            for codec in CODECS:
                converted = export_load(source, target, codec, data)
                actual = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0]
                text = (
                    actual["components"]["minecraft:custom_name"]
                    if target == "1.20.5"
                    else actual["tag"]["display"]["Name"]
                )
                event = json.loads(text)["hoverEvent"]
                if target == "1.15.2":
                    assert "contents" not in event and "value" in event, name
                elif name == "item":
                    components = event["contents"]["components"]
                    assert components["minecraft:damage"] == 3
                    same_nbt(
                        nbtlib.parse_nbt(components["minecraft:custom_data"]),
                        nbtlib.parse_nbt("{bar:2s,foo:[B;1b,2b]}"),
                        f"hover.{name}.{codec}.custom_data",
                    )
                restored = decode(converted, "1.20.4", allow_loss=True)["blocks"][0]["nbt"][
                    "Items"
                ][0]
                event = json.loads(restored["tag"]["display"]["Name"])["hoverEvent"]
                if name == "entity":
                    assert event["contents"]["type"] == "minecraft:pig"
                    assert event["contents"]["id"] == entity_hover["contents"]["id"]
                    event = event["contents"]["name"]["hoverEvent"]
                assert event["contents"]["id"] == "minecraft:diamond_sword"
                same_nbt(nbtlib.parse_nbt(event["contents"]["tag"]), tag, f"hover.{name}.{codec}")
        same_nbt(decode(source), snapshot, f"hover.{name}.source_unchanged")
    item = nbtlib.parse_nbt('{id:"minecraft:stone",count:1,components:{}}')
    custom = {"small": 2, "medium": 256, "wide": 65536, "arr": [1, 256], "fraction": 0.5}
    component = {
        "text": "Hover",
        "hoverEvent": {
            "action": "show_item",
            "contents": {
                "id": "minecraft:stone",
                "count": 1,
                "components": {"minecraft:custom_data": custom},
            },
        },
    }
    item["components"]["minecraft:custom_name"] = nbtlib.String(json.dumps(component))
    source = load(chest(item, 3837), data)
    output = decode(source, "1.20.4")["blocks"][0]["nbt"]["Items"][0]
    event = json.loads(output["tag"]["display"]["Name"])["hoverEvent"]
    same_nbt(
        nbtlib.parse_nbt(event["contents"]["tag"]),
        nbtlib.parse_nbt('{small:2b,medium:256s,wide:65536,arr:[{"":1b},{"":256s}],fraction:0.5f}'),
        "hover.json_custom_data.runtime_codec",
    )


def verify_component_availability(data):
    for key, value in (
        ("minecraft:repairable", '{items:"minecraft:stone"}'),
        ("minecraft:item_model", '"minecraft:stone"'),
        ("!minecraft:glider", "{}"),
    ):
        item = nbtlib.parse_nbt(
            '{id:"minecraft:stone",count:1,components:{' + json.dumps(key) + ":" + value + "}}"
        )
        source = load(chest(item, 4082), data)
        assert source.check_export(format="nbt", version="1.21.1").errors, key
    invalid = load(chest(item, 3955), data)
    assert invalid.check_export(format="nbt", version="1.21.3").errors


def verify_litematic_ticks(data):
    root = nbtlib.parse_nbt("""{
        Version:5,MinecraftDataVersion:2730,Regions:{main:{
            Position:{x:0,y:0,z:0},Size:{x:1,y:1,z:1},
            BlockStatePalette:[{Name:"minecraft:stone"}],BlockStates:[L;0L],
            TileEntities:[],Entities:[],PendingBlockTicks:[
                {Block:"minecraft:stone",x:0,y:0,z:0,Time:10,Priority:0},
                {Block:"minecraft:dirt",x:0,y:0,z:0,Time:10,Priority:0}
            ],PendingFluidTicks:[{Fluid:"minecraft:water",x:0,y:0,z:0,Time:10,Priority:0}]
        }}
    }""")

    def load_litematic(root):
        stream = BytesIO()
        nbtlib.File(root).write(stream)
        return Schematic.from_bytes(stream.getvalue(), format="litematic", data=data)

    def export(schematic, version):
        return nbtlib.File.parse(
            BytesIO(gzip.decompress(schematic.to_bytes(format="litematic", version=version)))
        )

    source = load_litematic(root)
    modern = export(source, "1.18.2")
    assert modern["Version"] == 6 and modern["SubVersion"] == 1
    ticks = modern["Regions"]["main"]["PendingBlockTicks"]
    assert [tick["SubTick"] for tick in ticks] == [nbtlib.Long(0), nbtlib.Long(1)]
    restored = export(load_litematic(modern), "1.17")
    assert restored["Version"] == 5 and "SubVersion" not in restored
    for key in ("PendingBlockTicks", "PendingFluidTicks"):
        same_nbt(restored["Regions"]["main"][key], root["Regions"]["main"][key], key)
    ticks[0]["SubTick"] = nbtlib.Long(8)
    ticks[1]["SubTick"] = nbtlib.Long(2)
    reordered = export(load_litematic(modern), "1.17")["Regions"]["main"]["PendingBlockTicks"]
    assert [tick["Block"] for tick in reordered] == ["minecraft:dirt", "minecraft:stone"]
    assert all("SubTick" not in tick for tick in reordered)
    newest = export(source, "1.20.5")
    assert newest["Version"] == 7
    same_nbt(
        export(source, "1.17")["Regions"]["main"]["PendingBlockTicks"],
        root["Regions"]["main"]["PendingBlockTicks"],
        "ticks.source_unchanged",
    )


def verify_modern_owners(data):
    def block_entity(payload, version):
        root = nbtlib.parse_nbt(
            '{DataVersion:3955,size:[1,1,1],palette:[{Name:"minecraft:stone"}],'
            "blocks:[{pos:[0,0,0],state:0}],entities:[]}"
        )
        root["DataVersion"] = nbtlib.Int(version)
        root["palette"][0]["Name"] = payload["id"]
        root["blocks"][0]["nbt"] = payload
        return load(root, data)

    item = '{id:"minecraft:apple",count:1,components:{"minecraft:fire_resistant":{}}}'
    expected_item = nbtlib.parse_nbt(
        '{id:"minecraft:apple",count:1,components:{"minecraft:damage_resistant":{types:"#minecraft:is_fire"}}}'
    )
    payloads = {
        "vault": nbtlib.parse_nbt(
            '{id:"minecraft:vault",config:{key_item:'
            + item
            + "},shared_data:{display_item:"
            + item
            + "},server_data:{items_to_eject:["
            + item
            + "]}}"
        ),
        "crafter": nbtlib.parse_nbt('{id:"minecraft:crafter",Items:[' + item + "]}"),
        "trial_spawner": nbtlib.parse_nbt("""{id:"minecraft:trial_spawner",
            normal_config:{spawn_potentials:[{weight:1,data:{entity:{id:"minecraft:zombie",
                attributes:[{id:"minecraft:generic.max_health",base:20d}]}}}]},
            spawn_data:{entity:{id:"minecraft:zombie",attributes:[{id:"minecraft:generic.max_health",base:20d}]}}
        }"""),
    }
    payloads["crafter"]["Items"][0]["Slot"] = nbtlib.Byte(0)
    for name, payload in payloads.items():
        source = block_entity(payload, 3955)
        for codec in CODECS:
            converted = export_load(source, "1.21.3", codec, data)
            output = decode(converted, allow_loss=True)["blocks"][0]["nbt"]
            if name == "vault":
                for field, key in (("config", "key_item"), ("shared_data", "display_item")):
                    same_nbt(output[field][key], expected_item, f"{name}.{field}.{codec}")
                same_nbt(
                    output["server_data"]["items_to_eject"][0],
                    expected_item,
                    f"{name}.eject.{codec}",
                )
            elif name == "crafter":
                actual = output["Items"][0]
                assert actual.pop("Slot") == 0
                same_nbt(actual, expected_item, f"{name}.{codec}")
            else:
                assert (
                    output["spawn_data"]["entity"]["attributes"][0]["id"] == "minecraft:max_health"
                )
                assert (
                    output["normal_config"]["spawn_potentials"][0]["data"]["entity"]["attributes"][
                        0
                    ]["id"]
                    == "minecraft:max_health"
                )
            restored = decode(converted, "1.21.1", allow_loss=True)["blocks"][0]["nbt"]
            same_nbt(restored, payload, f"{name}.{codec}.inverse")
    configs = json.loads(
        (Path(__file__).parent.parent / "src/convert/data/trial-configs-1.21.3.json").read_text()
    )["configs"]
    for reference, snbt in configs.items():
        payload = nbtlib.Compound(
            {
                "id": nbtlib.String("minecraft:trial_spawner"),
                "normal_config": nbtlib.String(reference),
            }
        )
        source = block_entity(payload, 4082)
        report = source.check_export(format="nbt", version="1.21.1")
        assert report.issues and not report.errors, reference
        output = decode(source, "1.21.1", allow_loss=True)["blocks"][0]["nbt"]
        same_nbt(output["normal_config"], nbtlib.parse_nbt(snbt), reference)
        assert decode(source)["blocks"][0]["nbt"]["normal_config"] == reference
    payload["normal_config"] = nbtlib.String("custom:trial")
    assert block_entity(payload, 4082).check_export(format="nbt", version="1.21.1").errors


def verify_cloud_colors(data):
    def entity(payload, version):
        root = nbtlib.parse_nbt(
            '{DataVersion:3700,size:[1,1,1],palette:[{Name:"minecraft:stone"}],'
            "blocks:[{pos:[0,0,0],state:0}],entities:[{pos:[0d,0d,0d],blockPos:[0,0,0],nbt:{}}]}"
        )
        root["DataVersion"] = nbtlib.Int(version)
        root["entities"][0]["nbt"] = nbtlib.parse_nbt(payload)
        return load(root, data)

    source = entity('{id:"minecraft:area_effect_cloud"}', 3700)
    modern = decode(source, "1.20.5")["entities"][0]["nbt"]
    assert modern["Particle"]["color"] == -16777216
    for payload, color in (
        ('{id:"minecraft:area_effect_cloud"}', 16777215),
        (
            '{id:"minecraft:area_effect_cloud",Particle:{type:"minecraft:entity_effect",color:-16711165}}',
            66051,
        ),
    ):
        source = entity(payload, 3837)
        old = decode(source, "1.20.4")["entities"][0]["nbt"]
        assert old["Color"] == color
        restored = decode(load(decode(source, "1.20.4"), data), "1.20.5")["entities"][0]["nbt"]
        assert restored["potion_contents"]["custom_color"] == color
    transparent = entity(
        '{id:"minecraft:area_effect_cloud",Particle:{type:"minecraft:entity_effect",color:66051}}',
        3837,
    )
    assert transparent.check_export(format="nbt", version="1.20.4").errors
    overridden = entity(
        '{id:"minecraft:area_effect_cloud",Particle:{type:"minecraft:entity_effect",color:66051},potion_contents:{custom_color:123}}',
        3837,
    )
    assert decode(overridden, "1.20.4")["entities"][0]["nbt"]["Color"] == 123


def verify_empty_predicates(data):
    for field, component in (
        ("CanDestroy", "minecraft:can_break"),
        ("CanPlaceOn", "minecraft:can_place_on"),
    ):
        original = nbtlib.parse_nbt('{id:"minecraft:stone",Count:1b,tag:{' + field + ":[]}}")
        source = load(chest(original, 3700), data)
        for codec in CODECS:
            converted = export_load(source, "1.20.5", codec, data)
            value = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0][
                "components"
            ][component]
            same_nbt(value, nbtlib.parse_nbt("{predicates:[{blocks:[]}]}"), field + ".never_match")
            restored = decode(converted, "1.20.4", allow_loss=True)["blocks"][0]["nbt"]["Items"][0]
            same_nbt(restored, original, field + ".inverse")


def verify_component_names(data):
    original = nbtlib.parse_nbt('{id:"minecraft:apple",count:1,components:{fire_resistant:{}}}')
    output = decode(load(chest(original, 3955), data), "1.21.3")["blocks"][0]["nbt"]["Items"][0]
    assert output["components"] == nbtlib.parse_nbt(
        '{"minecraft:damage_resistant":{types:"#minecraft:is_fire"}}'
    )
    for components in ('{damage:1,"minecraft:damage":2}', '{damage:1,"!minecraft:damage":{}}'):
        item = nbtlib.parse_nbt('{id:"minecraft:stone",count:1,components:' + components + "}")
        assert load(chest(item, 3955), data).check_export(format="nbt", version="1.21.3").errors


def verify_modern_snbt(data):
    text = """{
        DataVersion:4325,size:[1,1,1],palette:[{Name:"minecraft:chest"}],
        blocks:[{pos:[0,0,0],state:0,nbt:{id:"minecraft:chest",Items:[{
            Slot:0b,id:"minecraft:stone",count:1,components:{
                "minecraft:custom_name":{translate:"example",extra:["hello",{text:"styled",bold:true}],
                with:["value",4,{text:"argument"}]},
                "minecraft:custom_data":{true:false,quoted:"true false",nested:[1b,"two"]}
            }
        }]}}],entities:[]
    }"""
    source = Schematic.from_bytes(text.encode(), format="snbt", data=data)
    root = decode(source)
    item = root["blocks"][0]["nbt"]["Items"][0]
    name = item["components"]["minecraft:custom_name"]
    assert name["extra"][0][""] == "hello"
    assert name["extra"][1]["bold"] == nbtlib.Byte(1)
    custom = item["components"]["minecraft:custom_data"]
    assert custom["true"] == nbtlib.Byte(0) and custom["quoted"] == "true false"
    output = source.to_bytes(format="snbt")
    assert b'"":' not in output
    restored = Schematic.from_bytes(output, format="snbt", data=data)
    same_nbt(decode(restored), root, "modern_snbt.mixed_lists")
    malformed = text.replace("count:1", "count:1 2")
    try:
        Schematic.from_bytes(malformed.encode(), format="snbt", data=data)
    except ValueError:
        pass
    else:
        raise AssertionError("Whitespace must not merge separate SNBT tokens")


def verify_profiles(data):
    for profile, expected in (
        ('"Builder"', '{Name:"Builder"}'),
        (
            '{name:"Builder",properties:{textures:["texture"]}}',
            '{Name:"Builder",Properties:{textures:[{Value:"texture"}]}}',
        ),
        (
            "{id:[305419896,305419896,305419896,305419896]}",
            "{Id:[I;305419896,305419896,305419896,305419896]}",
        ),
    ):
        item = nbtlib.parse_nbt(
            '{id:"minecraft:player_head",count:1,components:{"minecraft:profile":' + profile + "}}"
        )
        source = load(chest(item, 3955), data)
        for codec in CODECS:
            converted = export_load(source, "1.20.4", codec, data)
            actual = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0]["tag"][
                "SkullOwner"
            ]
            same_nbt(actual, nbtlib.parse_nbt(expected), "profile." + codec)
    item = nbtlib.parse_nbt(
        '{id:"minecraft:player_head",count:1,components:{"minecraft:profile":{id:"12345678-1234-5678-1234-567812345678"}}}'
    )
    assert load(chest(item, 3955), data).check_export(format="nbt", version="1.20.4").errors


def verify_text_1215(data):
    item = nbtlib.parse_nbt('{id:"minecraft:written_book",count:1,components:{}}')
    components = item["components"]
    rich = {"text": "Key", "bold": True}
    components["minecraft:custom_name"] = nbtlib.String(json.dumps(rich))
    components["minecraft:lock"] = nbtlib.Compound(
        {
            "components": nbtlib.Compound(
                {
                    "custom_name": nbtlib.String(json.dumps(rich)),
                    "custom_data": nbtlib.parse_nbt(
                        '{text:"opaque",nested:{id:"minecraft:grass"}}'
                    ),
                }
            )
        }
    )
    components["minecraft:lore"] = nbtlib.List[nbtlib.String](
        [
            nbtlib.String(json.dumps("Plain")),
            nbtlib.String(json.dumps(rich)),
        ]
    )
    components["minecraft:written_book_content"] = nbtlib.Compound(
        {
            "title": nbtlib.String("Notes"),
            "author": nbtlib.String("Builder"),
            "pages": nbtlib.List[nbtlib.String](
                [
                    nbtlib.String(json.dumps("Page")),
                    nbtlib.String(json.dumps(rich)),
                ]
            ),
        }
    )
    root = chest(item, 4189)
    root["blocks"][0]["nbt"]["lock"] = nbtlib.Compound(
        {
            "components": nbtlib.Compound(
                {
                    "minecraft:custom_name": nbtlib.String(json.dumps(rich)),
                }
            )
        }
    )
    source = load(root, data)
    expected_text = nbtlib.parse_nbt('{text:"Key",bold:1b}')
    for codec in CODECS:
        converted = export_load(source, "1.21.5", codec, data)
        output = decode(converted, allow_loss=True)["blocks"][0]["nbt"]
        actual = output["Items"][0]["components"]
        same_nbt(actual["minecraft:custom_name"], expected_text, "text1215.name")
        same_nbt(
            actual["minecraft:lock"]["components"]["minecraft:custom_name"],
            expected_text,
            "text1215.item_lock",
        )
        same_nbt(
            output["lock"]["components"]["minecraft:custom_name"],
            expected_text,
            "text1215.block_lock",
        )
        same_nbt(
            actual["minecraft:lock"]["components"]["minecraft:custom_data"],
            components["minecraft:lock"]["components"]["custom_data"],
            "text1215.opaque",
        )
        assert actual["minecraft:lore"][0][""] == "Plain"
        assert actual["minecraft:written_book_content"]["pages"][0]["raw"] == "Page"
        same_nbt(
            actual["minecraft:written_book_content"]["pages"][1]["raw"],
            expected_text,
            "text1215.book",
        )
        restored = decode(converted, "1.21.4", allow_loss=True)["blocks"][0]["nbt"]
        old = restored["Items"][0]["components"]
        assert json.loads(old["minecraft:custom_name"]) == rich
        assert json.loads(old["minecraft:written_book_content"]["pages"][0]["raw"]) == "Page"
        assert json.loads(old["minecraft:lore"][0]) == "Plain"
        assert json.loads(restored["lock"]["components"]["minecraft:custom_name"]) == rich
    lock = root["blocks"][0]["nbt"]["lock"]["components"]
    lock["minecraft:unbreakable"] = nbtlib.Compound({})
    assert load(root, data).check_export(format="nbt", version="1.21.5").errors


def verify_components_1216(data):
    modifier = '{type:"minecraft:max_health",id:"minecraft:test",amount:1d,operation:"add_value"}'
    for display in ('{type:"default"}', '{type:"hidden"}'):
        item = nbtlib.parse_nbt(
            '{id:"minecraft:stone",count:1,components:{"minecraft:attribute_modifiers":['
            + modifier
            + "]}}"
        )
        item["components"]["minecraft:attribute_modifiers"][0]["display"] = nbtlib.parse_nbt(
            display
        )
        source = load(chest(item, 4435), data)
        for codec in CODECS:
            converted = export_load(source, "1.21.5", codec, data)
            components = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0][
                "components"
            ]
            assert "display" not in components["minecraft:attribute_modifiers"][0]
            if "hidden" in display:
                assert (
                    "minecraft:attribute_modifiers"
                    in components["minecraft:tooltip_display"]["hidden_components"]
                )
            else:
                assert "minecraft:tooltip_display" not in components
    item = nbtlib.parse_nbt(
        '{id:"minecraft:stone",count:1,components:{"minecraft:equippable":{slot:"chest",can_be_sheared:0b,shearing_sound:"minecraft:item.shears.snip"}}}'
    )
    source = load(chest(item, 4435), data)
    output = decode(source, "1.21.5")["blocks"][0]["nbt"]["Items"][0]["components"]
    same_nbt(
        output["minecraft:equippable"],
        nbtlib.parse_nbt('{slot:"chest"}'),
        "equippable.disabled_shearing",
    )
    item["components"]["minecraft:equippable"]["can_be_sheared"] = nbtlib.Byte(1)
    assert load(chest(item, 4435), data).check_export(format="nbt", version="1.21.5").errors
    for component in (
        '{"minecraft:attribute_modifiers":['
        + modifier[:-1]
        + ',display:{type:"override",value:"custom"}}]}',
        '{"minecraft:attribute_modifiers":[{type:"minecraft:camera_distance",id:"minecraft:test",amount:1d,operation:"add_value"}]}',
        '{"minecraft:custom_name":{text:"click",click_event:{action:"custom",id:"custom:event"}}}',
    ):
        item = nbtlib.parse_nbt('{id:"minecraft:stone",count:1,components:' + component + "}")
        assert load(chest(item, 4435), data).check_export(format="nbt", version="1.21.5").errors
    item = nbtlib.parse_nbt(
        '{id:"minecraft:stone",count:1,components:{"minecraft:attribute_modifiers":[{type:"minecraft:camera_distance",id:"minecraft:test",amount:1d,operation:"add_value"}]}}'
    )
    assert load(chest(item, 4325), data).check_export(format="nbt", version="1.21.6").errors


def verify_clouds_1216(data):
    def cloud(payload, version):
        root = nbtlib.parse_nbt(
            '{DataVersion:4325,size:[1,1,1],palette:[{Name:"minecraft:stone"}],blocks:[{pos:[0,0,0],state:0}],entities:[{pos:[0d,0d,0d],blockPos:[0,0,0],nbt:{}}]}'
        )
        root["DataVersion"] = nbtlib.Int(version)
        root["entities"][0]["nbt"] = nbtlib.parse_nbt(payload)
        return load(root, data)

    for payload, color in (
        ('{id:"minecraft:area_effect_cloud"}', -16777216),
        ('{id:"minecraft:area_effect_cloud",potion_contents:{custom_effects:[]}}', -16777216),
        (
            '{id:"minecraft:area_effect_cloud",Particle:{type:"minecraft:entity_effect",color:123},potion_contents:{potion:"minecraft:swiftness"}}',
            3402751 - 16777216,
        ),
    ):
        source = cloud(payload, 4325)
        for codec in CODECS:
            converted = export_load(source, "1.21.6", codec, data)
            actual = decode(converted, allow_loss=True)["entities"][0]["nbt"]
            assert actual["custom_particle"]["color"] == color
            assert "Particle" not in actual
            restored = decode(converted, "1.21.5", allow_loss=True)["entities"][0]["nbt"]
            assert restored["Particle"]["color"] == color
            assert restored["potion_contents"]["custom_color"] == color & 0xFFFFFF
    source = cloud('{id:"minecraft:area_effect_cloud"}', 4435)
    old = decode(source, "1.21.5")["entities"][0]["nbt"]
    assert old["Particle"]["color"] == 0x385DC6 - 16777216
    assert old["potion_contents"]["custom_color"] == 0x385DC6
    source = cloud(
        '{id:"minecraft:area_effect_cloud",custom_particle:{type:"minecraft:entity_effect",color:123}}',
        4435,
    )
    assert source.check_export(format="nbt", version="1.21.5").errors


def verify_profiles_1219(data):
    for profile in (
        '"Builder"',
        "{id:[1,2,3,4]}",
        '{name:"Builder",properties:{textures:["texture"]}}',
    ):
        item = nbtlib.parse_nbt(
            '{id:"minecraft:player_head",count:1,components:{"minecraft:profile":'
            + profile
            + ',"minecraft:lock":{items:"minecraft:chain"}}}'
        )
        source = load(chest(item, 4440), data)
        for codec in CODECS:
            converted = export_load(source, "1.21.9", codec, data)
            actual = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0][
                "components"
            ]
            assert actual["minecraft:lock"]["items"] == "minecraft:iron_chain"
            assert isinstance(actual["minecraft:profile"], nbtlib.Compound)
            restored = decode(converted, "1.21.8", allow_loss=True)["blocks"][0]["nbt"]["Items"][0][
                "components"
            ]
            same_nbt(
                restored["minecraft:profile"], actual["minecraft:profile"], "profile1219.inverse"
            )
            assert restored["minecraft:lock"]["items"] == "minecraft:chain"
    for components in (
        '{"minecraft:profile":{name:"Builder",texture:"custom:skin"}}',
        '{"minecraft:custom_name":{object:"player",player:{name:"Builder"}}}',
    ):
        item = nbtlib.parse_nbt(
            '{id:"minecraft:player_head",count:1,components:' + components + "}"
        )
        assert load(chest(item, 4554), data).check_export(format="nbt", version="1.21.8").errors


def verify_animation_12111(data):
    item = nbtlib.parse_nbt(
        '{id:"minecraft:stone",count:1,components:{"minecraft:consumable":{animation:"spear"}}}'
    )
    source = load(chest(item, 4556), data)
    for codec in CODECS:
        converted = export_load(source, "1.21.11", codec, data)
        modern = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0]
        assert modern["components"]["minecraft:consumable"]["animation"] == "trident"
        old = decode(converted, "1.21.10", allow_loss=True)["blocks"][0]["nbt"]["Items"][0]
        assert old["components"]["minecraft:consumable"]["animation"] == "spear"
    assert load(chest(item, 4671), data).check_export(format="nbt", version="1.21.10").errors


def verify_components_26(data):
    tags = json.loads(
        (Path(__file__).parent.parent / "src/convert/data/registry-tags-1.21.11.json").read_text()
    )["registries"]
    item = nbtlib.parse_nbt(
        '{id:"minecraft:stone",count:1,components:{"minecraft:provides_banner_patterns":"minecraft:flower","minecraft:damage_resistant":{types:[]}}}'
    )
    item["components"]["minecraft:damage_resistant"]["types"] = nbtlib.List[nbtlib.String](
        tags["damage_type"]["minecraft:is_fire"]
    )
    source = load(chest(item, 4786), data)
    for codec in CODECS:
        converted = export_load(source, "1.21.11", codec, data, allow_loss=True)
        old = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0]["components"]
        assert old["minecraft:provides_banner_patterns"] == "#minecraft:pattern_item/flower"
        assert old["minecraft:damage_resistant"]["types"] == "#minecraft:is_fire"
        restored = decode(converted, "26.1", allow_loss=True)["blocks"][0]["nbt"]["Items"][0][
            "components"
        ]
        same_nbt(restored, old, "registry26.tag_inverse")
    for payload in (
        '{nbt:"Items[0].id",block:"~0.5 ~ ~"}',
        '{nbt:"Items[{Slot:0b}].id",block:"~ ~ ~"}',
        '{selector:"@e[type=minecraft:pig]ignored"}',
    ):
        item = nbtlib.parse_nbt(
            '{id:"minecraft:stone",count:1,components:{"minecraft:custom_name":' + payload + "}}"
        )
        source = load(chest(item, 4671), data)
        expected = nbtlib.parse_nbt(payload)
        if "selector" in expected:
            expected["selector"] = nbtlib.String("@e[type=minecraft:pig]")
        for codec in CODECS:
            converted = export_load(source, "26.1", codec, data, allow_loss=True)
            name = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0]["components"][
                "minecraft:custom_name"
            ]
            same_nbt(name, expected, "text26." + codec)
    for payload in ('{nbt:"Items junk",block:"~ ~ ~"}', '{nbt:"Items",block:"0.5 2 3"}'):
        item = nbtlib.parse_nbt(
            '{id:"minecraft:stone",count:1,components:{"minecraft:custom_name":' + payload + "}}"
        )
        assert load(chest(item, 4671), data).check_export(format="nbt", version="26.1").errors
    item = nbtlib.parse_nbt(
        '{id:"minecraft:stone",count:1,components:{"minecraft:damage_resistant":{types:["minecraft:lava"]}}}'
    )
    assert load(chest(item, 4786), data).check_export(format="nbt", version="1.21.11").errors


def verify_spawn_egg_variants(data):
    cases = (
        ("wolf", "wolf/variant", '"minecraft:striped"', "variant", '"minecraft:striped"'),
        ("wolf", "wolf/collar", '"yellow"', "CollarColor", "4b"),
        ("axolotl", "axolotl/variant", '"gold"', "Variant", "2"),
        ("salmon", "salmon/size", '"large"', "type", '"large"'),
        ("cat", "cat/variant", '"minecraft:red"', "variant", '"minecraft:red"'),
        ("cat", "cat/collar", '"red"', "CollarColor", "14b"),
        ("frog", "frog/variant", '"minecraft:warm"', "variant", '"minecraft:warm"'),
        ("rabbit", "rabbit/variant", '"evil"', "RabbitType", "99"),
        ("parrot", "parrot/variant", '"green"', "Variant", "2"),
        ("llama", "llama/variant", '"gray"', "Variant", "3"),
        ("trader_llama", "llama/variant", '"creamy"', "Variant", "0"),
        ("sheep", "sheep/color", '"light_blue"', "Color", "3b"),
        ("shulker", "shulker/color", '"yellow"', "Color", "4b"),
        ("fox", "fox/variant", '"snow"', "Type", '"snow"'),
        ("mooshroom", "mooshroom/variant", '"brown"', "Type", '"brown"'),
    )
    for owner, component, value, field, expected in cases:
        item = nbtlib.parse_nbt(
            '{id:"minecraft:'
            + owner
            + '_spawn_egg",count:1,components:{"minecraft:'
            + component
            + '":'
            + value
            + "}}"
        )
        source = load(chest(item, 4325), data)
        for codec in CODECS:
            converted = export_load(source, "1.21.4", codec, data)
            components = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0][
                "components"
            ]
            assert "minecraft:" + component not in components
            payload = components["minecraft:entity_data"]
            assert payload["id"] == "minecraft:" + owner
            same_nbt(payload[field], nbtlib.parse_nbt(expected), component + "." + codec)
            restored = decode(converted, "1.21.5", allow_loss=True)["blocks"][0]["nbt"]["Items"][0][
                "components"
            ]
            same_nbt(restored["minecraft:entity_data"], payload, component + ".inverse_forward")

    for owner in ("pig", "cow", "chicken"):
        item = nbtlib.parse_nbt(
            '{id:"minecraft:'
            + owner
            + '_spawn_egg",count:1,components:{"minecraft:'
            + owner
            + '/variant":"minecraft:temperate"}}'
        )
        source = load(chest(item, 4325), data)
        payload = decode(source, "1.21.4")["blocks"][0]["nbt"]["Items"][0]["components"][
            "minecraft:entity_data"
        ]
        assert payload == {"id": "minecraft:" + owner}
        item["components"]["minecraft:" + owner + "/variant"] = nbtlib.String("minecraft:warm")
        assert load(chest(item, 4325), data).check_export(format="nbt", version="1.21.4").errors
    item = nbtlib.parse_nbt(
        '{id:"minecraft:tropical_fish_spawn_egg",count:1,components:{"minecraft:tropical_fish/pattern":"sunstreak","minecraft:tropical_fish/base_color":"magenta","minecraft:tropical_fish/pattern_color":"light_blue"}}'
    )
    source = load(chest(item, 4325), data)
    for codec in CODECS:
        converted = export_load(source, "1.21.4", codec, data)
        payload = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0]["components"][
            "minecraft:entity_data"
        ]
        assert payload["Variant"] == 50462976


def verify_components_263(data):
    verify_payloads(data, "components-26.3.json", "26.2", "26.3")
    for patch in (
        '{"!minecraft:swing_animation":{}}',
        '{"minecraft:provides_trim_material":{asset_name:"quartz",description:"Quartz"}}',
    ):
        item = nbtlib.parse_nbt('{id:"minecraft:stone",count:1,components:' + patch + "}")
        source = load(chest(item, 4903), data)
        for codec in CODECS:
            converted = export_load(source, "26.3", codec, data)
            components = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0][
                "components"
            ]
            if "swing_animation" in patch:
                assert "!minecraft:attack_animation" in components
                assert "!minecraft:interact_animation" in components
            else:
                assert (
                    components["minecraft:provides_trim_material"]["palette_id"]
                    == "minecraft:trim/quartz"
                )
            restored = decode(converted, "26.2", allow_loss=True)["blocks"][0]["nbt"]["Items"][0][
                "components"
            ]
            same_nbt(restored, item["components"], "components263.inverse." + codec)
    item = nbtlib.parse_nbt(
        '{id:"minecraft:oak_sign",count:1,components:{"minecraft:sign_text_front":{messages:["a","b","c","d"]},"minecraft:waxed":{}}}'
    )
    source = load(chest(item, 5023), data)
    for codec in CODECS:
        converted = export_load(source, "26.2", codec, data)
        block = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0]["components"][
            "minecraft:block_entity_data"
        ]
        assert block["id"] == "minecraft:sign" and block["is_waxed"] == 1
        assert block["front_text"]["messages"] == ["a", "b", "c", "d"]
    for patch in (
        '{"minecraft:attack_animation":{type:"stab",duration:12}}',
        '{"minecraft:pot_decorations":{back:{id:"minecraft:brick",count:2},left:"minecraft:brick",right:"minecraft:brick",front:"minecraft:brick"}}',
        '{"minecraft:instrument":{sound_event:"minecraft:item.goat_horn.sound.0",use_duration:7f,range:256f,description:"Horn",durability_damage:1}}',
        '{"minecraft:consumable":{on_consume_effects:[{type:"minecraft:teleport_randomly",directional_particles:1b}]}}',
    ):
        item = nbtlib.parse_nbt('{id:"minecraft:stone",count:1,components:' + patch + "}")
        assert load(chest(item, 5023), data).check_export(format="nbt", version="26.2").errors


def verify_latest_owners(data):
    item = nbtlib.parse_nbt('{id:"minecraft:stone",count:1}')
    source = load(chest(item, 4903), data)
    for codec in CODECS:
        converted = export_load(source, "26.3", codec, data)
        palette = decode(converted, allow_loss=True)["palette"]
        assert all("id" in state and "Name" not in state for state in palette)
        palette = decode(converted, "26.2", allow_loss=True)["palette"]
        assert all("Name" in state and "id" not in state for state in palette)
    for payload in (
        '{id:"minecraft:furnace",cooking_time_spent:40000}',
        '{id:"minecraft:brewing_stand",Fuel:128}',
        '{id:"minecraft:sign",allow_op_features:0b,front_text:{messages:[{text:"command",click_event:{action:"run_command",command:"say hi"}},{text:""},{text:""},{text:""}]}}',
    ):
        root = chest(nbtlib.parse_nbt('{id:"minecraft:stone",count:1}'), 5023)
        root["blocks"][0]["nbt"] = nbtlib.parse_nbt(payload)
        assert load(root, data).check_export(format="nbt", version="26.2").errors
    for payload, source_version, target, field, expected in (
        (
            '{id:"minecraft:player",respawn:{pos:[I;1,70,2],angle:90f}}',
            4440,
            "1.21.9",
            "respawn",
            '{pos:[I;1,70,2],yaw:90f,pitch:0f,dimension:"minecraft:overworld"}',
        ),
        (
            '{id:"minecraft:area_effect_cloud",custom_particle:{type:"minecraft:block",block_state:"minecraft:oak_log[axis=x]"}}',
            5023,
            "26.2",
            "custom_particle",
            '{type:"minecraft:block",block_state:{Name:"minecraft:oak_log",Properties:{axis:"x"}}}',
        ),
        (
            '{id:"minecraft:area_effect_cloud",custom_particle:{type:"minecraft:geyser",water_blocks:4}}',
            4903,
            "26.3",
            "custom_particle",
            '{type:"minecraft:geyser",water_blocks:4}',
        ),
        (
            '{id:"minecraft:pig",ticks_since_last_hurt_by_mob:-12}',
            4790,
            "26.2",
            "ticks_since_last_hurt_by_mob",
            "12",
        ),
        (
            '{id:"minecraft:falling_block",BlockState:{Name:"minecraft:oak_log",Properties:{axis:"x"}}}',
            4903,
            "26.3",
            "BlockState",
            '{id:"minecraft:oak_log",properties:{axis:"x"}}',
        ),
    ):
        root = chest(nbtlib.parse_nbt('{id:"minecraft:stone",count:1}'), source_version)
        entity = nbtlib.parse_nbt("{pos:[0d,0d,0d],blockPos:[0,0,0]}")
        entity["nbt"] = nbtlib.parse_nbt(payload)
        root["entities"] = nbtlib.List[nbtlib.Compound]([entity])
        source = load(root, data)
        for codec in CODECS:
            converted = export_load(source, target, codec, data)
            value = decode(converted, allow_loss=True)["entities"][0]["nbt"][field]
            same_nbt(value, nbtlib.parse_nbt(expected), "latest_owner." + field + "." + codec)

    for field in ("yaw", "pitch"):
        root = chest(nbtlib.parse_nbt('{id:"minecraft:stone",count:1}'), 4440)
        placement = nbtlib.parse_nbt("{pos:[0d,0d,0d],blockPos:[0,0,0]}")
        placement["nbt"] = nbtlib.parse_nbt(
            '{id:"minecraft:player",respawn:{pos:[I;1,70,2],' + field + ":20f}}"
        )
        root["entities"] = nbtlib.List[nbtlib.Compound]([placement])
        assert load(root, data).check_export(format="nbt", version="1.21.9").errors


def verify_nested_owners(data):
    root = nbtlib.parse_nbt("""{
      DataVersion:3700,size:[3,1,1],
      palette:[{Name:"minecraft:chest"},{Name:"minecraft:beehive"},{Name:"minecraft:white_banner"}],
      blocks:[
        {pos:[0,0,0],state:0,nbt:{id:"minecraft:chest",Items:[
          {Slot:0b,id:"minecraft:diamond_sword",Count:1b,tag:{Damage:7,Enchantments:[{id:"minecraft:sweeping",lvl:2s}],display:{Name:'{"text":"Blade"}'},marker:{id:"minecraft:scute",Count:2b}}},
          {Slot:1b,id:"minecraft:shulker_box",Count:1b,tag:{BlockEntityTag:{Items:[{Slot:3b,id:"minecraft:scute",Count:4b}]}}}
        ]}},
        {pos:[1,0,0],state:1,nbt:{id:"minecraft:beehive",FlowerPos:{X:3,Y:4,Z:5},Bees:[{EntityData:{id:"minecraft:bee",HivePos:{X:1,Y:2,Z:3}},TicksInHive:4,MinOccupationTicks:20}]}},
        {pos:[2,0,0],state:2,nbt:{id:"minecraft:banner",Patterns:[{Pattern:"bs",Color:14}]}}
      ],
      entities:[{pos:[0.5d,0.0d,0.5d],blockPos:[0,0,0],nbt:{id:"minecraft:zombie",Pos:[0.5d,0.0d,0.5d],HandItems:[{id:"minecraft:scute",Count:1b},{}],Passengers:[{id:"minecraft:pig",Pos:[0.5d,0.0d,0.5d]}]}}]
    }""")
    source = load(root, data)
    original = decode(source)
    for codec in CODECS:
        converted = export_load(source, "1.20.5", codec, data)
        output = decode(converted, allow_loss=True)
        hive = output["blocks"][1]["nbt"]
        assert list(hive["flower_pos"]) == [3, 4, 5]
        assert list(hive["bees"][0]["entity_data"]["hive_pos"]) == [1, 2, 3]
        assert hive["bees"][0]["min_ticks_in_hive"] == 20
        assert output["blocks"][2]["nbt"]["patterns"][0]["pattern"] == "minecraft:stripe_bottom"
        zombie = output["entities"][0]["nbt"]
        assert zombie["HandItems"][0]["id"] == "minecraft:turtle_scute"
        assert zombie["Passengers"][0]["id"] == "minecraft:pig"
        assert (
            output["blocks"][0]["nbt"]["Items"][1]["components"]["minecraft:container"][0]["item"][
                "id"
            ]
            == "minecraft:turtle_scute"
        )
    same_nbt(decode(source), original, "nested_owners.source_unchanged")


def verify_review_regressions(data):
    cases = (
        (
            "wall",
            2230,
            "1.16",
            '{id:"minecraft:cobblestone_wall",Count:1b,tag:{BlockStateTag:{north:"true"}}}',
            ("tag", "BlockStateTag"),
            '{north:"low"}',
        ),
        (
            "heart",
            4189,
            "1.21.5",
            '{id:"minecraft:creaking_heart",count:1,components:{"minecraft:block_state":{active:"true"}}}',
            ("components", "minecraft:block_state"),
            '{creaking_heart_state:"awake"}',
        ),
        (
            "legacy_predicate",
            3465,
            "1.20.3",
            '{id:"minecraft:diamond_pickaxe",Count:1b,tag:{CanDestroy:["minecraft:grass"]}}',
            ("tag", "CanDestroy"),
            '["minecraft:short_grass"]',
        ),
        (
            "empty_cauldron_predicate",
            2724,
            "1.16.5",
            '{id:"minecraft:diamond_pickaxe",Count:1b,tag:{CanDestroy:["minecraft:cauldron"]}}',
            ("tag", "CanDestroy"),
            '["minecraft:cauldron[level=0]"]',
        ),
        (
            "predicate_wall",
            2230,
            "1.16",
            '{id:"minecraft:diamond_pickaxe",Count:1b,tag:{CanDestroy:["minecraft:cobblestone_wall[north=true]"]}}',
            ("tag", "CanDestroy"),
            '["minecraft:cobblestone_wall[north=low]"]',
        ),
        (
            "route",
            4082,
            "1.21.2",
            '{id:"minecraft:stone",count:1,components:{"minecraft:equippable":{slot:"head"}}}',
            ("components", "minecraft:equippable"),
            '{slot:"head"}',
        ),
        (
            "sign_identity",
            5023,
            "26.2",
            '{id:"minecraft:oak_sign",count:1,components:{"minecraft:sign_text_front":{messages:["a","b","c","d"]},"minecraft:block_entity_data":{id:"minecraft:sign",opaque:42}}}',
            ("components", "minecraft:block_entity_data"),
            '{id:"minecraft:sign",opaque:42,front_text:{messages:["a","b","c","d"]}}',
        ),
    )
    for name, version, target, payload, path, expected in cases:
        item = nbtlib.parse_nbt(payload)
        source = load(chest(item, version), data)
        snapshot = decode(source)
        for codec in CODECS:
            converted = export_load(source, target, codec, data)
            actual = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0]
            for key in path:
                actual = actual[key]
            same_nbt(actual, nbtlib.parse_nbt(expected), name + "." + codec)
        same_nbt(decode(source), snapshot, name + ".source_unchanged")
    nested = nbtlib.parse_nbt(
        """{id:"minecraft:diamond_pickaxe",Count:1b,tag:{CanDestroy:['minecraft:chest{Items:[{id:"minecraft:stone",Count:1b}]}']}}"""
    )
    source = load(chest(nested, 3700), data)
    for codec in CODECS:
        assert source.check_export(format=codec, version="1.20.5").errors
    water = nbtlib.parse_nbt(
        '{id:"minecraft:diamond_pickaxe",Count:1b,tag:{CanDestroy:["minecraft:water_cauldron"]}}'
    )
    source = load(chest(water, 2724), data)
    for codec in CODECS:
        assert source.check_export(format=codec, version="1.16.5").errors
    for source_version, target, component in (
        (3837, "1.20.4", '{"minecraft:potion_contents":{potion:"minecraft:wind_charged"}}'),
        (
            3953,
            "1.20.6",
            '{"minecraft:can_break":{predicates:[{blocks:"minecraft:trial_spawner"}]}}',
        ),
        (
            4671,
            "1.21.10",
            '{"minecraft:potion_contents":{custom_effects:[{id:"minecraft:breath_of_the_nautilus",duration:100}]}}',
        ),
        (
            4671,
            "1.21.10",
            '{"minecraft:potion_contents":{custom_effects:[{id:"custom:unknown",duration:100}]}}',
        ),
    ):
        item = nbtlib.parse_nbt('{id:"minecraft:stone",count:1,components:' + component + "}")
        source = load(chest(item, source_version), data)
        for codec in CODECS:
            assert source.check_export(format=codec, version=target).errors, component
            try:
                source.to_bytes(format=codec, version=target, allow_loss=True)
            except ValueError:
                pass
            else:
                raise AssertionError("allow_loss bypassed unsupported typed payload")


def verify_potion_references(data):
    potions = json.loads(
        (Path(__file__).parent.parent / "src/convert/data/potions-1.21.5.json").read_text()
    )["potions"]
    new = {"minecraft:wind_charged", "minecraft:weaving", "minecraft:oozing", "minecraft:infested"}
    for identifier in potions:
        item = nbtlib.parse_nbt(
            '{id:"minecraft:potion",count:1,components:{"minecraft:potion_contents":{potion:"'
            + identifier
            + '"}}}'
        )
        source = load(chest(item, 3837), data)
        original = decode(source)
        for codec in CODECS:
            if identifier in new:
                assert source.check_export(format=codec, version="1.13").errors, identifier
            else:
                converted = export_load(source, "1.13", codec, data)
                value = decode(converted, allow_loss=True)["blocks"][0]["nbt"]["Items"][0]
                assert value["tag"]["Potion"] == identifier
                restored = export_load(converted, "1.20.5", codec, data, allow_loss=True)
                same_nbt(
                    decode(restored, allow_loss=True)["blocks"][0]["nbt"]["Items"][0],
                    item,
                    "potion_registry.inverse",
                )
        same_nbt(decode(source), original, "potion_registry.source_unchanged")
    print(
        f"Potion references: all {len(potions)} named potions across four codecs passed", flush=True
    )
    for version, target in ((3700, "1.20.5"), (3837, "1.20.4")):
        for potion in ("minecraft:wind_charged", "custom:unknown"):
            item = nbtlib.parse_nbt(
                '{id:"minecraft:potion",Count:1b,tag:{Potion:"' + potion + '"}}'
                if version < 3837
                else '{id:"minecraft:potion",count:1,components:{"minecraft:potion_contents":{potion:"'
                + potion
                + '"}}}'
            )
            roots = [chest(item, version)]
            for owner in ("arrow", "area_effect_cloud"):
                root = chest(
                    nbtlib.parse_nbt(
                        '{id:"minecraft:stone",'
                        + ("Count:1b" if version < 3837 else "count:1")
                        + "}"
                    ),
                    version,
                )
                placement = nbtlib.parse_nbt("{pos:[0d,0d,0d],blockPos:[0,0,0]}")
                placement["nbt"] = nbtlib.parse_nbt(
                    '{id:"minecraft:' + owner + '",Potion:"' + potion + '"}'
                )
                root["entities"] = nbtlib.List[nbtlib.Compound]([placement])
                roots.append(root)
                if version >= 3837:
                    modern = deepcopy(root)
                    owner_data = modern["entities"][0]["nbt"]
                    owner_data.pop("Potion")
                    content = nbtlib.Compound({"potion": nbtlib.String(potion)})
                    if owner == "area_effect_cloud":
                        owner_data["potion_contents"] = content
                    else:
                        owner_data["item"] = nbtlib.Compound(
                            {
                                "id": nbtlib.String("minecraft:tipped_arrow"),
                                "count": nbtlib.Int(1),
                                "components": nbtlib.Compound(
                                    {"minecraft:potion_contents": content}
                                ),
                            }
                        )
                    roots.append(modern)
            for root in roots:
                source = load(root, data)
                original = decode(source)
                for codec in CODECS:
                    assert source.check_export(format=codec, version=target).errors, (
                        version,
                        potion,
                        codec,
                    )
                    try:
                        source.to_bytes(format=codec, version=target, allow_loss=True)
                    except ValueError:
                        pass
                    else:
                        raise AssertionError("unavailable potion exported")
                same_nbt(decode(source), original, "potion.source_unchanged")


def verify_release_routes(data):
    releases = json.loads(
        (Path(__file__).parent.parent / "src/convert/data/releases.json").read_text()
    )
    marker = nbtlib.parse_nbt('{id:"minecraft:grass",count:3b,n:[I;1,-2,3],wide:1234567890123L}')
    checked = 0
    for release in releases:
        version = release["data_version"]
        item = nbtlib.parse_nbt(
            '{id:"minecraft:stone",' + ("count:1" if version >= 3837 else "Count:1b") + "}"
        )
        if version >= 3837:
            item["components"] = nbtlib.Compound({"minecraft:custom_data": marker})
        else:
            item["tag"] = marker
        source = load(chest(item, version), data)
        original = decode(source)
        for target in releases:
            for codec in CODECS:
                converted = export_load(source, target["version"], codec, data)
                output = decode(converted, allow_loss=True)
                assert output["DataVersion"] == target["data_version"], (
                    release["version"],
                    target["version"],
                    codec,
                )
                actual = output["blocks"][0]["nbt"]["Items"][0]
                assert (
                    actual["id"] == "minecraft:stone"
                    and actual["count" if target["data_version"] >= 3837 else "Count"] == 1
                )
                custom = (
                    actual["components"]["minecraft:custom_data"]
                    if target["data_version"] >= 3837
                    else actual["tag"]
                )
                same_nbt(custom, marker, "route.custom_data")
                restored = export_load(converted, release["version"], codec, data, allow_loss=True)
                restored_item = decode(restored, allow_loss=True)["blocks"][0]["nbt"]["Items"][0]
                same_nbt(restored_item, item, "route.inverse")
                checked += 1
        same_nbt(decode(source), original, "route.source_unchanged")
        print(
            f"Release pairs: {release['version']} to all {len(releases)} targets passed", flush=True
        )
    print(
        f"Release pairs: {len(releases) ** 2} ordered pairs, {checked} format routes and their inverses passed"
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cache-dir", type=Path)
    args = parser.parse_args()
    data = MinecraftData(cache_dir=args.cache_dir, offline=True)
    verify_items(data)
    verify_nested_owners(data)
    verify_review_regressions(data)
    verify_potion_references(data)
    verify_failures(data)
    verify_historical(data)
    verify_attributes(data)
    verify_biomes(data)
    verify_earlier_routes(data)
    verify_opaque_entity(data)
    verify_payloads(data, "modern-1.21.3.json", "1.21.1", "1.21.3")
    verify_payloads(data, "particles-1.20.5.json", "1.20.4", "1.20.5")
    verify_payloads(data, "modern-1.21.4.json", "1.21.3", "1.21.4")
    verify_payloads(data, "tooltips-1.21.5.json", "1.21.4", "1.21.5")
    verify_payloads(data, "item-variants-1.21.5.json", "1.21.4", "1.21.5")
    verify_payloads(data, "entities-1.21.5.json", "1.21.4", "1.21.5")
    verify_block_defaults(data)
    verify_block_routes(data)
    verify_owner_defaults(data)
    verify_hover_text(data)
    verify_component_availability(data)
    verify_litematic_ticks(data)
    verify_modern_owners(data)
    verify_cloud_colors(data)
    verify_empty_predicates(data)
    verify_component_names(data)
    verify_modern_snbt(data)
    verify_profiles(data)
    verify_text_1215(data)
    verify_components_1216(data)
    verify_clouds_1216(data)
    verify_profiles_1219(data)
    verify_animation_12111(data)
    verify_components_26(data)
    verify_components_263(data)
    verify_payloads(data, "owners-26.3.json", "26.2", "26.3")
    verify_spawn_egg_variants(data)
    verify_latest_owners(data)
    verify_release_routes(data)
    print(
        "Conversion corpus: 11 positive item, 40 historical, five attribute, 87 modern and eight particle cases across four Java codecs; hover text, owner defaults, trial configs, tick ordering, mixed SNBT, predicates, failure cases and biome palettes passed"
    )


if __name__ == "__main__":
    main()
