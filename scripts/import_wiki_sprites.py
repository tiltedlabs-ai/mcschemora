import argparse
import concurrent.futures
import hashlib
import io
import json
import math
import pathlib
import re
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

from PIL import Image

API = "https://minecraft.wiki/api.php"
FAMILIES = {"BlockSprite": "blocks.png", "EntitySprite": "entities.png"}
MODULES = ["Module:SpriteFile", "Module:SpriteGrid", "Module:Schematic/data"]


def request(url):
    for attempt in range(6):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "SchemoraSpriteImporter/1.0"})
            with urllib.request.urlopen(req, timeout=60) as response:
                return response.read()
        except (urllib.error.URLError, TimeoutError) as error:
            if isinstance(error, urllib.error.HTTPError) and error.code not in (
                429,
                500,
                502,
                503,
                504,
            ):
                raise
            if attempt == 5:
                raise
            time.sleep(2**attempt)
    raise RuntimeError("Request retries exhausted")


def api(**params):
    url = API + "?" + urllib.parse.urlencode({"action": "query", "format": "json", **params})
    result = json.loads(request(url))
    if "error" in result:
        raise RuntimeError(result["error"])
    return result


def paginate(**params):
    while True:
        result = api(**params)
        yield result["query"]
        if "continue" not in result:
            break
        params.update(result["continue"])


def inventory(family):
    files = []
    for batch in paginate(
        list="allimages",
        aiprefix=family + " ",
        ailimit=500,
        aiprop="canonicaltitle|url|size|sha1|timestamp|mime",
    ):
        files.extend(batch["allimages"])
    if not files:
        raise ValueError(f"Empty inventory for {family}")
    return files


def redirect_titles(family):
    return [
        page["title"]
        for batch in paginate(
            list="allpages",
            apnamespace=6,
            apprefix=family + " ",
            apfilterredir="redirects",
            aplimit=500,
        )
        for page in batch["allpages"]
    ]


def image_metadata(titles):
    files = {}
    redirects = {}
    for offset in range(0, len(titles), 50):
        result = api(
            titles="|".join(titles[offset : offset + 50]),
            redirects=1,
            prop="imageinfo|revisions",
            iiprop="url|size|sha1|timestamp|mime|extmetadata",
            rvprop="ids|timestamp|content",
            rvslots="main",
        )["query"]
        redirects.update((r["from"], r["to"]) for r in result.get("redirects", []))
        for page in result["pages"].values():
            if "imageinfo" not in page:
                raise ValueError(f"Missing image: {page['title']}")
            files[page["title"]] = page["imageinfo"][0]
            revision = page["revisions"][0]
            files[page["title"]]["description"] = {
                "revision": revision["revid"],
                "timestamp": revision["timestamp"],
                "wikitext": revision["slots"]["main"]["*"],
            }
        if offset % 500 == 0:
            print(
                f"Fetched metadata for {min(offset + 50, len(titles))}/{len(titles)} titles",
                flush=True,
            )
    return files, redirects


def source_modules():
    pages = api(
        titles="|".join(MODULES),
        prop="revisions",
        rvprop="ids|timestamp|content",
        rvslots="main",
    )["query"]["pages"]
    return {
        page["title"]: {
            "revision": page["revisions"][0]["revid"],
            "timestamp": page["revisions"][0]["timestamp"],
            "source": page["revisions"][0]["slots"]["main"]["*"],
        }
        for page in pages.values()
    }


def download(info, cache):
    path = cache / info["sha1"]
    data = path.read_bytes() if path.exists() else request(info["url"])
    digest = hashlib.sha256(data).hexdigest()
    if "download_sha256" in info and digest != info["download_sha256"]:
        raise ValueError(f"Cached/downloaded bytes changed: {info['url']}")
    with Image.open(io.BytesIO(data)) as image:
        if getattr(image, "n_frames", 1) != 1:
            raise ValueError(f"Animated sprite requires explicit handling: {info['url']}")
        if image.size != (info["width"], info["height"]):
            raise ValueError(f"Dimensions changed: {info['url']}")
        image.load()
    if not path.exists():
        path.write_bytes(data)
    return path, digest


def identity(title, family):
    prefix = f"File:{family} "
    if not title.startswith(prefix) or not title.endswith(".png"):
        raise ValueError(f"Unexpected sprite filename: {title}")
    return family + ":" + title[len(prefix) : -4]


def pack(family, catalog, blobs, output, sprites):
    files = catalog["files"]
    redirects = catalog["redirects"]
    references = {
        identity(title, family): title for title in files if title.startswith(f"File:{family} ")
    }
    for source, target in redirects.items():
        if not source.startswith(f"File:{family} "):
            continue
        visited = {source}
        while target in redirects:
            if target in visited:
                raise ValueError(f"Redirect cycle: {source}")
            visited.add(target)
            target = redirects[target]
        if target not in files:
            raise ValueError(f"Missing redirect target: {source} -> {target}")
        references[identity(source, family)] = target
    titles = sorted(
        set(references.values()),
        key=lambda title: (-files[title]["height"], -files[title]["width"], title),
    )
    area = sum(files[title]["width"] * files[title]["height"] for title in titles)
    width = 2 ** math.ceil(math.log2(max(math.sqrt(area), max(files[t]["width"] for t in titles))))
    free = [(0, 0, width, width)]
    height = width
    rectangles = {}
    for title in titles:
        w, h = files[title]["width"], files[title]["height"]
        candidates = [(y, x, i) for i, (x, y, fw, fh) in enumerate(free) if fw >= w and fh >= h]
        if not candidates:
            free.append((0, height, width, max(width, h)))
            height += max(width, h)
            index = len(free) - 1
        else:
            index = min(candidates)[2]
        x, y, fw, fh = free.pop(index)
        rectangles[title] = [x, y, w, h]
        if fw - w > fh - h:
            splits = [(x + w, y, fw - w, fh), (x, y + h, w, fh - h)]
        else:
            splits = [(x + w, y, fw - w, h), (x, y + h, fw, fh - h)]
        free.extend(rect for rect in splits if rect[2] > 0 and rect[3] > 0)
    sheet = Image.new("RGBA", (width, max(y + h for _, y, _, h in rectangles.values())))
    for title, rect in rectangles.items():
        with Image.open(blobs[files[title]["sha1"]]) as image:
            sheet.paste(image.convert("RGBA"), tuple(rect[:2]))
    for key, title in references.items():
        if key in sprites:
            raise ValueError(f"Duplicate sprite: {key}")
        sprites[key] = {"sheet": FAMILIES[family], "rect": rectangles[title], "source": title}
    sheet.save(output / FAMILIES[family], optimize=True)
    return {"file": FAMILIES[family], "width": sheet.width, "height": sheet.height}


def schematic(module, files, blobs, output, sprites):
    source = module["source"]
    match = re.fullmatch(
        r"\s*return\s*\{\s*settings\s*=\s*\{.*?\},\s*ids\s*=\s*\{(.*?)\}\s*\}\s*", source, re.S
    )
    if not match or not re.search(r"\bsheetsize\s*=\s*512\b", source):
        raise ValueError("Schematic data format changed; review the upstream module")
    entries = re.compile(r"\s*\['([^']+)'\]\s*=\s*\{\s*pos\s*=\s*(\d+)\s*\}\s*,?\s*")
    body = match[1]
    if entries.sub("", body).strip():
        raise ValueError("Unsupported schematic ID definition")
    title = "File:SchematicSprite.png"
    path = blobs[files[title]["sha1"]]
    with Image.open(path) as image:
        if image.width != 512 or image.height % 16:
            raise ValueError("Schematic sheet geometry changed")
        size = image.size
    for name, position in entries.findall(body):
        pos = int(position) - 1
        rect = [(pos % 32) * 16, (pos // 32) * 16, 16, 16]
        key = "SchematicSprite:" + name
        if pos < 0 or rect[1] + 16 > size[1] or key in sprites:
            raise ValueError(f"Invalid schematic entry: {name}")
        sprites[key] = {
            "sheet": "schematic.png",
            "rect": rect,
            "source": title,
            "position": pos + 1,
        }
    (output / "schematic.png").write_bytes(path.read_bytes())
    return {"file": "schematic.png", "width": size[0], "height": size[1]}


def main():
    parser = argparse.ArgumentParser(description="Import three Minecraft Wiki sprite sheets")
    parser.add_argument(
        "--output",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parents[1] / "data/wiki-sprites",
    )
    parser.add_argument(
        "--cache-dir",
        type=pathlib.Path,
        default=pathlib.Path(tempfile.gettempdir()) / "schemora-wiki-sprites-cache",
    )
    parser.add_argument(
        "--refresh", action="store_true", help="Fetch a new inventory and module snapshot"
    )
    args = parser.parse_args()
    args.cache_dir.mkdir(parents=True, exist_ok=True)
    snapshot = args.cache_dir / "snapshot.json"
    if snapshot.exists() and not args.refresh:
        catalog = json.loads(snapshot.read_text())
    else:
        titles = ["File:SchematicSprite.png"]
        for family in FAMILIES:
            items = inventory(family)
            titles.extend(item["title"] for item in items)
            aliases = redirect_titles(family)
            titles.extend(aliases)
            print(f"{family}: {len(items)} files, {len(aliases)} redirects", flush=True)
        files, redirects = image_metadata(titles)
        catalog = {"files": files, "redirects": redirects, "modules": source_modules()}
        snapshot.write_text(json.dumps(catalog, ensure_ascii=False, indent=2) + "\n")
    files = catalog["files"]
    unique = {info["sha1"]: info for info in files.values()}
    blobs = {}
    errors = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        jobs = {pool.submit(download, info, args.cache_dir): sha for sha, info in unique.items()}
        for i, job in enumerate(concurrent.futures.as_completed(jobs), 1):
            try:
                path, digest = job.result()
                blobs[jobs[job]] = path
                unique[jobs[job]]["download_sha256"] = digest
            except Exception as error:
                errors.append(str(error))
                print(f"Download failed: {error}", flush=True)
            if i % 200 == 0 or i == len(jobs):
                print(f"Downloaded/verified {i}/{len(jobs)} unique images", flush=True)
    if errors:
        raise RuntimeError("\n".join(errors))
    for info in files.values():
        info["download_sha256"] = unique[info["sha1"]]["download_sha256"]
    snapshot.write_text(json.dumps(catalog, ensure_ascii=False, indent=2) + "\n")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=args.output.parent) as temporary:
        output = pathlib.Path(temporary)
        sprites = {}
        sheets = [pack(family, catalog, blobs, output, sprites) for family in FAMILIES]
        sheets.append(
            schematic(catalog["modules"]["Module:Schematic/data"], files, blobs, output, sprites)
        )
        manifest = {
            "source": "https://minecraft.wiki/",
            "sheets": sheets,
            "sprites": dict(sorted(sprites.items())),
            "redirects": dict(sorted(catalog["redirects"].items())),
            "files": dict(sorted(files.items())),
            "modules": catalog["modules"],
        }
        (output / "sprites.json").write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n"
        )
        args.output.mkdir(parents=True, exist_ok=True)
        for name in ["blocks.png", "entities.png", "schematic.png", "sprites.json"]:
            (output / name).replace(args.output / name)
        print(f"Wrote {len(sprites)} identifiers, 3 sheets to {args.output}")


if __name__ == "__main__":
    main()
