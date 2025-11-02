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
	@echo "  test         - Run all tests (single-threaded to prevent FD exhaustion)"
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

# Run tests (single-threaded to prevent Ditto file descriptor exhaustion)
test: clean-ditto
	@echo "Running tests (single-threaded)..."
	@echo "Note: Tests use isolated temp directories, but run single-threaded to"
	@echo "      prevent file descriptor exhaustion from too many concurrent Ditto instances"
	cargo test -- --test-threads=1

# Run E2E integration tests
test-e2e: clean-ditto
	@echo "Running E2E integration tests..."
	@if [ ! -f .env ]; then \
		echo "⚠️  Warning: .env file not found. Ditto tests may be skipped."; \
		echo "   Create .env with DITTO_APP_ID, DITTO_OFFLINE_TOKEN, DITTO_SHARED_KEY"; \
	fi
	cd cap-protocol && export $$(grep -v '^#' ../.env | xargs) && cargo test --test squad_formation_e2e -- --nocapture --test-threads=1

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
	@echo "3. Running tests (single-threaded)..."
	@cargo test -- --test-threads=1
	@echo ""
	@echo "✅ All pre-commit checks passed!"

# CI pipeline: check formatting without modifying, then clippy and test
ci: clean-ditto
	@echo "Running CI pipeline..."
	@echo "1. Checking code formatting..."
	@cargo fmt --all -- --check
	@echo "2. Running clippy..."
	@cargo clippy --all-targets --all-features -- -D warnings
	@echo "3. Running tests (single-threaded)..."
	@cargo test -- --test-threads=1
	@echo ""
	@echo "✅ CI pipeline passed!"

# Full workflow: clean, build, and check
all: clean build check
	@echo ""
	@echo "✅ All tasks completed successfully!"
