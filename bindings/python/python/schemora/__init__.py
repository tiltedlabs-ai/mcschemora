"""Schemora: Minecraft schematic authoring backed by Rust."""

from __future__ import annotations

import json
from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from types import MappingProxyType

from . import _core
from ._core import Placement as _Placement
from ._core import bed, chest, door, sign

__all__ = [
    "Schematic",
    "MinecraftData",
    "Region",
    "Selection",
    "Block",
    "Bounds",
    "Fragment",
    "Report",
    "block",
    "water_source",
    "bed",
    "door",
    "sign",
    "chest",
    "item",
    "mob",
]
__version__ = "0.1.0"
Position = tuple[int, int, int]


def _properties(values: Mapping) -> dict[str, str]:
    result = {}
    for key, value in values.items():
        if not isinstance(key, str):
            raise TypeError("Property names must be strings")
        if isinstance(value, bool):
            value = "true" if value else "false"
        elif isinstance(value, int):
            value = str(value)
        elif not isinstance(value, str):
            raise TypeError(f"{key}: expected a string, integer, or boolean")
        result[key] = value
    return result


@dataclass(frozen=True)
class Block:
    """Immutable block identifier and properties; validated on placement."""

    id: str
    _properties: tuple[tuple[str, str], ...] = ()

    @property
    def states(self) -> Mapping[str, str]:
        return MappingProxyType(dict(self._properties))

    def __str__(self) -> str:
        suffix = ",".join(f"{k}={v}" for k, v in self._properties)
        return self.id + (f"[{suffix}]" if suffix else "")

    @classmethod
    def _from_native(cls, value):
        name, properties = value
        return cls(name, tuple(sorted(properties.items())))


def block(identifier: str, **states) -> Block:
    """Describe a block using Minecraft property names."""
    if not isinstance(identifier, str):
        raise TypeError("Block identifier must be a string")
    if ":" not in identifier:
        identifier = "minecraft:" + identifier
    return Block(identifier, tuple(sorted(_properties(states).items())))


def water_source() -> Block:
    return block("water", level=0)


@dataclass(frozen=True)
class Bounds:
    start: Position
    size: Position

    @classmethod
    def _from_native(cls, value):
        start, size = value
        return cls(tuple(start), tuple(size))


@dataclass(frozen=True)
class Report:
    errors: tuple[str, ...] = ()
    losses: tuple[str, ...] = ()
    warnings: tuple[str, ...] = ()
    unknown: tuple[str, ...] = ()

    @property
    def issues(self) -> tuple[str, ...]:
        return self.errors + self.losses + self.warnings + self.unknown

    @property
    def ok(self) -> bool:
        return not (self.errors or self.losses or self.unknown)

    def __str__(self) -> str:
        if not self.issues:
            return "No issues found."
        return "\n".join(
            f"{label}: {message}"
            for label, messages in (
                ("Error", self.errors),
                ("Loss", self.losses),
                ("Warning", self.warnings),
                ("Unknown", self.unknown),
            )
            for message in messages
        )


class MinecraftData:
    """Download pinned Java catalogs into a shared runtime cache."""

    def __init__(self, cache_dir=None, *, offline=False):
        directory = None if cache_dir is None else Path(cache_dir).expanduser().resolve()
        self._native = _core.MinecraftData(directory, offline)

    @property
    def cache_dir(self) -> Path:
        return Path(self._native.cache_dir())

    @property
    def versions(self) -> tuple[str, ...]:
        """Available Java catalogs at or above the 1.13 minimum."""
        return tuple(self._native.versions())

    def fetch(self, version="latest", *, visuals=False) -> str:
        """Cache a complete catalog and optionally prepare its visual bundle."""
        return self._native.fetch(version, visuals)

    def dataset_path(self, version: str, kind: str) -> Path:
        """Fetch a raw blocks, items, entities, or blockCollisionShapes dataset."""
        return Path(self._native.dataset_path(version, kind))

    def visuals(self, version="latest") -> Path:
        """Return the shared Java 1.21.1 visual bundle for rendering any version."""
        return Path(self._native.visuals(version))


@lru_cache(maxsize=1)
def _default_data() -> MinecraftData:
    return MinecraftData()


def _source(data):
    return (data if data is not None else _default_data())._native


class Registry:
    def __init__(self, schematic):
        self._schematic = schematic

    def describe(self, identifier: str) -> dict:
        return json.loads(self._schematic._native.describe(identifier))


class Schematic:
    def __init__(self, native):
        self._native = native

    @classmethod
    def create(cls, *, edition="java", version="latest", data: MinecraftData | None = None):
        return cls(_core.Document(edition, version, _source(data)))

    @classmethod
    def load(cls, path, *, format=None, data: MinecraftData | None = None):
        path = Path(path)
        return cls.from_bytes(
            path.read_bytes(), format=format or path.suffix.lstrip("."), data=data
        )

    @classmethod
    def from_bytes(cls, content: bytes, *, format: str, data: MinecraftData | None = None):
        return cls(_core.Document.from_bytes(content, format, _source(data)))

    def to_bytes(self, *, format: str, allow_loss=False, flatten=False) -> bytes:
        return self._native.to_bytes(format, allow_loss, flatten)

    def save(self, path, *, format=None, allow_loss=False, flatten=False):
        path = Path(path)
        data = self.to_bytes(
            format=format or path.suffix.lstrip("."), allow_loss=allow_loss, flatten=flatten
        )
        path.write_bytes(data)

    def region(self, name="main") -> Region:
        return Region(self._native.region(name))

    def export_glb(self, path, *, region: str | None = None, y=None) -> tuple[str, ...]:
        """Export textured geometry; return diagnostics for visual approximations.

        Select a world Y level with an integer or an inclusive (minimum, maximum) pair.
        """
        if y is not None:
            if type(y) is int:
                y = (y, y)
            elif (
                not isinstance(y, (tuple, list))
                or len(y) != 2
                or any(type(value) is not int for value in y)
            ):
                raise ValueError("y must be an integer or an inclusive pair of integers")
        content, diagnostics = self._native.glb(region, y)
        Path(path).write_bytes(content)
        return tuple(diagnostics)

    def add_region(self, name: str, *, origin=(0, 0, 0)) -> Region:
        return Region(self._native.add_region(name, origin))

    @property
    def regions(self) -> tuple[str, ...]:
        return tuple(self._native.regions())

    @property
    def edition(self) -> str:
        return self._native.info()[0]

    @property
    def version(self) -> str:
        return self._native.info()[1]

    @property
    def data_version(self) -> int:
        return self._native.info()[2]

    @property
    def metadata(self) -> str:
        """Document metadata as typed SNBT."""
        return self._native.metadata()

    @metadata.setter
    def metadata(self, snbt: str):
        self._native.set_metadata(snbt)

    @property
    def registry(self) -> Registry:
        return Registry(self)

    def validate(self) -> Report:
        """Check Java block rules without changing the schematic or simulating ticks.

        Warnings describe unstable states. Unknown results need surrounding world
        blocks or game data; they prevent the report from confirming validity.
        """
        errors, warnings, unknown = self._native.validate()
        return Report(errors=tuple(errors), warnings=tuple(warnings), unknown=tuple(unknown))

    def check_export(self, *, format: str, flatten=False) -> Report:
        errors, losses = self._native.check_export(format, flatten)
        return Report(tuple(errors), tuple(losses))


class Region:
    def __init__(self, native):
        self._native = native

    @property
    def bounds(self) -> Bounds:
        return Bounds._from_native(self._native.bounds())

    @property
    def origin(self) -> Position:
        return tuple(self._native.origin())

    def get(self, at: Position) -> Block:
        return Block._from_native(self._native.get(at))

    def set(self, at: Position, content: Block | Fragment):
        if isinstance(content, Fragment):
            self._native.set_fragment(at, content._native)
        elif isinstance(content, Block):
            self.set_many([(at, content)])
        else:
            raise TypeError("set() expects a Block or Fragment")

    def set_many(self, placements: Iterable[tuple[Position, Block]]):
        self._native.set_many([(at, b.id, dict(b.states)) for at, b in placements])

    def patch(self, at: Position, **states):
        self.select(start=at, size=(1, 1, 1)).patch(**states)

    def select(self, *, start: Position, size: Position) -> Selection:
        return Selection(self._native.select(start, size))

    def place(self, placement: _Placement, *, at: Position, replace=False):
        if not isinstance(placement, _Placement):
            raise TypeError("place() expects a bed, door, sign, or chest helper")
        self._native.place(placement, at, replace)

    @property
    def entities(self) -> Entities:
        return Entities(self._native)

    @property
    def block_entities(self) -> BlockEntities:
        return BlockEntities(self._native)


class Selection:
    """A selection of cells and entities, updated by its own transforms."""

    def __init__(self, native):
        self._native = native

    @property
    def bounds(self) -> Bounds:
        return Bounds._from_native(self._native.bounds())

    def select(self, *, block: str | None = None, states: Mapping | None = None):
        if block is None and not states:
            raise ValueError("Supply a block ID or state filter")
        return Selection(self._native.select(block, _properties(states or {})))

    def fill(self, value: Block):
        self._native.fill(value.id, dict(value.states))
        return self

    def replace(self, identifier: str, value: Block):
        self.select(block=identifier).fill(value)
        return self

    def patch(self, **states):
        self._native.patch(_properties(states))
        return self

    def delete(self):
        self._native.delete()
        return self

    def move(self, *, offset: Position, replace=False):
        self._native.move_by(offset, replace)
        return self

    def rotate(self, *, axis="y", steps=1, pivot=None, replace=False):
        self._native.rotate(axis, steps, pivot, replace)
        return self

    def flip(self, *, axis: str, center=None, replace=False):
        self._native.flip(axis, center, replace)
        return self

    def duplicate(self, *, offset: Position, replace=False):
        return Selection(self._native.duplicate(offset, replace))

    def copy(self) -> Fragment:
        return Fragment(self._native.copy())

    def counts(self) -> dict[str, int]:
        return self._native.counts()

    def describe_layer(self, *, y: int) -> str:
        """Bounded text grid followed by a block-state legend; z increases downward."""
        cells = self._native.layer(y)
        if not cells:
            return f"No selected cells at y={y}."
        states = sorted({state for _, state in cells if state != "minecraft:air"})
        legend = {state: str(i + 1) for i, state in enumerate(states)}
        lookup = {tuple(p): legend.get(state, ".") for p, state in cells}
        xs = [p[0] for p, _ in cells]
        zs = [p[2] for p, _ in cells]
        width = max(1, len(str(len(states))))
        rows = [f"y={y}; x={min(xs)}..{max(xs)}, z={min(zs)}..{max(zs)}"]
        for z in range(min(zs), max(zs) + 1):
            rows.append(
                " ".join(
                    lookup.get((x, y, z), "-").rjust(width) for x in range(min(xs), max(xs) + 1)
                )
            )
        rows += [". = air; - = unselected"]
        rows += [f"{legend[state]} = {state}" for state in states]
        return "\n".join(rows)


class Fragment:
    def __init__(self, native):
        self._native = native

    @property
    def size(self) -> Position:
        return tuple(self._native.size())


def item(identifier: str, *, count=1, components: str | None = None) -> tuple[str, int, str | None]:
    """Describe an inventory item; optional components use typed SNBT."""
    if isinstance(count, bool) or not isinstance(count, int):
        raise TypeError("Item count must be an integer")
    return (identifier, count, components)


@dataclass(frozen=True)
class _Mob:
    id: str
    persistent: bool
    nbt: str | None


def mob(identifier: str, *, persistent=True, nbt: str | None = None):
    if not isinstance(persistent, bool):
        raise TypeError("Mob persistence must be a boolean")
    return _Mob(identifier, persistent, nbt)


@dataclass(frozen=True)
class Entity:
    reference: int
    position: tuple[float, float, float]
    nbt: str


class Entities:
    def __init__(self, native):
        self._native = native

    def add(self, value: _Mob, *, at: tuple[float, float, float]) -> int:
        return self._native.entity_add(value.id, value.persistent, value.nbt, at)

    def get(self, reference: int) -> Entity:
        position, snbt = self._native.entity_get(reference)
        return Entity(reference, tuple(position), snbt)

    def update(self, reference: int, *, position=None, nbt=None):
        """Replace supplied position or full NBT; omitted fields remain unchanged."""
        self._native.entity_update(reference, position, nbt)

    def remove(self, reference: int):
        self._native.entity_remove(reference)

    def __iter__(self):
        return iter(self._native.entity_list())


class BlockEntities:
    def __init__(self, native):
        self._native = native

    def get(self, at: Position) -> str | None:
        return self._native.block_entity_get(at)

    def set(self, at: Position, nbt: str):
        """Set a typed SNBT compound, including its compatible block-entity id."""
        self._native.block_entity_set(at, nbt)

    def remove(self, at: Position):
        self._native.block_entity_remove(at)
