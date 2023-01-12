.PHONY: clippy test

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test

benchmarks:
	cargo criterion --bench benchmark
