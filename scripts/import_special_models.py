import argparse
import copy
import json
import math
import pathlib
import subprocess

parser = argparse.ArgumentParser(
    description="Normalize the pinned block-model-renderer overrides for Java 1.21.1"
)
parser.add_argument("repository", type=pathlib.Path)
parser.add_argument("visuals", type=pathlib.Path)
args = parser.parse_args()
revision = subprocess.check_output(
    ["git", "-C", str(args.repository), "rev-parse", "HEAD"], text=True
).strip()
if revision != "9ed4d3b0462e3444c16b743dead6042fbf4c6eb6":
    parser.error(
        "Expected the pinned block-model-renderer revision from data/block-models/SOURCE.md"
    )
root = args.repository / "assets"
visual = args.visuals
vanilla = json.load(open(visual / "blockstates.json"))
textures = json.load(open(visual / "textures.json"))
models = json.load(open(visual / "models.json"))
states = {}
for pack in ["additional", "additional_26.1", "additional_1.21.11", "forced"]:
    for f in (root / pack / "assets").glob("*/models/**/*.json"):
        ns = f.relative_to(root / pack / "assets").parts[0]
        name = (
            ns
            + ":"
            + str(f.relative_to(root / pack / "assets" / ns / "models")).removesuffix(".json")
        )
        models[name] = json.load(open(f))
    for f in (root / pack / "assets").glob("minecraft/blockstates/*.json"):
        if "minecraft:" + f.stem in vanilla and f.stem not in [
            "water",
            "lava",
            "barrier",
            "light",
            "structure_void",
        ]:
            states["minecraft:" + f.stem] = json.load(open(f))


def inherit(name):
    if ":" not in name:
        name = "minecraft:" + name
    v = copy.deepcopy(models[name])
    p = inherit(v.pop("parent")) if "parent" in v else {}
    t = p.get("textures", {}) | v.get("textures", {})
    p.update(v)
    p["textures"] = t
    return p


corners = {
    "down": [(0, 0, 1), (0, 0, 0), (1, 0, 0), (1, 0, 1)],
    "up": [(0, 1, 0), (0, 1, 1), (1, 1, 1), (1, 1, 0)],
    "north": [(1, 1, 0), (1, 0, 0), (0, 0, 0), (0, 1, 0)],
    "south": [(0, 1, 1), (0, 0, 1), (1, 0, 1), (1, 1, 1)],
    "west": [(0, 1, 0), (0, 0, 0), (0, 0, 1), (0, 1, 1)],
    "east": [(1, 1, 1), (1, 0, 1), (1, 0, 0), (1, 1, 0)],
}
colors = dict(
    zip(
        [
            "white",
            "orange",
            "magenta",
            "light_blue",
            "yellow",
            "lime",
            "pink",
            "gray",
            "light_gray",
            "cyan",
            "purple",
            "blue",
            "brown",
            "green",
            "red",
            "black",
        ],
        [
            0xF9FFFE,
            0xF9801D,
            0xC74EBD,
            0x3AB3DA,
            0xFED83D,
            0x80C71F,
            0xF38BAA,
            0x474F52,
            0x9D9D97,
            0x169C9C,
            0x8932B8,
            0x3C44AA,
            0x835432,
            0x5E7C16,
            0xB02E26,
            0x1D1D21,
        ],
        strict=True,
    )
)


def rotate(p, axis, angle, origin):
    p = [p[i] - origin[i] for i in range(3)]
    a = (axis + 1) % 3
    b = (axis + 2) % 3
    c = math.cos(math.radians(angle))
    s = math.sin(math.radians(angle))
    p[a], p[b] = p[a] * c - p[b] * s, p[a] * s + p[b] * c
    return [p[i] + origin[i] for i in range(3)]


def texture(t, bindings):
    while t.startswith("#"):
        t = bindings[t[1:]]
    t = t if ":" in t else "minecraft:" + t
    if t in textures:
        return t
    for old, new in [
        ("entity/enchantment/", "entity/"),
        ("entity/end_portal/", "entity/"),
        ("entity/bed/", "entity/bed/"),
        ("entity/banner/base", "entity/banner/base"),
        ("entity/banner/banner_base", "entity/banner_base"),
    ]:
        v = t.replace(old, new)
        if v in textures:
            return v
    raise ValueError(t)


used = {}


def bake(name):
    if name in used:
        return
    m = inherit(name)
    faces = []
    for e in m.get("elements", []):
        for direction, f in e.get("faces", {}).items():
            if e.get("part") == "flag" and f.get("texture") == "#banner":
                continue
            tex = texture(f["texture"], m["textures"])
            points = []
            for c in corners[direction]:
                p = [e["from"][i] + c[i] * (e["to"][i] - e["from"][i]) for i in range(3)]
                r = e.get("rotation", {})
                o = r.get("origin", [8, 8, 8])
                if "axis" in r:
                    p = rotate(p, "xyz".index(r["axis"]), r["angle"], o)
                else:
                    for axis in [2, 1, 0]:
                        p = rotate(p, axis, r.get("xyz"[axis], 0), o)
                if m.get("dynamic") == "enchanting_book":
                    pivot = e.get("pivot", [8, 8, 8])
                    part = e.get("part")
                    angles = {
                        "cover_left": math.pi + 1.25,
                        "cover_right": -1.25,
                        "book_spine": math.pi / 2,
                        "pages_left": 1.25,
                        "pages_right": -1.25,
                        "flipping_page_right": 1.0,
                        "flipping_page_left": -1.0,
                    }
                    p = rotate(p, 1, math.degrees(angles.get(part, 0)), pivot)
                    p = rotate(p, 2, 80, [8, 8, 8])
                    p = rotate(p, 1, 90, [8, 8, 8])
                    p[1] += 5.6
                points.append([round(v / 16, 7) for v in p])
            u0, v0, u1, v1 = f.get("uv", [0, 0, 16, 16])
            uv = []
            cs = [(0, 0), (0, 1), (1, 1), (1, 0)]
            for i in range(4):
                u, v = cs[(i + f.get("rotation", 0) // 90) % 4]
                uv.append([(u0 + (u1 - u0) * u) / 16, (v0 + (v1 - v0) * v) / 16])
            color = [255] * 4
            if f.get("tintindex", -1) >= 0:
                value = colors[m["tints"][f["tintindex"]]]
                color = [value >> 16, (value >> 8) & 255, value & 255, 255]
            faces.append(
                dict(positions=points, uv=uv, texture=tex, color=color, shade=e.get("shade", True))
            )
    used[name] = faces


rules = {}
for name, state in states.items():
    applications = []
    for predicate, app in state.get("variants", {}).items():
        cond = dict(p.split("=") for p in predicate.split(",") if p)
        applications.append((cond, app))
    for part in state.get("multipart", []):
        applications.append((part.get("when", {}), part["apply"]))
    rules[name] = []
    for cond, app in applications:
        for a in app if isinstance(app, list) else [app]:
            bake(a["model"])
            y = a.get("y", 0)
            if "rotation" in cond:
                y = (180 + float(cond["rotation"]) * 22.5) % 360
            rules[name].append(dict(when=cond, model=a["model"], x=a.get("x", 0), y=y))
print(len(rules), "states", len(used), "models")
json.dump(
    dict(states=rules, models=used),
    open(pathlib.Path(__file__).resolve().parents[1] / "data/block-models/models.json", "w"),
    separators=(",", ":"),
    sort_keys=True,
)
