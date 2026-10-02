.PHONY: check ci test docker-build docker-run docker-stop

check:
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test

ci: check

docker-build:
	docker build -t document-server:latest .

docker-run:
	docker run --rm -p 8090:8090 -e INTERNAL_API_KEY=$$(openssl rand -hex 32) --name document-server-test document-server:latest

docker-stop:
	docker stop document-server-test || true

