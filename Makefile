.PHONY: setup rebuild format lint

setup:
	uv sync --all-packages

rebuild:
	uv sync --all-packages --reinstall-package schemora

format:
	cargo fmt --all
	uv run --all-packages ruff format bindings/python/python

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	uv run --all-packages ruff format --check bindings/python/python
	uv run --all-packages ruff check bindings/python/python
