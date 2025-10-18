.PHONY: help check fmt clippy test ci-check clean

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Available targets:'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  %-15s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

check: ## Run cargo check
	cargo check

fmt: ## Check code formatting
	cargo fmt -- --check

clippy: ## Run clippy with warnings as errors (exactly as CI does)
	cargo clippy --all-targets -- -D warnings

test: ## Run all tests
	cargo test --all-targets

ci-check: fmt clippy test ## Run all CI checks locally (format, clippy, tests)
	@echo "✅ All CI checks passed!"

clean: ## Clean build artifacts
	cargo clean
