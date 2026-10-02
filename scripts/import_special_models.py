import argparse
import copy
import json
import math
import pathlib
import subprocess

REVISION = "9ed4d3b0462e3444c16b743dead6042fbf4c6eb6"
PACKS = ("additional", "additional_26.1", "additional_1.21.11", "forced")
EXCLUDED_BLOCKS = {"water", "lava", "barrier", "light", "structure_void"}
FACE_CORNERS = {
    "down": [(0, 0, 1), (0, 0, 0), (1, 0, 0), (1, 0, 1)],
    "up": [(0, 1, 0), (0, 1, 1), (1, 1, 1), (1, 1, 0)],
    "north": [(1, 1, 0), (1, 0, 0), (0, 0, 0), (0, 1, 0)],
    "south": [(0, 1, 1), (0, 0, 1), (1, 0, 1), (1, 1, 1)],
    "west": [(0, 1, 0), (0, 0, 0), (0, 0, 1), (0, 1, 1)],
    "east": [(1, 1, 1), (1, 0, 1), (1, 0, 0), (1, 1, 0)],
}
UV_CORNERS = [(0, 0), (0, 1), (1, 1), (1, 0)]
COLORS = {
    "white": 0xF9FFFE,
    "orange": 0xF9801D,
    "magenta": 0xC74EBD,
    "light_blue": 0x3AB3DA,
    "yellow": 0xFED83D,
    "lime": 0x80C71F,
    "pink": 0xF38BAA,
    "gray": 0x474F52,
    "light_gray": 0x9D9D97,
    "cyan": 0x169C9C,
    "purple": 0x8932B8,
    "blue": 0x3C44AA,
    "brown": 0x835432,
    "green": 0x5E7C16,
    "red": 0xB02E26,
    "black": 0x1D1D21,
}
BOOK_ANGLES = {
    "cover_left": math.pi + 1.25,
    "cover_right": -1.25,
    "book_spine": math.pi / 2,
    "pages_left": 1.25,
    "pages_right": -1.25,
    "flipping_page_right": 1.0,
    "flipping_page_left": -1.0,
}


def read_json(path: pathlib.Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def load_overrides(root: pathlib.Path, vanilla: dict, models: dict) -> dict:
    states = {}
    for pack in PACKS:
        assets = root / pack / "assets"
        for path in assets.glob("*/models/**/*.json"):
            namespace = path.relative_to(assets).parts[0]
            model_path = path.relative_to(assets / namespace / "models").with_suffix("")
            models[f"{namespace}:{model_path.as_posix()}"] = read_json(path)
        for path in assets.glob("minecraft/blockstates/*.json"):
            name = "minecraft:" + path.stem
            if name in vanilla and path.stem not in EXCLUDED_BLOCKS:
                states[name] = read_json(path)
    return states


def inherit(name: str, models: dict) -> dict:
    if ":" not in name:
        name = "minecraft:" + name
    model = copy.deepcopy(models[name])
    parent = inherit(model.pop("parent"), models) if "parent" in model else {}
    textures = parent.get("textures", {}) | model.get("textures", {})
    parent.update(model)
    parent["textures"] = textures
    return parent


def rotate(point: list[float], axis: int, angle: float, origin: list[float]) -> list[float]:
    point = [point[i] - origin[i] for i in range(3)]
    first_axis = (axis + 1) % 3
    second_axis = (axis + 2) % 3
    cosine = math.cos(math.radians(angle))
    sine = math.sin(math.radians(angle))
    point[first_axis], point[second_axis] = (
        point[first_axis] * cosine - point[second_axis] * sine,
        point[first_axis] * sine + point[second_axis] * cosine,
    )
    return [point[i] + origin[i] for i in range(3)]


def resolve_texture(identifier: str, bindings: dict, textures: dict) -> str:
    while identifier.startswith("#"):
        identifier = bindings[identifier[1:]]
    if ":" not in identifier:
        identifier = "minecraft:" + identifier
    if identifier in textures:
        return identifier
    for source, target in (
        ("entity/enchantment/", "entity/"),
        ("entity/end_portal/", "entity/"),
        ("entity/banner/banner_base", "entity/banner_base"),
    ):
        resolved = identifier.replace(source, target)
        if resolved in textures:
            return resolved
    raise ValueError(f"Missing texture: {identifier}")


def face_positions(model: dict, element: dict, direction: str) -> list[list[float]]:
    points = []
    rotation = element.get("rotation", {})
    origin = rotation.get("origin", [8, 8, 8])
    for corner in FACE_CORNERS[direction]:
        point = [
            element["from"][i] + corner[i] * (element["to"][i] - element["from"][i])
            for i in range(3)
        ]
        if "axis" in rotation:
            point = rotate(point, "xyz".index(rotation["axis"]), rotation["angle"], origin)
        else:
            for axis in (2, 1, 0):
                point = rotate(point, axis, rotation.get("xyz"[axis], 0), origin)
        if model.get("dynamic") == "enchanting_book":
            pivot = element.get("pivot", [8, 8, 8])
            angle = BOOK_ANGLES.get(element.get("part"), 0)
            point = rotate(point, 1, math.degrees(angle), pivot)
            point = rotate(point, 2, 80, [8, 8, 8])
            point = rotate(point, 1, 90, [8, 8, 8])
            point[1] += 5.6
        points.append([round(value / 16, 7) for value in point])
    return points


def face_uv(face: dict) -> list[list[float]]:
    u0, v0, u1, v1 = face.get("uv", [0, 0, 16, 16])
    coordinates = []
    for index in range(4):
        u, v = UV_CORNERS[(index + face.get("rotation", 0) // 90) % 4]
        coordinates.append([(u0 + (u1 - u0) * u) / 16, (v0 + (v1 - v0) * v) / 16])
    return coordinates


def bake(name: str, models: dict, textures: dict) -> list[dict]:
    model = inherit(name, models)
    faces = []
    for element in model.get("elements", []):
        for direction, face in element.get("faces", {}).items():
            if element.get("part") == "flag" and face.get("texture") == "#banner":
                continue
            texture = resolve_texture(face["texture"], model["textures"], textures)
            color = [255] * 4
            if face.get("tintindex", -1) >= 0:
                value = COLORS[model["tints"][face["tintindex"]]]
                color = [value >> 16, (value >> 8) & 255, value & 255, 255]
            faces.append(
                {
                    "positions": face_positions(model, element, direction),
                    "uv": face_uv(face),
                    "texture": texture,
                    "color": color,
                    "shade": element.get("shade", True),
                }
            )
    return faces


def compile_overrides(states: dict, models: dict, textures: dict) -> dict:
    rules = {}
    baked_models = {}
    for name, state in states.items():
        applications = []
        for predicate, application in state.get("variants", {}).items():
            condition = dict(pair.split("=") for pair in predicate.split(",") if pair)
            applications.append((condition, application))
        for part in state.get("multipart", []):
            applications.append((part.get("when", {}), part["apply"]))
        rules[name] = []
        for condition, choices in applications:
            for application in choices if isinstance(choices, list) else [choices]:
                model_name = application["model"]
                if model_name not in baked_models:
                    baked_models[model_name] = bake(model_name, models, textures)
                rotation = application.get("y", 0)
                if "rotation" in condition:
                    rotation = (180 + float(condition["rotation"]) * 22.5) % 360
                rules[name].append(
                    {
                        "when": condition,
                        "model": model_name,
                        "x": application.get("x", 0),
                        "y": rotation,
                    }
                )
    return {"states": rules, "models": baked_models}


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Normalize the pinned block-model-renderer overrides for Java 1.21.1"
    )
    parser.add_argument("repository", type=pathlib.Path)
    parser.add_argument("visuals", type=pathlib.Path)
    parser.add_argument(
        "--output",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parents[1] / "data/block-models/models.json",
    )
    args = parser.parse_args()
    revision = subprocess.check_output(
        ["git", "-C", str(args.repository), "rev-parse", "HEAD"], text=True
    ).strip()
    if revision != REVISION:
        parser.error(
            "Expected the pinned block-model-renderer revision from data/block-models/SOURCE.md"
        )
    models = read_json(args.visuals / "models.json")
    textures = read_json(args.visuals / "textures.json")
    states = load_overrides(
        args.repository / "assets", read_json(args.visuals / "blockstates.json"), models
    )
    output = compile_overrides(states, models, textures)
    args.output.write_text(
        json.dumps(output, separators=(",", ":"), sort_keys=True), encoding="utf-8"
    )
    print(len(output["states"]), "states", len(output["models"]), "models")


if __name__ == "__main__":
    main()
