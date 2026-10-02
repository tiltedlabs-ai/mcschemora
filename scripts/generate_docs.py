import argparse
import ast
import inspect
import re
from pathlib import Path

import mcschemora

ROOT = Path(__file__).resolve().parents[1]
PYTHON_SOURCE = ROOT / "bindings/python/python/mcschemora/__init__.py"
NATIVE_SOURCE = ROOT / "bindings/python/src/lib.rs"
STUB_SOURCE = PYTHON_SOURCE.with_name("_core.pyi")


class Annotation:
    def __init__(self, text: str) -> None:
        self.text = text

    def __repr__(self) -> str:
        return self.text


def signature(value, stub: ast.FunctionDef | None = None) -> str:
    result = inspect.signature(value)
    annotations = {}
    if stub:
        arguments = [*stub.args.posonlyargs, *stub.args.args, *stub.args.kwonlyargs]
        annotations = {arg.arg: ast.unparse(arg.annotation) for arg in arguments if arg.annotation}
    parameters = []
    for parameter in result.parameters.values():
        if parameter.name in {"self", "cls"}:
            continue
        annotation = annotations.get(parameter.name, parameter.annotation)
        if isinstance(annotation, str):
            parameter = parameter.replace(annotation=Annotation(annotation))
        parameters.append(parameter)
    returns = ast.unparse(stub.returns) if stub and stub.returns else result.return_annotation
    if isinstance(returns, str):
        returns = Annotation(returns)
    result = result.replace(parameters=parameters, return_annotation=returns)
    if len(str(result)) <= 80:
        return str(result)
    lines = []
    for index, parameter in enumerate(parameters):
        if parameter.kind == inspect.Parameter.KEYWORD_ONLY and (
            index == 0
            or parameters[index - 1].kind
            not in {inspect.Parameter.KEYWORD_ONLY, inspect.Parameter.VAR_POSITIONAL}
        ):
            lines.append("    *,")
        lines.append(f"    {parameter},")
        if parameter.kind == inspect.Parameter.POSITIONAL_ONLY and (
            index == len(parameters) - 1
            or parameters[index + 1].kind != inspect.Parameter.POSITIONAL_ONLY
        ):
            lines.append("    /,")
    return "(\n" + "\n".join(lines) + "\n)" + str(result.replace(parameters=[]))[2:]


def docstring(value) -> str:
    text = inspect.getdoc(value)
    if not text:
        raise ValueError(f"Missing public docstring: {value}")
    sections = {"Args:", "Returns:", "Raises:", "Attributes:", "Yields:", "Examples:"}
    output = []
    section = None
    for line in text.splitlines():
        if line in sections:
            section = line[:-1]
            output.extend(["", f"**{section}**", ""])
        elif section in {"Args", "Raises", "Attributes"} and line.startswith("    "):
            entry = re.match(r"^    (\S+):\s*(.*)$", line)
            output.append(f"- `{entry[1]}`: {entry[2]}" if entry else "  " + line.strip())
        elif section == "Examples":
            output.append(line)
        else:
            output.append(line.strip())
    return re.sub(r"\n{3,}", "\n\n", "\n".join(output).strip())


def source_link(path: Path, line: int) -> str:
    return f"[Source](../../{path.relative_to(ROOT).as_posix()}#L{line})"


def python_reference() -> str:
    tree = ast.parse(PYTHON_SOURCE.read_text())
    nodes = {
        node.name: node for node in tree.body if isinstance(node, ast.ClassDef | ast.FunctionDef)
    }
    stubs = {
        node.name: node
        for node in ast.parse(STUB_SOURCE.read_text()).body
        if isinstance(node, ast.FunctionDef)
    }
    exports = [(name, getattr(mcschemora, name)) for name in mcschemora.__all__]
    exports.extend(
        (name, value)
        for name, value in vars(mcschemora).items()
        if inspect.isclass(value)
        and value.__module__ == mcschemora.__name__
        and not name.startswith("_")
        and name not in mcschemora.__all__
    )
    lines = [
        "# Python API",
        "",
        "Generated from public signatures and docstrings by `make docs`. Do not edit this page.",
        "",
        "Start with [getting started](../guides/getting-started.md) or the "
        "[runnable examples](../../examples/README.md).",
        "",
        "Arguments after `*` are keyword-only. Types and properties are documented below.",
        "",
        "## Contents",
        "",
        *[
            f"- [{name}](#{'' if inspect.isclass(value) else 'function-'}{name.lower()})"
            for name, value in exports
        ],
        "- [Type aliases](#type-aliases)",
        "",
    ]
    for name, value in exports:
        node = nodes.get(name)
        if node:
            source = source_link(PYTHON_SOURCE, node.lineno)
        else:
            match = re.search(rf"^fn {name}\(", NATIVE_SOURCE.read_text(), re.MULTILINE)
            if match is None:
                raise ValueError(f"Missing source for {name}")
            source = source_link(
                NATIVE_SOURCE, NATIVE_SOURCE.read_text()[: match.start()].count("\n") + 1
            )
        heading = name if inspect.isclass(value) else f"Function {name}"
        lines.extend([f"## {heading}", "", source, ""])
        if not inspect.isclass(value):
            lines.extend(["```python", name + signature(value, stubs.get(name)), "```", ""])
        lines.extend([docstring(value), ""])
        if not isinstance(node, ast.ClassDef):
            continue
        attributes = {
            key: annotation
            for key, annotation in getattr(value, "__annotations__", {}).items()
            if not key.startswith("_")
        }
        if attributes:
            lines.extend(
                [
                    "```python",
                    *[f"{name}.{key}: {annotation}" for key, annotation in attributes.items()],
                    "```",
                    "",
                ]
            )
        for member_node in node.body:
            if not isinstance(member_node, ast.FunctionDef):
                continue
            member_name = member_node.name
            if member_name.startswith("_") and member_name not in {
                "__init__",
                "__iter__",
                "__str__",
            }:
                continue
            if member_name == "__init__" and not ast.get_docstring(member_node):
                continue
            if any(
                isinstance(d, ast.Attribute) and d.attr == "setter"
                for d in member_node.decorator_list
            ):
                continue
            member = inspect.getattr_static(value, member_name)
            if isinstance(member, classmethod | staticmethod):
                member = member.__func__
            label = name if member_name == "__init__" else f"{name}.{member_name}"
            lines.extend(
                [
                    f"### {name}.{member_name}",
                    "",
                    source_link(PYTHON_SOURCE, member_node.lineno),
                    "",
                ]
            )
            if isinstance(member, property):
                returns = inspect.signature(member.fget).return_annotation
                lines.extend(
                    ["```python", f"{label}: {returns}", "```", "", docstring(member.fget), ""]
                )
                if member.fset:
                    lines.extend(["Writable property.", "", docstring(member.fset), ""])
            else:
                lines.extend(
                    ["```python", label + signature(member), "```", "", docstring(member), ""]
                )
    lines.extend(["## Type aliases", "", "```python"])
    for node in tree.body:
        if (
            isinstance(node, ast.AnnAssign)
            and isinstance(node.annotation, ast.Name)
            and node.annotation.id == "TypeAlias"
        ):
            lines.append(f"{ast.unparse(node.target)} = {ast.unparse(node.value)}")
    lines.extend(
        [
            "```",
            "",
            "`Placement` (`_Placement` in facade annotations) is the opaque recipe returned by `bed`, `door`, "
            "`chest`, and `sign`. `_Mob` is the entity description returned by `mob`.",
            "",
        ]
    )
    return "\n".join(lines)


def wasm_reference() -> str:
    path = ROOT / "bindings/wasm/pkg/mcschemora.d.ts"
    declarations = path.read_text()
    declarations = declarations[: declarations.index("export type InitInput")]
    declarations = re.sub(
        r"^/\* (?:tslint:disable|eslint-disable) \*/\r?\n?", "", declarations, flags=re.MULTILINE
    ).strip()
    return "\n".join(
        [
            "# WASM API",
            "",
            "Generated from wasm-bindgen's TypeScript declarations by `make docs`. Do not edit this page.",
            "",
            "See the [browser quickstart](../../bindings/wasm/README.md) for initialization, "
            "file import, and resource cleanup.",
            "",
            "The declarations below include the public classes and their source documentation.",
            "The complete declarations, including initialization types, are generated at "
            "`bindings/wasm/pkg/mcschemora.d.ts` during the browser build.",
            "",
            "```typescript",
            declarations,
            "```",
            "",
        ]
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    pages = {"python-api.md": python_reference(), "wasm-api.md": wasm_reference()}
    stale = []
    for name, content in pages.items():
        path = ROOT / "docs/reference" / name
        if args.check:
            if not path.exists() or path.read_text() != content:
                stale.append(path.relative_to(ROOT).as_posix())
        else:
            path.write_text(content)
            print(f"Generated {path.relative_to(ROOT)}")
    if stale:
        raise SystemExit("Outdated API documentation: " + ", ".join(stale) + ". Run make docs.")
    if args.check:
        print("API documentation is current.")


if __name__ == "__main__":
    main()
