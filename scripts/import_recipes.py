import argparse
import hashlib
import io
import json
import tempfile
import urllib.request
import zipfile
from collections import Counter, defaultdict
from functools import cache
from itertools import product
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SNAPSHOTS = ROOT / "src/catalog/data"
MANIFEST = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"
COOKING = {"smelting", "blasting", "smoking", "campfire_cooking", "stonecutting"}


def download(url, path, sha1=None):
    if path.exists():
        content = path.read_bytes()
        if sha1 is None or hashlib.sha1(content).hexdigest() == sha1:
            return content
    with urllib.request.urlopen(url, timeout=60) as response:
        content = response.read()
    if sha1 is not None and hashlib.sha1(content).hexdigest() != sha1:
        raise ValueError(f"Download hash mismatch: {url}")
    path.write_bytes(content)
    return content


def resources(archive, folders):
    result = {}
    for name in sorted(archive.namelist()):
        parts = name.split("/")
        for folder in folders:
            if (
                parts[0] == "data"
                and parts[2 : 2 + len(folder)] == list(folder)
                and len(parts) > 2 + len(folder)
                and name.endswith(".json")
            ):
                identifier = parts[1] + ":" + "/".join(parts[2 + len(folder) :])[:-5]
                if identifier in result:
                    raise ValueError(f"Duplicate resource: {identifier}")
                result[identifier] = json.loads(archive.read(name))
    return result


def recipe_index(definitions, tags, items):
    active = set()

    @cache
    def tag(identifier):
        entries = tags[identifier]["values"]
        if identifier in active:
            raise ValueError(f"Cyclic item tag: {identifier}")
        active.add(identifier)
        values = set()
        for entry in entries:
            required = True
            if isinstance(entry, dict):
                required = entry.get("required", True)
                entry = entry["id"]
            try:
                values.update(tag(entry[1:]) if entry.startswith("#") else [items[entry]])
            except KeyError:
                if required:
                    raise
        active.remove(identifier)
        return tuple(sorted(values))

    def ingredient(value):
        if isinstance(value, list):
            return tuple(sorted({item for entry in value for item in ingredient(entry)}))
        if isinstance(value, dict):
            value = "#" + value["tag"] if "tag" in value else value["item"]
        return tag(value[1:]) if value.startswith("#") else (items[value],)

    index = defaultdict(list)
    indexed = []
    for identifier, recipe in sorted(
        definitions.items(),
        key=lambda pair: (pair[1]["type"].removeprefix("minecraft:") in COOKING, pair[0]),
    ):
        kind = recipe["type"].removeprefix("minecraft:")
        if kind == "crafting_shaped":
            keys = recipe["key"]
            slots = [
                [None if symbol == " " else keys[symbol] for symbol in row]
                for row in recipe["pattern"]
            ]
        elif kind == "crafting_shapeless":
            slots = [recipe["ingredients"]]
        elif kind in COOKING:
            slots = [[recipe["ingredient"]]]
        elif kind in {"smithing", "smithing_transform"}:
            slots = [[recipe[key] for key in ("template", "base", "addition") if key in recipe]]
        elif kind == "crafting_transmute":
            slots = [[recipe["input"], recipe["material"]]]
        else:
            continue
        result = recipe["result"]
        if isinstance(result, str):
            result = {"id": result, "count": recipe.get("count", 1)}
        output_name = result.get("id", result.get("item"))
        if output_name is None:
            continue
        output = items[output_name]
        count = result.get("count", 1)
        if not isinstance(count, int) or count <= 0:
            raise ValueError(f"Invalid output count: {identifier}")
        options = {
            json.dumps(value, sort_keys=True): ingredient(value)
            for row in slots
            for value in row
            if value is not None
        }
        if any(not values for values in options.values()):
            raise ValueError(f"Empty ingredient: {identifier}")
        for choices in product(*options.values()):
            selected = dict(zip(options, choices, strict=True))
            shape = [
                [
                    selected[json.dumps(value, sort_keys=True)] if value is not None else None
                    for value in row
                ]
                for row in slots
            ]
            entry = {"result": {"id": output, "count": count}}
            entry["inShape" if kind == "crafting_shaped" else "ingredients"] = (
                shape if kind == "crafting_shaped" else shape[0]
            )
            index[str(output)].append(entry)
        indexed.append(identifier)
    resolved_tags = {identifier: list(tag(identifier)) for identifier in sorted(tags)}
    assert index and definitions and resolved_tags
    assert len(indexed) == len(set(indexed))
    return dict(sorted(index.items(), key=lambda pair: int(pair[0]))), resolved_tags, indexed


def server_runtime(version, manifest, directory):
    entry = manifest[version]
    metadata = json.loads(download(entry["url"], directory / f"{version}.json", entry["sha1"]))
    server = metadata["downloads"]["server"]
    content = download(server["url"], directory / f"{version}.jar", server["sha1"])
    server_hash = hashlib.sha256(content).hexdigest()
    with zipfile.ZipFile(io.BytesIO(content)) as archive:
        bundled = [
            name
            for name in archive.namelist()
            if name.startswith("META-INF/versions/") and name.endswith(".jar")
        ]
        if bundled:
            if len(bundled) != 1:
                raise ValueError(f"Ambiguous server runtime: {version}")
            content = archive.read(bundled[0])
    runtime_hash = hashlib.sha256(content).hexdigest()
    return metadata, content, server_hash, runtime_hash


def add_recipes(snapshot, metadata, content, server_hash, runtime_hash):
    version = snapshot["version"]
    provenance = snapshot["provenance"]
    expected = provenance.get("official_server_sha256", provenance.get("runtime_jar_sha256"))
    if expected not in {server_hash, runtime_hash}:
        raise ValueError(f"Runtime does not match existing snapshot: {version}")
    with zipfile.ZipFile(io.BytesIO(content)) as archive:
        definitions = resources(archive, [("recipe",), ("recipes",)])
        tags = resources(archive, [("tags", "item"), ("tags", "items")])
    items = {item["name"]: item["id"] for item in snapshot["datasets"]["items"]}
    index, tags, indexed = recipe_index(definitions, tags, items)
    snapshot["datasets"].update(recipes=index, recipeDefinitions=definitions, itemTags=tags)
    provenance["recipes"] = {
        "server_url": metadata["downloads"]["server"]["url"],
        "server_sha256": server_hash,
        "runtime_sha256": runtime_hash,
        "method": "vanilla recipe and item-tag resources; numeric IDs from this snapshot's item registry",
        "definitions": len(definitions),
        "indexed_definitions": len(indexed),
        "variants": sum(map(len, index.values())),
        "types": dict(sorted(Counter(recipe["type"] for recipe in definitions.values()).items())),
        "unindexed": sorted(set(definitions) - set(indexed)),
    }
    print(
        f"{version}: {len(definitions)} definitions, {len(indexed)} indexed, {len(tags)} tags",
        flush=True,
    )


def write_snapshots(updates, check):
    for path, snapshot in updates:
        content = json.dumps(snapshot, ensure_ascii=False, separators=(",", ":")) + "\n"
        if check:
            if path.read_text() != content:
                raise ValueError(f"Snapshot is out of date: {path.name}")
        else:
            with tempfile.NamedTemporaryFile(
                mode="w", encoding="utf-8", dir=path.parent, delete=False
            ) as output:
                output.write(content)
            Path(output.name).replace(path)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("versions", nargs="*")
    parser.add_argument(
        "--cache-dir", type=Path, default=Path(tempfile.gettempdir()) / "schemora-recipe-sources"
    )
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    args.cache_dir.mkdir(parents=True, exist_ok=True)
    manifest = json.loads(download(MANIFEST, args.cache_dir / "manifest.json"))
    manifest = {entry["id"]: entry for entry in manifest["versions"]}
    snapshots = {path.stem: path for path in SNAPSHOTS.glob("*.json")}
    paths = [snapshots[version] for version in args.versions or sorted(snapshots)]
    updates = []
    for path in paths:
        snapshot = json.loads(path.read_text())
        add_recipes(snapshot, *server_runtime(snapshot["version"], manifest, args.cache_dir))
        updates.append((path, snapshot))
    write_snapshots(updates, args.check)


if __name__ == "__main__":
    main()
