.PHONY: setup rebuild format lint docs docs-check docs-wasm

setup:
	uv sync --all-packages

rebuild:
	uv sync --all-packages --reinstall-package mcschemora

docs-wasm:
	npm --prefix bindings/wasm run build

docs: rebuild docs-wasm
	uv run --all-packages python scripts/generate_docs.py

docs-check: rebuild docs-wasm
	uv run --all-packages python scripts/generate_docs.py --check

format:
	cargo fmt --all
	uv run --all-packages ruff format bindings/python/python scripts

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	uv run --all-packages ruff format --check bindings/python/python scripts
	uv run --all-packages ruff check bindings/python/python scripts
