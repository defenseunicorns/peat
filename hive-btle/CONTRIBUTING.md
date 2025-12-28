# Contributing to hive-btle

Thank you for your interest in contributing to hive-btle! This document provides guidelines and information for contributors.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/hive-btle.git`
3. Create a feature branch: `git checkout -b feature/your-feature-name`
4. Make your changes
5. Run tests: `cargo test`
6. Commit with a descriptive message
7. Push to your fork and submit a Pull Request

## Development Setup

### Prerequisites

- Rust 1.75 or later
- Platform-specific dependencies:
  - **Linux**: BlueZ 5.48+, D-Bus development libraries
  - **macOS**: Xcode Command Line Tools
  - **ESP32**: ESP-IDF toolchain

### Building

```bash
# Build for your platform
cargo build --features linux    # Linux
cargo build --features macos    # macOS
cargo build --features ios      # iOS (cross-compile)

# Build for ESP32
cargo +esp build --target xtensa-esp32-espidf --features esp32
```

### Testing

```bash
# Run unit tests (no hardware required)
cargo test

# Run with platform features
cargo test --features linux

# Run specific test module
cargo test sync::
```

## Code Guidelines

### Style

- Follow Rust standard style (use `cargo fmt`)
- Run `cargo clippy` and address warnings
- Write doc comments for public APIs
- Keep functions focused and reasonably sized

### Safety

- Minimize `unsafe` code; document safety invariants when required
- Avoid panics in library code; return `Result` types
- Be careful with BLE security - never store credentials in code

### Testing

- Write unit tests for new functionality
- Integration tests go in `tests/`
- Test on real hardware when possible (BLE simulators have limitations)

## Priority Areas

We welcome contributions in these areas:

1. **Android Implementation** (#410) - JNI bindings to Android Bluetooth API
2. **Security Integration** (#413) - BLE pairing + application-layer encryption
3. **Windows Implementation** (#412) - WinRT Bluetooth APIs
4. **Hardware Testing** - Real-world validation on various devices
5. **Documentation** - Examples, tutorials, API documentation

## Pull Request Process

1. Ensure your code compiles without warnings
2. Add tests for new functionality
3. Update documentation as needed
4. Keep PRs focused on a single change
5. Reference any related issues in the PR description

## Reporting Issues

When reporting bugs, please include:

- Platform and OS version
- Bluetooth hardware details
- Rust version (`rustc --version`)
- Steps to reproduce
- Expected vs actual behavior
- Relevant log output

## Code of Conduct

Be respectful and constructive in all interactions. We're building software for critical tactical applications - quality and reliability matter more than speed.

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.
