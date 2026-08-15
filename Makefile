.PHONY: check ci test

check:
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test

ci: check
