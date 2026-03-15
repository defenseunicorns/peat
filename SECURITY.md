# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in peat, please report it responsibly. **Do not open a public GitHub issue for security vulnerabilities.**

### How to Report

You have two options:

1. **Email**: Send a detailed report to [security@defenseunicorns.com](mailto:security@defenseunicorns.com)
2. **GitHub Security Advisories**: Use the [private vulnerability reporting](https://github.com/defenseunicorns/peat/security/advisories/new) feature on this repository

### What to Include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### Response Timeline

- **Acknowledgment**: Within 3 business days
- **Initial assessment**: Within 10 business days
- **Fix timeline**: Dependent on severity

### Disclosure Policy

- We will acknowledge reporters in the remediation PR (unless anonymity is requested)
- We follow coordinated disclosure practices
- We aim to release patches before public disclosure

## Supported Versions

| Version | Supported |
|---------|-----------|
| latest  | Yes       |

## Security-Relevant Areas

peat is a tactical mesh protocol workspace comprising 10 crates. The following areas are particularly security-sensitive:

- **Protocol security**: Formation keys, membership certificates, and channel encryption govern mesh access control. Weaknesses here could allow unauthorized nodes to join a formation or eavesdrop on traffic.
- **CRDT sync integrity**: Automerge and Ditto backends replicate state across peers. Malicious or malformed CRDT operations could corrupt shared state or cause divergence.
- **Transport security**: QUIC/Iroh, BLE, and UDP bypass channels carry mesh traffic. TLS configuration, connection establishment, and stream multiplexing must enforce secure defaults across all transports.
- **FFI boundary**: peat-ffi exposes mobile bindings. Memory safety, input validation, and error handling at the FFI layer are critical to prevent undefined behavior in host applications.
- **TAK bridge**: CoT protocol translation bridges tactical data between peat and TAK ecosystems. Malformed or spoofed CoT messages could inject false situational awareness data.
- **Edge inference**: Model distribution and orchestration move ML artifacts across the mesh. Tampered models or unauthorized orchestration commands could compromise edge nodes.

## Security Best Practices

When integrating peat, follow these practices:

- Use certificate-based enrollment for production deployments
- Enable formation key authentication to restrict mesh membership
- Use unique formation secrets per mesh network to prevent cross-network access
- Deploy behind TLS-terminating ingress for REST APIs
- Keep dependencies up to date
