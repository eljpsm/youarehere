.PHONY: build install test test-cli bench coverage coverage-html lint fmt hooks release

VERSION = $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml)

# Build the release binary.
build:
	cargo build --release

# Install youarehere onto PATH (~/.cargo/bin).
install:
	cargo install --path .

# Run everything: unit + CLI.
test:
	cargo test

# End-to-end binary tests (prompt output, init snippets).
test-cli:
	cargo test --test cli

# Measure release prompt latency in ordinary and tag-heavy repositories.
bench: build
	bash bench/prompt.sh

# Line and region coverage across all tests, printed per file.
coverage:
	cargo llvm-cov --all-targets

# Same, rendered as an annotated HTML report.
coverage-html:
	cargo llvm-cov --all-targets --open

# Clippy across all targets; warnings fail, locally and in CI alike.
lint:
	cargo clippy --all-targets -- --deny warnings

# Format the Rust source.
fmt:
	cargo fmt

# Install the git hooks from prek.toml. Run once per checkout.
hooks:
	prek install

# Tag the current commit with the Cargo.toml version and push the tag.
# CI then runs goreleaser and publishes the binaries.
release:
	git tag "v$(VERSION)"
	git push origin "v$(VERSION)"
