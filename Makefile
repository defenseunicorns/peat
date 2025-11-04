.PHONY: help clean clean-ditto build test test-e2e fmt clippy check all pre-commit ci

# Default target
help:
	@echo "CAP Protocol Development Makefile"
	@echo ""
	@echo "Available targets:"
	@echo "  help         - Show this help message"
	@echo "  clean        - Remove build artifacts and Ditto directories"
	@echo "  clean-ditto  - Remove Ditto persistence directories only"
	@echo "  build        - Build all crates"
	@echo "  test         - Run all tests serially (due to Ditto resource usage)"
	@echo "  test-e2e     - Run E2E integration tests for Squad Formation"
	@echo "  fmt          - Format all code with cargo fmt"
	@echo "  clippy       - Run clippy linter"
	@echo "  check        - Run fmt + clippy + test"
	@echo "  pre-commit   - Run all checks before committing (fmt + clippy + test)"
	@echo "  ci           - Run full CI pipeline (fmt check + clippy + test)"
	@echo "  all          - Clean, build, and run all checks"

# Clean build artifacts and Ditto directories
clean: clean-ditto
	@echo "Cleaning build artifacts..."
	cargo clean

# Remove Ditto persistence directories
clean-ditto:
	@echo "Removing Ditto persistence directories..."
	@find . -type d -name ".ditto*" -exec rm -rf {} + 2>/dev/null || true
	@echo "Ditto directories cleaned"

# Build all crates
build:
	@echo "Building all crates..."
	cargo build

# Run tests (serial execution due to Ditto resource usage)
# NOTE: Most tests use real Ditto instances and must run serially to avoid FD exhaustion
test: clean-ditto
	@echo "Running tests serially (single-threaded due to Ditto resource usage)..."
	@if [ -f .env ]; then \
		export $$(grep -v '^#' .env | xargs) && cargo test -- --test-threads=1; \
	else \
		cargo test -- --test-threads=1; \
	fi

# Run E2E integration tests
test-e2e: clean-ditto
	@echo "Running E2E integration tests..."
	@if [ ! -f .env ]; then \
		echo "⚠️  Warning: .env file not found. Ditto tests may be skipped."; \
		echo "   Create .env with DITTO_APP_ID, DITTO_OFFLINE_TOKEN, DITTO_SHARED_KEY"; \
	fi
	cd cap-protocol && export $$(grep -v '^#' ../.env | xargs) && cargo test --test squad_formation_e2e -- --nocapture

# Format all code
fmt:
	@echo "Formatting code..."
	cargo fmt --all

# Run clippy linter
clippy:
	@echo "Running clippy..."
	cargo clippy --all-targets --all-features -- -D warnings

# Quick check: fmt + clippy + test
check: fmt clippy test

# Pre-commit hook: run all checks
pre-commit: clean-ditto
	@echo "Running pre-commit checks..."
	@echo "1. Formatting code..."
	@cargo fmt --all
	@echo "2. Running clippy..."
	@cargo clippy --all-targets --all-features -- -D warnings
	@echo "3. Running tests serially (due to Ditto resource usage)..."
	@if [ -f .env ]; then \
		export $$(grep -v '^#' .env | xargs) && cargo test -- --test-threads=1; \
	else \
		cargo test -- --test-threads=1; \
	fi
	@echo ""
	@echo "✅ All pre-commit checks passed!"

# CI pipeline: check formatting without modifying, then clippy and test
ci: clean-ditto
	@echo "Running CI pipeline..."
	@echo "1. Checking code formatting..."
	@cargo fmt --all -- --check
	@echo "2. Running clippy..."
	@cargo clippy --all-targets --all-features -- -D warnings
	@echo "3. Running tests serially (due to Ditto resource usage)..."
	@if [ -f .env ]; then \
		export $$(grep -v '^#' .env | xargs) && cargo test -- --test-threads=1; \
	else \
		cargo test -- --test-threads=1; \
	fi
	@echo ""
	@echo "✅ CI pipeline passed!"

# Full workflow: clean, build, and check
all: clean build check
	@echo ""
	@echo "✅ All tasks completed successfully!"
