import argparse
import io
import json
import os
import subprocess
import tempfile
import zipfile
from itertools import product
from pathlib import Path

from import_recipes import (
    MANIFEST,
    SNAPSHOTS,
    download,
    recipe_catalog,
    resources,
    server_runtime,
    write_recipes,
    write_snapshots,
)


def entries(report):
    return [
        {"name": name, "id": value["protocol_id"]}
        for name, value in sorted(report["entries"].items())
    ]


def blocks(report, registry):
    result = []
    for entry in entries(registry):
        name = entry["name"]
        block = report[name]
        properties = dict(sorted(block.get("properties", {}).items()))
        states = sorted(block["states"], key=lambda state: state["id"])
        combinations = list(product(*properties.values()))
        assert len(states) == len(combinations), name
        for offset, (state, values) in enumerate(zip(states, combinations, strict=True)):
            assert state["id"] == states[0]["id"] + offset, name
            assert state.get("properties", {}) == dict(zip(properties, values, strict=True)), name
        defaults = [state["id"] for state in states if state.get("default", False)]
        assert len(defaults) == 1, name
        result.append(
            entry
            | {
                "displayName": name.split(":", 1)[1],
                "minStateId": states[0]["id"],
                "defaultState": defaults[0],
                "states": [{"name": key, "values": values} for key, values in properties.items()],
            }
        )
    return result


def generate(version, manifest, args, helper):
    metadata, content, server_hash, runtime_hash = server_runtime(version, manifest, args.cache_dir)
    with zipfile.ZipFile(io.BytesIO(content)) as runtime:
        version_data = json.loads(runtime.read("version.json"))
        biomes = resources(runtime, [("worldgen", "biome")])
    aliases = {}
    if "server_mappings" in metadata["downloads"]:
        mappings = metadata["downloads"]["server_mappings"]
        text = download(
            mappings["url"], args.cache_dir / f"{version}-mappings.txt", mappings["sha1"]
        ).decode()
        aliases = {
            line.split(" -> ")[0]: line.split(" -> ")[1][:-1]
            for line in text.splitlines()
            if line and not line.startswith((" ", "#")) and " -> " in line
        }
    entity = aliases.get(
        "net.minecraft.world.entity.EntityType", "net.minecraft.world.entity.EntityType"
    )
    living = aliases.get(
        "net.minecraft.world.entity.LivingEntity", "net.minecraft.world.entity.LivingEntity"
    )
    holder = aliases.get(
        "net.minecraft.core.registries.BuiltInRegistries",
        aliases.get(
            "net.minecraft.core.Registry", "net.minecraft.core.registries.BuiltInRegistries"
        ),
    )
    if version == "1.14.2":
        entity, living, holder = "aim", "air", "fm"
    work = args.cache_dir / "reports" / version
    work.mkdir(parents=True, exist_ok=True)
    classpath = [str(helper)]
    with zipfile.ZipFile(args.cache_dir / f"{version}.jar") as archive:
        jars = [
            name
            for name in archive.namelist()
            if name.startswith(("META-INF/libraries/", "META-INF/versions/"))
            and name.endswith(".jar")
        ]
        for name in jars:
            path = work / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(archive.read(name))
            classpath.append(str(path))
        if not jars:
            classpath.append(str(args.cache_dir / f"{version}.jar"))
    command = [
        args.java,
        "-Xmx1G",
        "-cp",
        os.pathsep.join(classpath),
        "schemora.tools.SnapshotRuntime",
        str(work / "entities.json"),
        entity,
        living,
        holder,
        "--reports",
        "--output",
        str(work / "generated"),
    ]
    if not biomes:
        command.append("--server")
    print(f"{version}: generating official registry reports", flush=True)
    with (work / "generation.log").open("w") as log:
        result = subprocess.run(
            command, cwd=work, stdout=log, stderr=subprocess.STDOUT, timeout=180
        )
    if result.returncode:
        raise RuntimeError(f"Data generation failed for {version}; see {work / 'generation.log'}")
    output = work / "generated"
    registries = json.loads((output / "reports/registries.json").read_text())
    report = json.loads((output / "reports/blocks.json").read_text())
    entity_classes = json.loads((work / "entities.json").read_text())
    entities = entries(registries["minecraft:entity_type"])
    assert {entry["name"] for entry in entities} == set(entity_classes)
    entities = [entry | entity_classes[entry["name"]] for entry in entities]
    if not biomes:
        if "minecraft:biome" in registries:
            biomes = registries["minecraft:biome"]["entries"]
        else:
            biomes = {
                "minecraft:" + path.relative_to(base).with_suffix("").as_posix(): None
                for base in [output / "reports/minecraft/worldgen/biome", output / "reports/biomes"]
                for path in base.rglob("*.json")
            }
    assert biomes, version
    snapshot = {
        "version": version,
        "data_version": version_data["world_version"],
        "provenance": {
            "official_server_sha256": server_hash,
            "method": "official data-generator --reports; EntityType generic classes checked against LivingEntity; vanilla biome registry/resources",
        },
        "datasets": {
            "blocks": blocks(report, registries["minecraft:block"]),
            "items": entries(registries["minecraft:item"]),
            "entities": entities,
            "biomes": [{"name": name} for name in sorted(biomes)],
        },
    }
    items = {item["name"]: item["id"] for item in snapshot["datasets"]["items"]}
    recipes = recipe_catalog(version, items, metadata, content, server_hash, runtime_hash)
    return snapshot, recipes


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("versions", nargs="*")
    parser.add_argument(
        "--cache-dir", type=Path, default=Path(tempfile.gettempdir()) / "schemora-recipe-sources"
    )
    parser.add_argument("--java", default="java")
    parser.add_argument("--javac", default="javac")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    args.cache_dir = args.cache_dir.resolve()
    args.cache_dir.mkdir(parents=True, exist_ok=True)
    manifest = json.loads(download(MANIFEST, args.cache_dir / "manifest.json"))
    manifest = {entry["id"]: entry for entry in manifest["versions"]}
    helper = args.cache_dir / "helper"
    helper.mkdir(exist_ok=True)
    subprocess.run(
        [
            args.javac,
            "--release",
            "11",
            "-d",
            str(helper),
            str(Path(__file__).with_name("SnapshotRuntime.java")),
        ],
        check=True,
    )
    versions = args.versions or sorted(path.stem for path in SNAPSHOTS.glob("*.json"))
    generated = [(version, *generate(version, manifest, args, helper)) for version in versions]
    write_snapshots(
        [(SNAPSHOTS / f"{version}.json", snapshot) for version, snapshot, _ in generated],
        args.check,
    )
    write_recipes([recipes for _, _, recipes in generated], args.check)


if __name__ == "__main__":
    main()
