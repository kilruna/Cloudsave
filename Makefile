.PHONY: ci flatpak-build sync

ci:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --locked -- -D warnings
	cargo nextest run --workspace --locked
	cargo deny check

sync:
	cargo run -p cloudsave-cli --locked -- sync

flatpak-build:
	flatpak-builder --user --force-clean _build flatpak/org.cloudsave.Cloudsave.yml
