# Contributing to PEAT Protocol

Thank you for your interest in contributing to the PEAT Protocol! This document provides guidelines for contributing to both the specification and the reference implementation.

## Table of Contents

1. [Ways to Contribute](#ways-to-contribute)
2. [Getting Started](#getting-started)
3. [Specification Contributions](#specification-contributions)
4. [Code Contributions](#code-contributions)
5. [Review Process](#review-process)
6. [Style Guidelines](#style-guidelines)
7. [Legal](#legal)

---

## Ways to Contribute

### For Everyone

- **Report bugs**: Found an issue? Open a GitHub issue with details
- **Suggest features**: Have an idea? Start a discussion
- **Improve documentation**: Typos, clarifications, examples welcome
- **Test and validate**: Run the test suite, report flaky tests

### For Developers

- **Fix bugs**: Check issues labeled `good first issue` or `help wanted`
- **Implement features**: Coordinate via issue before starting large work
- **Write tests**: Increase coverage, especially E2E tests
- **Review PRs**: Help review pending pull requests

### For Researchers

- **Validate claims**: Run experiments, publish results
- **Propose improvements**: Back proposals with evidence
- **Compare alternatives**: Benchmark against other approaches

### For Standards Experts

- **Review specification**: Check for ambiguity, inconsistency
- **Suggest clarifications**: Where is the spec unclear?
- **Alignment**: How does PEAT relate to existing standards?

---

## Getting Started

### 1. Fork and Clone

```bash
git clone https://github.com/YOUR_USERNAME/peat.git
cd peat
```

### 2. Set Up Development Environment

```bash
# Install Rust (1.70+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build
cargo build

# Run tests
cargo test -- --test-threads=1
```

### 3. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/issue-number-description
```

### 4. Make Changes

- Follow the style guidelines below
- Add tests for new functionality
- Update documentation as needed

### 5. Submit Pull Request

```bash
git push origin your-branch-name
# Then open PR on GitHub
```

---

## Specification Contributions

### Location

Specification content lives in `/spec/`:

```
spec/
├── draft-peat-protocol-00.md   # Main specification
├── proto/                       # Protocol Buffer definitions
│   └── cap/v1/*.proto
└── README.md
```

### Types of Spec Changes

#### Editorial (Minor)

- Typo fixes
- Clarifications that don't change meaning
- Formatting improvements

Process: Direct PR, quick review

#### Substantive (Major)

- New protocol features
- Changed semantics
- Wire format modifications

Process:
1. Open issue describing proposed change
2. Discussion period (2+ weeks)
3. Draft RFC if significant
4. Implementation proof-of-concept
5. PR with specification change

### RFC 2119 Language

Use requirement keywords correctly:

- **MUST** / **MUST NOT**: Absolute requirement
- **SHOULD** / **SHOULD NOT**: Recommended
- **MAY**: Optional

### Versioning

- Patch: Editorial, clarifications
- Minor: Backward-compatible additions
- Major: Breaking changes (new proto package version)

---

## Code Contributions

### Repository Structure

```
peat/
├── spec/                    # Normative specification (CC0/CC BY)
├── reference/               # Reference implementation (MIT/Apache-2.0)
│   └── rust/
│       ├── peat-protocol/   # Core library
│       ├── peat-mesh/       # Mesh management
│       └── ...
├── tools/                   # Utilities and testing tools
├── labs/                    # Experiments
└── governance/              # Project governance
```

### Code Quality

All code contributions must:

1. **Build without warnings**: `cargo build` clean
2. **Pass all tests**: `cargo test -- --test-threads=1`
3. **Pass linting**: `cargo clippy`
4. **Be formatted**: `cargo fmt`

### Testing Requirements

- **Unit tests**: For new functions/modules
- **Integration tests**: For component interactions
- **E2E tests**: For distributed behavior (when applicable)

**Critical**: Flaky tests are not acceptable. If a test fails intermittently, it must be fixed or removed before merge.

### Pre-Commit Checklist

```bash
# Run all checks
make pre-commit

# Or individually:
cargo fmt --check
cargo clippy -- -D warnings
cargo test -- --test-threads=1
```

---

## Review Process

### Pull Request Guidelines

1. **Clear title**: Describe the change concisely
2. **Description**: Explain what and why
3. **Issue link**: Reference related issues
4. **Small PRs**: Easier to review, faster to merge

### Review Criteria

Reviewers check for:

- [ ] Correctness: Does it do what it claims?
- [ ] Tests: Are changes tested?
- [ ] Style: Does it follow guidelines?
- [ ] Documentation: Are docs updated?
- [ ] Security: Any security implications?
- [ ] Performance: Any performance impact?

### Approval Requirements

- **Documentation/minor**: 1 maintainer approval
- **Code changes**: 1 maintainer approval
- **Spec changes**: 2 maintainer approvals + review period
- **Breaking changes**: Project Lead approval

### Merge Process

1. All CI checks pass
2. Required approvals obtained
3. No unresolved conversations
4. Squash and merge (clean history)

---

## Style Guidelines

### Rust Code

Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/):

- Use `rustfmt` defaults
- Prefer explicit error handling over `unwrap()`
- Document public APIs with `///` comments
- Use meaningful variable names

### Protocol Buffers

- Use `snake_case` for field names
- Use `SCREAMING_SNAKE_CASE` for enum values
- Include comments explaining each message and field
- Use RFC 2119 keywords in normative comments

### Markdown

- Use ATX-style headers (`#`)
- One sentence per line (easier diffs)
- Reference links at document end
- Code blocks with language specifier

### Git Commits

Format:
```
type(scope): short description

Longer explanation if needed.

Fixes #123
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

---

## Legal

### Contributor License

By contributing, you agree that:

1. Your contributions are your original work
2. You have the right to submit them
3. You license them under the project's licenses:
   - Specification: CC BY 4.0 / CC0 1.0
   - Code: MIT OR Apache-2.0

### Patent Grant

Contributors grant a patent license consistent with [PATENT_PLEDGE.md](PATENT_PLEDGE.md).

### Third-Party Code

If including third-party code:

1. Ensure license compatibility
2. Preserve copyright notices
3. Document the source

---

## Questions?

- **GitHub Issues**: For bugs, features, questions
- **GitHub Discussions**: For open-ended discussion
- **Email**: kit@revolveteam.com

Welcome to the PEAT community!
