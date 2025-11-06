.PHONY: help check fmt fmt-fix clippy test ci-check clean openapi coverage

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Available targets:'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  %-15s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

check: ## Run cargo check
	cargo check

fmt: ## Check code formatting (same as CI)
	cargo fmt -- --check

fmt-fix: ## Auto-fix code formatting issues
	cargo fmt

clippy: ## Run clippy with warnings as errors (exactly as CI does)
	cargo clippy --all-targets -- -D warnings

test: ## Run all tests
	cargo test --all-targets

openapi: ## Generate openapi.json spec file
	cargo run --bin generate-openapi

ci-check: fmt clippy test openapi ## Run all CI checks locally (format, clippy, tests, openapi)
	@echo "✅ All CI checks passed!"

clean: ## Clean build artifacts
	cargo clean

coverage: ## Generate test coverage report (excludes runtime/startup files)
	cargo llvm-cov --all-targets --ignore-filename-regex 'src/(main|app|config|scheduler)\.rs$$|src/db/pool\.rs$$'

coverage-lcov: ## Generate lcov.info for detailed coverage analysis
	cargo llvm-cov --all-targets --ignore-filename-regex 'src/(main|app|config|scheduler)\.rs$$|src/db/pool\.rs$$' --lcov --output-path lcov.info
