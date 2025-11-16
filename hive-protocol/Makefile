.PHONY: test test-fast test-one test-e2e clean pre-commit clippy fmt check coverage help

# Run all tests with proper Ditto cleanup
# Note: Tests now run in parallel! Each test gets an isolated temp directory automatically.
# Performance: ~6s vs ~267s with sequential execution (43x faster!)
test:
	@echo "Removing Ditto persistence directories..."
	@find . -type d -name ".ditto*" -exec rm -rf {} + 2>/dev/null || true
	@echo "Ditto directories cleaned"
	@echo "Running tests in parallel..."
	@cargo test --lib

# Run tests without cleanup (faster for iterative development)
test-fast:
	@cargo test --lib

# Run specific test by name
test-one:
	@cargo test --lib $(TEST) -- --nocapture --test-threads=1

# Run E2E tests specifically
test-e2e:
	@echo "Running E2E integration tests..."
	@find . -type d -name ".ditto*" -exec rm -rf {} + 2>/dev/null || true
	@if [ -f ../.env ]; then \
		export $$(grep -v '^#' ../.env | xargs) && cargo test --test squad_formation_e2e -- --nocapture --test-threads=1; \
	else \
		cargo test --test squad_formation_e2e -- --nocapture --test-threads=1; \
	fi

# Clean build artifacts and Ditto directories
clean:
	@cargo clean
	@find . -type d -name ".ditto*" -exec rm -rf {} + 2>/dev/null || true
	@echo "Cleaned build artifacts and Ditto directories"

# Format code
fmt:
	@cargo fmt --all

# Run clippy linter
clippy:
	@cargo clippy --all-targets --all-features -- -D warnings

# Type check only
check:
	@cargo check

# Pre-commit checks (format, clippy, test)
pre-commit: clean
	@echo "Running pre-commit checks..."
	@echo "1. Formatting code..."
	@cargo fmt --all
	@echo "2. Running clippy..."
	@cargo clippy --all-targets --all-features -- -D warnings
	@echo "3. Running tests..."
	@$(MAKE) test
	@echo "All pre-commit checks passed!"

# Run tests with coverage (requires cargo-llvm-cov)
coverage:
	@echo "Running tests with coverage..."
	@cargo llvm-cov --lib --html
	@echo "Coverage report generated in target/llvm-cov/html/index.html"

# Help command
help:
	@echo "CAP Protocol - Available Make Targets:"
	@echo ""
	@echo "  make test          - Run all unit tests in parallel (~6s, 43x faster!)"
	@echo "  make test-fast     - Run tests without cleanup (even faster)"
	@echo "  make test-one TEST=<name> - Run specific test with output"
	@echo "  make test-e2e      - Run E2E integration tests (sequential)"
	@echo "  make clean         - Clean build artifacts and Ditto dirs"
	@echo "  make fmt           - Format code with rustfmt"
	@echo "  make clippy        - Run clippy linter"
	@echo "  make check         - Type check only (fast)"
	@echo "  make pre-commit    - Run all pre-commit checks"
	@echo "  make coverage      - Generate test coverage report"
	@echo "  make help          - Show this help message"
