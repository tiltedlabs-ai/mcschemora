"""Minecraft schematic authoring, editing, conversion, and rendering.

Create or load a Schematic, then edit its regions and selections. Region
operations use local coordinates; rendering filters use schematic-global coordinates.
"""

from __future__ import annotations

import json
from collections.abc import Iterable, Iterator, Mapping, Sequence
from dataclasses import dataclass, field
from functools import lru_cache
from importlib.metadata import version as _package_version
from os import PathLike, fsync, replace
from pathlib import Path
from tempfile import NamedTemporaryFile
from types import MappingProxyType
from typing import TypeAlias

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
    "RepairChange",
    "RepairReport",
    "block",
    "water_source",
    "bed",
    "door",
    "sign",
    "chest",
    "item",
    "mob",
]
__version__ = _package_version("mcschemora")
Position: TypeAlias = tuple[int, int, int]
FloatPosition: TypeAlias = tuple[float, float, float]
PropertyValue: TypeAlias = str | int | bool
AxisRange: TypeAlias = int | tuple[int, int] | list[int] | None
JsonValue: TypeAlias = bool | int | float | str | list["JsonValue"] | dict[str, "JsonValue"] | None
_Path: TypeAlias = str | PathLike[str]
_Item: TypeAlias = tuple[str, int, str | None]


def _position(value: Sequence[int]) -> Position:
    return value[0], value[1], value[2]


def _properties(values: Mapping[str, PropertyValue]) -> dict[str, str]:
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
    """Immutable block description.

    Use block() to normalize an identifier and its properties.

    Attributes:
        id: Namespaced Minecraft block identifier.
        properties: Read-only Minecraft property names and values.
    """

    id: str
    _properties: tuple[tuple[str, str], ...] = ()
    _hash: int = field(init=False, repr=False, compare=False)

    def __post_init__(self) -> None:
        object.__setattr__(self, "_hash", hash((self.id, self._properties)))

    def __hash__(self) -> int:
        return self._hash

    def __reduce__(self) -> tuple[type[Block], tuple[str, tuple[tuple[str, str], ...]]]:
        return type(self), (self.id, self._properties)

    @property
    def properties(self) -> Mapping[str, str]:
        """The read-only mapping of Minecraft property names to string values."""
        return MappingProxyType(dict(self._properties))

    def __str__(self) -> str:
        """Returns the full block-state string with properties in sorted order."""
        suffix = ",".join(f"{k}={v}" for k, v in self._properties)
        return self.id + (f"[{suffix}]" if suffix else "")

    @classmethod
    def _from_native(cls, value: tuple[str, dict[str, str]]) -> Block:
        name, properties = value
        return cls(name, tuple(sorted(properties.items())))


def block(identifier: str, **properties: PropertyValue) -> Block:
    """Creates a block description, adding the minecraft namespace if omitted.

    Args:
        identifier: Minecraft block identifier.
        **properties: Minecraft properties as strings, integers, or booleans.

    Returns:
        An immutable Block. Catalog validation occurs when it is placed.
    """
    if not isinstance(identifier, str):
        raise TypeError("Block identifier must be a string")
    if ":" not in identifier:
        identifier = "minecraft:" + identifier
    return Block(identifier, tuple(sorted(_properties(properties).items())))


def water_source() -> Block:
    """Returns a water block with level=0."""
    return block("water", level=0)


@dataclass(frozen=True)
class Bounds:
    """A box with an inclusive start and exclusive upper bounds.

    Attributes:
        start: Minimum local cell coordinates.
        size: Cell counts along X, Y, and Z.
    """

    start: Position
    size: Position

    @classmethod
    def _from_native(cls, value: tuple[list[int], list[int]]) -> Bounds:
        start, size = value
        return cls(_position(start), _position(size))


@dataclass(frozen=True)
class RepairChange:
    """A repaired block, identified by region and local position.

    Attributes:
        region: Name of the containing region.
        position: Region-local cell coordinates.
        before: Block state before repair.
        after: Block state after repair.
    """

    region: str
    position: Position
    before: Block
    after: Block


@dataclass(frozen=True)
class RepairReport:
    """Changes applied by repair and cases skipped because their context is unknown.

    Attributes:
        changes: Immutable snapshots of changed blocks.
        skipped: Reasons why eligible blocks could not be repaired.
    """

    changes: tuple[RepairChange, ...] = ()
    skipped: tuple[str, ...] = ()

    @property
    def changed(self) -> int:
        """The number of blocks changed."""
        return len(self.changes)


@dataclass(frozen=True)
class Report:
    """Issues found during game-rule validation or export preflight.

    Attributes:
        errors: Problems that prevent validation or export.
        losses: Data omissions requiring explicit acceptance for export.
        warnings: Unstable states or other advisory messages.
        unknown: Checks that lack surrounding blocks or required game data.
    """

    errors: tuple[str, ...] = ()
    losses: tuple[str, ...] = ()
    warnings: tuple[str, ...] = ()
    unknown: tuple[str, ...] = ()

    @property
    def issues(self) -> tuple[str, ...]:
        """All messages in error, loss, warning, then unknown order."""
        return self.errors + self.losses + self.warnings + self.unknown

    @property
    def ok(self) -> bool:
        """Whether errors, losses, and unknown results are absent; warnings are allowed."""
        return not (self.errors or self.losses or self.unknown)

    def __str__(self) -> str:
        """Returns labeled issues, or "No issues found." for an empty report."""
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
    """Shared access to pinned Java catalogs and cached rendering assets."""

    def __init__(self, cache_dir: _Path | None = None, *, offline: bool = False) -> None:
        """Creates a catalog provider without preloading catalogs or visuals.

        Args:
            cache_dir: Cache directory; None uses the platform default.
            offline: Whether to require cached data and disable downloads.
        """
        directory = None if cache_dir is None else Path(cache_dir).expanduser().resolve()
        self._native = _core.MinecraftData(directory, offline)

    @property
    def cache_dir(self) -> Path:
        """The resolved directory used for cached catalogs and rendering assets."""
        return Path(self._native.cache_dir())

    @property
    def versions(self) -> tuple[str, ...]:
        """Available Java catalog versions at or above the 1.13 minimum."""
        return tuple(self._native.versions())

    def load(self, version: str = "latest") -> str:
        """Loads a catalog and returns its resolved Java version string.

        Args:
            version: Java version, or latest for the newest supported release.

        Raises:
            ValueError: If the version is unsupported or its catalog cannot be loaded.
        """
        return self._native.load(version)

    def dataset(self, version: str, kind: str) -> JsonValue:
        """Loads and returns a parsed catalog dataset for a Java version.

        Args:
            version: Java version, or latest for the newest supported release.
            kind: blocks, items, entities, or blockCollisionShapes.

        Returns:
            The dataset's JSON value.
        """
        dataset: JsonValue = json.loads(self._native.dataset(version, kind))
        return dataset

    def load_visuals(self, version: str = "latest") -> Path:
        """Prepares rendering assets and returns their cache directory.

        Args:
            version: Java version, or latest. All versions use the 1.21.1 visual bundle.

        Raises:
            ValueError: If the version is unsupported or the assets cannot be prepared.
        """
        return Path(self._native.load_visuals(version))


@lru_cache(maxsize=1)
def _default_data() -> MinecraftData:
    return MinecraftData()


def _source(data: MinecraftData | None) -> _core.MinecraftData:
    return (data if data is not None else _default_data())._native


class Registry:
    """Block schemas for a schematic's version, obtained through its registry."""

    def __init__(self, schematic: Schematic) -> None:
        self._schematic = schematic

    def describe(self, identifier: str) -> dict[str, JsonValue]:
        """Returns a block schema for this schematic's Minecraft version.

        Args:
            identifier: Minecraft block identifier, with an optional namespace.

        Returns:
            A dictionary with id, properties, version, and numeric_id.

        Raises:
            ValueError: If the block is unknown or the schematic has no catalog.
        """
        description: dict[str, JsonValue] = json.loads(self._schematic._native.describe(identifier))
        return description


def _axis_range(value: AxisRange, axis: str) -> tuple[int, int] | None:
    if value is None:
        return None
    if type(value) is int:
        value = (value, value)
    if (
        not isinstance(value, (tuple, list))
        or len(value) != 2
        or any(type(v) is not int for v in value)
    ):
        raise ValueError(f"{axis} must be an integer or an inclusive pair of integers")
    if value[0] > value[1]:
        raise ValueError(f"{axis} range start exceeds end")
    if any(not -(2**31) <= v < 2**31 for v in value):
        raise ValueError(f"{axis} coordinates must fit signed 32-bit integers")
    return value[0], value[1]


class Schematic:
    """A versioned schematic containing named regions and metadata.

    Obtain a schematic with create(), load(), or from_bytes(). Its region handles
    and selections edit the same schematic.
    """

    def __init__(self, native: _core.Schematic) -> None:
        self._native = native

    @classmethod
    def create(
        cls, *, edition: str = "java", version: str = "latest", data: MinecraftData | None = None
    ) -> Schematic:
        """Creates an empty schematic with a main region and a loaded catalog.

        Args:
            edition: Minecraft edition; new authoring currently supports java.
            version: Java version, or latest for the newest supported release.
            data: Catalog provider; None uses the shared default provider.

        Returns:
            A new Schematic.

        Raises:
            ValueError: If the edition or version is unsupported or loading fails.
        """
        return cls(_core.Schematic(edition, version, _source(data)))

    @classmethod
    def load(
        cls,
        path: _Path,
        *,
        format: str | None = None,
        data: MinecraftData | None = None,
        version: str | None = None,
        origin: Position | None = None,
        palette: Mapping[str, Block] | None = None,
    ) -> Schematic:
        """Reads a schematic file, preserving its edition and Minecraft version.

        Args:
            path: Input file path.
            format: Codec name; None infers it from the extension, with .wiki
                selecting blueprint.
            data: Catalog provider; None uses the shared default provider.
            version: Explicit Java version for blueprint import only.
            origin: Schematic-global origin for blueprint import only.
            palette: Blueprint symbols mapped to explicit Block descriptions.

        Returns:
            A Schematic with any import notices in import_diagnostics.

        Raises:
            OSError: If the file cannot be read.
            ValueError: If decoding fails or import options are unsupported.
        """
        path = Path(path)
        return cls.from_bytes(
            path.read_bytes(),
            format=format or ("blueprint" if path.suffix == ".wiki" else path.suffix.lstrip(".")),
            data=data,
            version=version,
            origin=origin,
            palette=palette,
        )

    @classmethod
    def from_bytes(
        cls,
        content: bytes,
        *,
        format: str,
        data: MinecraftData | None = None,
        version: str | None = None,
        origin: Position | None = None,
        palette: Mapping[str, Block] | None = None,
    ) -> Schematic:
        """Decodes schematic bytes without changing their Minecraft version.

        Args:
            content: Encoded schematic, or UTF-8 text for snbt and blueprint.
            format: schem, litematic, nbt, snbt, mcstructure, or blueprint.
            data: Catalog provider; None uses the shared default provider.
            version: Explicit Java version required for blueprint import only.
            origin: Schematic-global origin for blueprint import only; defaults to (0, 0, 0).
            palette: Blueprint symbols mapped to explicit Block descriptions.

        Returns:
            A Schematic with any import notices in import_diagnostics.

        Raises:
            ValueError: If decoding fails or import options are unsupported.
        """
        block_states = {}
        for symbol, state in (palette or {}).items():
            if not isinstance(state, Block):
                raise TypeError("palette values must be Block objects")
            block_states[symbol] = str(state)
        return cls(
            _core.Schematic.from_bytes(
                content, format, _source(data), version, origin, block_states
            )
        )

    def to_bytes(
        self,
        *,
        format: str,
        version: str | None = None,
        allow_loss: bool = False,
        flatten: bool = False,
    ) -> bytes:
        """Encodes the schematic, requiring explicit acceptance of reported losses.

        Args:
            format: schem, litematic, nbt, snbt, mcstructure, or blueprint.
            version: Target Minecraft Java version; None preserves the current version.
            allow_loss: Whether to accept omissions reported by check_export().
                Blocking errors still prevent export.
            flatten: Whether to merge regions for a single-region format. Overlapping
                bounds are rejected, and loss of region boundaries is reported.

        Returns:
            The encoded bytes.

        Raises:
            ValueError: If export has blocking errors or unaccepted losses.
        """
        return self._native.to_bytes(format, allow_loss, flatten, version)

    def save(
        self,
        path: _Path,
        *,
        format: str | None = None,
        version: str | None = None,
        allow_loss: bool = False,
        flatten: bool = False,
    ) -> None:
        """Encodes the schematic and atomically replaces the destination file.

        Args:
            path: Output file path.
            format: Codec name; None infers it from the extension, with .wiki
                selecting blueprint.
            version: Target Minecraft Java version; None preserves the current version.
            allow_loss: Whether to accept reported data omissions. Blocking errors
                still prevent export.
            flatten: Whether to merge regions for a single-region format.

        Raises:
            OSError: If the output file cannot be written.
            ValueError: If export has blocking errors or unaccepted losses.
        """
        path = Path(path)
        data = self.to_bytes(
            format=format or ("blueprint" if path.suffix == ".wiki" else path.suffix.lstrip(".")),
            version=version,
            allow_loss=allow_loss,
            flatten=flatten,
        )
        temporary = None
        try:
            with NamedTemporaryFile(
                dir=path.parent, prefix=f".{path.name}.", delete=False
            ) as output:
                temporary = Path(output.name)
                output.write(data)
                output.flush()
                fsync(output.fileno())
            replace(temporary, path)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)

    def region(self, name: str = "main") -> Region:
        """Returns an editing handle for an existing named region.

        Args:
            name: Region name.

        Raises:
            ValueError: If the region does not exist.
        """
        return Region(self._native.region(name))

    def export_glb(
        self,
        path: _Path,
        *,
        region: str | None = None,
        x: AxisRange = None,
        y: AxisRange = None,
        z: AxisRange = None,
    ) -> tuple[str, ...]:
        """Writes textured geometry to a binary glTF file.

        Args:
            path: Output file path, replaced if it exists.
            region: Region name; None selects all regions.
            x: Global X coordinate or inclusive (minimum, maximum) pair; None keeps all.
            y: Global Y coordinate or inclusive (minimum, maximum) pair; None keeps all.
            z: Global Z coordinate or inclusive (minimum, maximum) pair; None keeps all.

        Returns:
            Diagnostics describing visual approximations.
        """
        content, diagnostics = self._native.glb(
            region, [_axis_range(value, axis) for axis, value in zip("xyz", (x, y, z), strict=True)]
        )
        Path(path).write_bytes(content)
        return tuple(diagnostics)

    def export_blueprint(
        self,
        path: _Path,
        *,
        name: str = "Blueprint",
        region: str | None = None,
        y: AxisRange = None,
        rotation: int = 0,
        sprites: Mapping[str, str] | None = None,
    ) -> tuple[str, ...]:
        """Writes Minecraft Wiki layered-blueprint markup using block sprites.

        Args:
            path: Output UTF-8 file path, replaced if it exists.
            name: Blueprint title.
            region: Region name; None selects all regions.
            y: Global Y coordinate or inclusive pair; None keeps all layers.
            rotation: Number of quarter turns about Y.
            sprites: Block IDs or full state strings mapped to wiki sprite identifiers.

        Returns:
            Diagnostics for state properties, attached data, and other omissions.
        """
        if type(rotation) is not int:
            raise TypeError("rotation must be an integer number of quarter turns")
        content, diagnostics = self._native.blueprint(
            name, region, _axis_range(y, "y"), rotation, dict(sprites or {})
        )
        Path(path).write_text(content, encoding="utf-8")
        return tuple(diagnostics)

    def export_sprites(
        self,
        path: _Path,
        *,
        view: str = "top",
        cell_size: int = 32,
        grid: bool = False,
        entities: bool = True,
        region: str | None = None,
        x: AxisRange = None,
        y: AxisRange = None,
        z: AxisRange = None,
        sprites: Mapping[str, str] | None = None,
    ) -> tuple[str, ...]:
        """Writes a flat PNG diagram using bundled Minecraft Wiki sprites.

        Args:
            path: Output file path, replaced if it exists.
            view: top, bottom, north, south, east, or west.
            cell_size: Pixels per grid cell, from 1 through 128.
            grid: Whether to draw grid lines.
            entities: Whether to include free-entity icons.
            region: Region name; None selects all regions.
            x: Global X coordinate or inclusive pair; None keeps all.
            y: Global Y coordinate or inclusive pair; None keeps all.
            z: Global Z coordinate or inclusive pair; None keeps all.
            sprites: Block IDs or full state strings mapped to wiki sprite identifiers.

        Returns:
            Diagnostics describing omitted state details and sprite approximations.
        """
        if type(cell_size) is not int or not 1 <= cell_size <= 128:
            raise ValueError("cell_size must be an integer between 1 and 128")
        content, diagnostics = self._native.sprites(
            region,
            [_axis_range(value, axis) for axis, value in zip("xyz", (x, y, z), strict=True)],
            view,
            (cell_size, grid, entities),
            dict(sprites or {}),
        )
        Path(path).write_bytes(content)
        return tuple(diagnostics)

    def export_png(
        self,
        path: _Path,
        *,
        size: tuple[int, int] | list[int] = (1024, 1024),
        view: str = "isometric",
        grid: bool = False,
        region: str | None = None,
        x: AxisRange = None,
        y: AxisRange = None,
        z: AxisRange = None,
    ) -> tuple[str, ...]:
        """Writes an automatically framed textured PNG with a transparent background.

        Args:
            path: Output file path, replaced if it exists.
            size: (width, height), each from 1 through 4096 pixels.
            view: isometric, top, bottom, north, south, east, or west. Cardinal names
                describe the viewer's location; side views keep Y-up vertical.
            grid: Whether to draw outlined block edges; entities are excluded.
            region: Region name; None selects all regions.
            x: Global X coordinate or inclusive pair; None keeps all.
            y: Global Y coordinate or inclusive pair; None keeps all.
            z: Global Z coordinate or inclusive pair; None keeps all.

        Returns:
            Diagnostics describing visual approximations. Rendering uses the shared
            Java 1.21.1 visual bundle.
        """
        if (
            not isinstance(size, (tuple, list))
            or len(size) != 2
            or any(type(v) is not int or not 1 <= v <= 4096 for v in size)
        ):
            raise ValueError("size must be (width, height), each between 1 and 4096")
        content, diagnostics = self._native.png(
            region,
            [_axis_range(value, axis) for axis, value in zip("xyz", (x, y, z), strict=True)],
            size,
            view,
            grid,
        )
        Path(path).write_bytes(content)
        return tuple(diagnostics)

    def add_region(self, name: str, *, origin: Position = (0, 0, 0)) -> Region:
        """Adds an empty region and returns its editing handle.

        Args:
            name: Nonempty, unique region name.
            origin: Region origin in schematic-global coordinates.

        Raises:
            ValueError: If the name is empty or already exists.
        """
        return Region(self._native.add_region(name, origin))

    @property
    def regions(self) -> tuple[str, ...]:
        """The names of the schematic's regions."""
        return tuple(self._native.regions())

    @property
    def import_diagnostics(self) -> tuple[str, ...]:
        """Notices about assumptions or omissions made while importing."""
        return tuple(self._native.import_diagnostics())

    @property
    def edition(self) -> str:
        """The schematic's Minecraft edition."""
        return self._native.info()[0]

    @property
    def version(self) -> str:
        """The schematic's Minecraft version string."""
        return self._native.info()[1]

    @property
    def data_version(self) -> int:
        """The numeric Minecraft data version stored in the schematic."""
        return self._native.info()[2]

    @property
    def metadata(self) -> str:
        """Schematic metadata as typed SNBT; assigning replaces the whole compound."""
        return self._native.metadata()

    @metadata.setter
    def metadata(self, snbt: str) -> None:
        """Replaces schematic metadata with a typed SNBT compound."""
        self._native.set_metadata(snbt)

    @property
    def registry(self) -> Registry:
        """Block schema access for the schematic's Minecraft version."""
        return Registry(self)

    def validate(self) -> Report:
        """Checks Java game rules without changing the schematic or simulating ticks.

        Returns:
            A Report with structural errors, unstable-state warnings, and unknown
            checks requiring surrounding blocks or unavailable game data.
        """
        errors, warnings, unknown = self._native.validate()
        return Report(errors=tuple(errors), warnings=tuple(warnings), unknown=tuple(unknown))

    def repair(self, *, rules: Sequence[str] | None = None) -> RepairReport:
        """Repairs neighbor-dependent connections and shapes in place.

        Uses the same expected-state calculations as validate(). All supported
        rules run by default. Neighbor lookups cross regions in schematic-global coordinates;
        unknown surrounding blocks cause a cell to be skipped. Isolated redstone
        dots remain dots. Power, facing, waterlogging, entities, and attached data
        are preserved. Loading never invokes repair automatically.

        Args:
            rules: Any subset of redstone, stairs, fences, panes, and walls.
                None enables all rules; an empty sequence changes nothing.

        Returns:
            A RepairReport containing before/after states and skipped reasons.

        Raises:
            ValueError: If a rule is unsupported, regions overlap, bounds are
                invalid, or repairs cannot converge. No changes are committed.
            TypeError: If rules is a string rather than a sequence of rule names.
        """
        if isinstance(rules, str):
            raise TypeError("rules must be a sequence of rule names")
        changes, skipped = self._native.repair(None if rules is None else list(rules))
        return RepairReport(
            tuple(
                RepairChange(
                    region,
                    _position(position),
                    Block._from_native(before),
                    Block._from_native(after),
                )
                for region, position, before, after in changes
            ),
            tuple(skipped),
        )

    def check_export(
        self, *, format: str, version: str | None = None, flatten: bool = False
    ) -> Report:
        """Checks conversion errors and losses without writing or changing the schematic.

        Args:
            format: Target codec name.
            version: Target Minecraft Java version; None preserves the current version.
            flatten: Whether to evaluate merging regions for a single-region format.

        Returns:
            A Report whose errors block export and whose losses need allow_loss=True.
        """
        errors, losses = self._native.check_export(format, flatten, version)
        return Report(tuple(errors), tuple(losses))


class Region:
    """An editing handle for a named region within a schematic.

    Cell coordinates are local to this region. Its origin locates those coordinates
    in schematic-global coordinates. Obtain a handle through Schematic.region() or add_region().
    """

    def __init__(self, native: _core.Region) -> None:
        self._native = native

    @property
    def bounds(self) -> Bounds:
        """The region's stored bounding box in local coordinates."""
        return Bounds._from_native(self._native.bounds())

    @property
    def origin(self) -> Position:
        """The schematic-global coordinates corresponding to this region's local (0, 0, 0)."""
        return _position(self._native.origin())

    def to_global(self, local: Position) -> Position:
        """Converts a region-local position to schematic-global coordinates by adding the origin.

        Args:
            local: Integer coordinates relative to this region's origin.

        Raises:
            ValueError: If the result exceeds signed 32-bit coordinates.
        """
        return _position(self._native.to_global(local))

    def to_local(self, global_position: Position) -> Position:
        """Converts a schematic-global position to region-local coordinates by subtracting the origin.

        Args:
            global_position: Integer coordinates in the schematic's shared coordinate space.

        Raises:
            ValueError: If the result exceeds signed 32-bit coordinates.
        """
        return _position(self._native.to_local(global_position))

    def get(self, at: Position) -> Block:
        """Returns the block at a position in this region.

        Args:
            at: Integer coordinates relative to the region's origin.

        Returns:
            The block at the position, or air if the cell is absent.
        """
        return Block._from_native(self._native.get(at))

    def get_all(self) -> dict[Position, Block]:
        """Returns a snapshot of non-air blocks keyed by region-local coordinates.

        Identical states share immutable Block objects. Changes to the returned
        dictionary do not edit the schematic. Block entities and entities are excluded.
        """
        return self._native.get_all(Block._from_native)

    def set(self, at: Position, content: Block | Fragment) -> None:
        """Writes a block or pastes a fragment, replacing destination cells.

        Args:
            at: Local cell coordinates; fragments are anchored at their minimum corner.
            content: Block description or independent Fragment. Fragments require
                matching editions and versions and preserve unrelated entities.
        """
        if isinstance(content, Fragment):
            self._native.set_fragment(at, content._native)
        elif isinstance(content, Block):
            self.set_many([(at, content)])
        else:
            raise TypeError("set() expects a Block or Fragment")

    def set_many(
        self, placements: Mapping[Position, Block] | Iterable[tuple[Position, Block]]
    ) -> None:
        """Validates and writes a batch of blocks atomically.

        Args:
            placements: Mapping of local positions to Blocks, or an iterable of
                (local position, Block) pairs. Later writes to the same position
                take precedence. Omitted positions are unchanged; write air to clear cells.

        Raises:
            ValueError: If a block, coordinate, or region-bound change is unsupported.
                No blocks are written when validation fails.
        """
        indices: dict[Block, int] = {}
        palette: list[tuple[str, dict[str, str]]] = []
        cells: list[tuple[Position, int]] = []
        entries = placements.items() if isinstance(placements, Mapping) else placements
        for at, value in entries:
            index = indices.get(value)
            if index is None:
                index = len(palette)
                indices[value] = index
                palette.append((value.id, dict(value._properties)))
            cells.append((at, index))
        self._native.set_many(palette, cells)

    def delete(self) -> None:
        """Clears blocks, attached data, and entities within this region's bounds.

        The named region, origin, and bounds remain in the schematic.
        """
        bounds = self.bounds
        self.select(start=bounds.start, size=bounds.size).delete()

    def patch(self, at: Position, **properties: PropertyValue) -> None:
        """Changes supplied properties of the block at a local cell position.

        Args:
            at: Local cell coordinates.
            **properties: Minecraft properties to replace; other properties are preserved.
        """
        self.select(start=at, size=(1, 1, 1)).patch(**properties)

    def select(self, *, start: Position, size: Position) -> Selection:
        """Returns a box selection that edits this region's current content.

        Args:
            start: Inclusive minimum local coordinates; negative values are allowed.
            size: Nonnegative cell counts along X, Y, and Z. Upper bounds are exclusive.

        Returns:
            A Selection including cells and entities within the box.
        """
        return Selection(self._native.select(start, size))

    def place(self, placement: _Placement, *, at: Position, replace: bool = False) -> None:
        """Validates and commits a placement recipe atomically.

        Args:
            placement: Placement returned by bed(), door(), sign(), or chest().
            at: Local anchor cell: the foot of a bed, lower half of a door, or the
                sign or chest block.
            replace: Whether to overwrite non-air blocks at recipe targets.

        Raises:
            ValueError: If the recipe is invalid for this version or targets are
                occupied with replace=False.
        """
        if not isinstance(placement, _Placement):
            raise TypeError("place() expects a bed, door, sign, or chest helper")
        self._native.place(placement, at, replace)

    @property
    def entities(self) -> Entities:
        """The manager for entities in this region."""
        return Entities(self._native)

    @property
    def block_entities(self) -> BlockEntities:
        """The manager for NBT attached to this region's blocks."""
        return BlockEntities(self._native)


class Selection:
    """A set of region-local cells and entities that reads current content.

    Filters resolve membership once. Transforms update this selection's coordinates;
    other selections retain theirs. Block filters exclude entities.
    """

    def __init__(self, native: _core.Selection) -> None:
        self._native = native

    @property
    def bounds(self) -> Bounds:
        """The selection's local bounding box."""
        return Bounds._from_native(self._native.bounds())

    def get_all(self) -> dict[Position, Block]:
        """Returns current non-air blocks at selected region-local coordinates.

        The dictionary is an independent snapshot, with identical states sharing
        immutable Block objects. Block entities and entities are excluded.
        """
        return self._native.get_all(Block._from_native)

    def select(
        self, *, block: str | None = None, properties: Mapping[str, PropertyValue] | None = None
    ) -> Selection:
        """Returns a new selection filtered by current block IDs and properties.

        Args:
            block: Minecraft block identifier; None accepts any block ID.
            properties: Required property values; None leaves properties unrestricted.

        Returns:
            A selection with fixed cell membership and no entities.

        Raises:
            ValueError: If neither a block ID nor a property filter is supplied.
        """
        if block is None and not properties:
            raise ValueError("Supply a block ID or property filter")
        return Selection(self._native.select(block, _properties(properties or {})))

    def fill(self, value: Block) -> Selection:
        """Fills selected cells with a block and returns self; entities are unchanged."""
        self._native.fill(value.id, dict(value.properties))
        return self

    def replace(self, identifier: str, value: Block) -> Selection:
        """Replaces selected blocks matching an identifier and returns self.

        Args:
            identifier: Minecraft block ID to match, regardless of properties.
            value: Replacement block description.
        """
        self.select(block=identifier).fill(value)
        return self

    def patch(self, **properties: PropertyValue) -> Selection:
        """Changes supplied properties on selected blocks and returns self.

        Args:
            **properties: Properties to replace; other properties and entities remain.
        """
        self._native.patch(_properties(properties))
        return self

    def delete(self) -> Selection:
        """Clears selected blocks, attached data, and selected entities; returns self."""
        self._native.delete()
        return self

    def move(self, *, offset: Position, replace: bool = False) -> Selection:
        """Moves selected content atomically, updates membership, and returns self.

        Args:
            offset: Local X, Y, and Z displacement.
            replace: Whether to overwrite occupied targets outside the source selection.

        Raises:
            ValueError: If a target is occupied with replace=False or movement is
                unsupported. Self-overlap is allowed; selected air clears targets.
        """
        self._native.move_by(offset, replace)
        return self

    def rotate(
        self,
        *,
        axis: str = "y",
        steps: int = 1,
        pivot: FloatPosition | None = None,
        replace: bool = False,
    ) -> Selection:
        """Rotates content atomically, updates membership, and returns self.

        Args:
            axis: Rotation axis; currently only y is supported.
            steps: Number of quarter turns. Positive steps turn north toward west.
            pivot: Local coordinates; None uses the bounding box's geometric center.
            replace: Whether to overwrite occupied targets outside the source selection.

        Raises:
            ValueError: If cells end off-grid, a target is occupied with replace=False,
                or block states or attached data cannot be transformed.
        """
        self._native.rotate(axis, steps, pivot, replace)
        return self

    def flip(self, *, axis: str, center: float | None = None, replace: bool = False) -> Selection:
        """Reflects content atomically, updates membership, and returns self.

        Args:
            axis: Reflection axis, x or z.
            center: Local plane coordinate; None uses the bounding box's center.
            replace: Whether to overwrite occupied targets outside the source selection.

        Raises:
            ValueError: If cells end off-grid, a target is occupied with replace=False,
                or block states or attached data cannot be transformed.
        """
        self._native.flip(axis, center, replace)
        return self

    def duplicate(self, *, offset: Position, replace: bool = False) -> Selection:
        """Copies content at an offset and returns a new destination selection.

        Args:
            offset: Local X, Y, and Z displacement.
            replace: Whether to overwrite occupied targets. Selected air clears targets.

        Returns:
            The copied selection. The source selection retains its coordinates;
            copied entities receive new references.
        """
        return Selection(self._native.duplicate(offset, replace))

    def copy(self) -> Fragment:
        """Returns an independent fragment anchored at the selection's minimum corner.

        Selected air and attached data are included; unselected cells are omitted.

        Raises:
            ValueError: If retained format data prevents copying the region.
        """
        return Fragment(self._native.copy())

    def counts(self) -> dict[str, int]:
        """Returns counts by full block-state string, excluding all air block types."""
        return self._native.counts()

    def describe_layer(self, *, y: int) -> str:
        """Returns a local Y layer as a text grid followed by a block-state legend.

        Args:
            y: Local Y coordinate. Z increases down the grid; . means air and - means
                unselected. An empty layer returns a descriptive message.

        Raises:
            ValueError: If the selection's X/Z area exceeds 65,536 cells.
        """
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
    """An independent copy of selected cells, attached data, and entities.

    Obtain one through Selection.copy() and paste it with Region.set(). Pasting
    requires matching Minecraft editions and versions.
    """

    def __init__(self, native: _core.Fragment) -> None:
        self._native = native

    @property
    def size(self) -> Position:
        """Cell counts along X, Y, and Z in the copied bounding box."""
        return _position(self._native.size())


def item(identifier: str, *, count: int = 1, components: str | None = None) -> _Item:
    """Creates an inventory item value for chest().

    Args:
        identifier: Minecraft item identifier.
        count: Item count, from 1 through 99 when placed.
        components: Optional typed SNBT compound; requires Java 1.20.5 or newer.

    Returns:
        An (identifier, count, components) tuple, validated on chest placement.
    """
    if isinstance(count, bool) or not isinstance(count, int):
        raise TypeError("Item count must be an integer")
    return (identifier, count, components)


@dataclass(frozen=True)
class _Mob:
    id: str
    persistent: bool
    nbt: str | None


def mob(identifier: str, *, persistent: bool = True, nbt: str | None = None) -> _Mob:
    """Creates a free-entity description for Region.entities.add().

    Args:
        identifier: Living-mob identifier supported by the target catalog.
        persistent: Whether to set the entity's PersistenceRequired flag.
        nbt: Optional typed SNBT compound; id and persistence are set from the
            supplied arguments when the entity is added.

    Returns:
        An entity description, validated against the target catalog when added.
    """
    if not isinstance(persistent, bool):
        raise TypeError("Mob persistence must be a boolean")
    return _Mob(identifier, persistent, nbt)


@dataclass(frozen=True)
class Entity:
    """An immutable snapshot of a entity returned by Entities.get().

    Attributes:
        reference: Schematic-local integer used to address the stored entity.
        position: Floating-point coordinates local to its region.
        nbt: The entity's full typed SNBT compound.
    """

    reference: int
    position: FloatPosition
    nbt: str


class Entities:
    """Access to a region's entities through schematic-local integer references."""

    def __init__(self, native: _core.Region) -> None:
        self._native = native

    def add(self, value: _Mob, *, at: FloatPosition) -> int:
        """Adds an entity and returns its schematic-local reference.

        Args:
            value: Entity description returned by mob().
            at: Floating-point coordinates local to the region.
        """
        return self._native.entity_add(value.id, value.persistent, value.nbt, at)

    def get(self, reference: int) -> Entity:
        """Returns an immutable snapshot of the entity with the given reference.

        Args:
            reference: Schematic-local reference of an entity in this region.

        Raises:
            ValueError: If the reference does not identify an entity in this region.
        """
        position, snbt = self._native.entity_get(reference)
        return Entity(reference, (position[0], position[1], position[2]), snbt)

    def update(
        self, reference: int, *, position: FloatPosition | None = None, nbt: str | None = None
    ) -> None:
        """Replaces supplied fields of an entity; omitted fields are preserved.

        Args:
            reference: Schematic-local reference of an entity in this region.
            position: New floating-point local coordinates; None preserves the position.
            nbt: Full typed SNBT compound including id; None preserves attached data.
        """
        self._native.entity_update(reference, position, nbt)

    def remove(self, reference: int) -> None:
        """Removes the entity identified by a schematic-local reference.

        Args:
            reference: Schematic-local reference of an entity in this region.

        Raises:
            ValueError: If the reference does not identify an entity in this region.
        """
        self._native.entity_remove(reference)

    def __iter__(self) -> Iterator[int]:
        """Iterates a snapshot of this region's entity references.

        Yields:
            Schematic-local integer references accepted by this manager.
        """
        return iter(self._native.entity_list())


class BlockEntities:
    """Access to typed NBT attached to blocks at region-local cell coordinates."""

    def __init__(self, native: _core.Region) -> None:
        self._native = native

    def get(self, at: Position) -> str | None:
        """Returns typed SNBT at a local cell position, or None if no data is attached."""
        return self._native.block_entity_get(at)

    def set(self, at: Position, nbt: str) -> None:
        """Replaces the NBT attached to a block at a local cell position.

        Args:
            at: Local coordinates of the owning block.
            nbt: Full typed SNBT compound with an id compatible with that block.

        Raises:
            ValueError: If parsing fails or id is missing or incompatible.
        """
        self._native.block_entity_set(at, nbt)

    def remove(self, at: Position) -> None:
        """Removes attached NBT at a local cell position; the owning block remains."""
        self._native.block_entity_remove(at)
