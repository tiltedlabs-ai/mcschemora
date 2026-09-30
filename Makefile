.PHONY: setup rebuild format lint example

setup:
	uv sync --all-packages

rebuild:
	uv sync --all-packages --reinstall-package schemora

format:
	cargo fmt --all
	uv run --all-packages ruff format bindings/python/python examples

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	uv run --all-packages ruff format --check bindings/python/python examples
	uv run --all-packages ruff check bindings/python/python examples

example:
	uv run --all-packages python examples/build.py
	uv run --all-packages python examples/inspect_files.py
	uv run --all-packages python examples/check_edits.py
