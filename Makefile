.PHONY: help setup build run dev test lint smoke docker docker-run docker-stop fmt clean install check-config

help: ## Show this help
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-14s %s\n", $$1, $$2}'

setup: ## Locate/download KataGo + network, write configs, build
	./setup.sh

build: ## Release build
	cargo build --release

run: build ## Run the release binary
	./target/release/katago-server

dev: ## Run with debug logging
	RUST_LOG=debug,katago_server=trace cargo run

test: ## Unit + integration tests (needs python3 for the fake KataGo)
	cargo test

lint: ## fmt check, clippy (deny warnings), cargo-deny if installed
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings
	@command -v cargo-deny >/dev/null 2>&1 && cargo deny check || echo "cargo-deny not installed, skipping"

smoke: ## Smoke test a running server (KATAGO_SERVER_URL or localhost:2718)
	./test.sh

check-config: ## Print the effective configuration
	cargo run --quiet -- check-config

fmt: ## Format code
	cargo fmt --all

docker: ## Build the CPU image locally
	docker build --target cpu -t katago-server:cpu .

docker-run: ## Start with docker compose
	docker compose up -d

docker-stop: ## Stop docker compose
	docker compose down

clean: ## Remove build artifacts and downloaded files
	cargo clean
	rm -f katago *.bin.gz

install: build ## Install binary and systemd unit (needs sudo)
	sudo install -m 0755 target/release/katago-server /usr/local/bin/katago-server
	sudo install -d /opt/katago-server
	sudo install -m 0644 katago-server.service /etc/systemd/system/katago-server.service
	sudo systemctl daemon-reload
	@echo "Edit /etc/systemd/system/katago-server.service (User/Group) and /opt/katago-server/config.toml, then: sudo systemctl enable --now katago-server"
