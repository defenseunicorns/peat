# Shadow Network Simulator - Installation Guide

**Date**: 2025-11-05
**Shadow Version**: 3.3.0
**Tested On**: Ubuntu 22.04 (Linux 6.8.0-87-generic)

## Quick Summary

Shadow requires **Rust nightly** to build from source due to unstable features used in the codebase.

## Installation Steps

### 1. Install Rust Nightly

```bash
# Install nightly toolchain
rustup toolchain install nightly

# Verify installation
rustup toolchain list
```

### 2. Clone Shadow Repository

```bash
cd ~/Code/revolve  # or your preferred location
git clone https://github.com/shadow/shadow.git
cd shadow
```

### 3. Checkout Stable Release

```bash
# Use stable v3.3.0 instead of main branch
git checkout v3.3.0
```

### 4. Set Nightly Toolchain Override

```bash
# Set nightly for Shadow directory only
rustup override set nightly

# Verify
rustc --version  # Should show nightly version
```

### 5. Build Shadow

```bash
# Clean build with release mode
./setup build --clean
```

**Build time**: ~2-3 minutes on modern hardware

**Expected output**:
```
...
Building workspace...
Building workspace...
Building shadow-tests...
...
Finished `release` profile [optimized] target(s) in XX.XXs
```

### 6. Install Shadow

```bash
# Install to ~/.local/bin (no sudo required!)
./setup install
```

**Installation location**: `~/.local/bin/shadow`

### 7. Verify Installation

```bash
# Add to PATH if not already (add to ~/.bashrc for persistence)
export PATH="$HOME/.local/bin:$PATH"

# Verify
shadow --version
```

**Expected output**:
```
Shadow 3.3.0 — v3.3.0-0-g5a05740ba 2025-10-16--11:24:01
```

## Troubleshooting

### Error: `use of unstable library feature 'unsigned_is_multiple_of'`

**Cause**: Building with stable Rust instead of nightly

**Solution**:
```bash
cd ~/Code/revolve/shadow
rustup override set nightly
./setup build --clean
```

### Error: Rustup not installed

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Error: Missing system dependencies

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y build-essential cmake pkg-config libglib2.0-dev

# Fedora
sudo dnf install gcc gcc-c++ cmake pkgconfig glib2-devel
```

## System Requirements

- **OS**: Linux (Ubuntu 22.04+, Debian 11+, Fedora 42+)
- **Kernel**: 5.10+ (standard on modern distros)
- **RAM**: 4GB minimum (16GB recommended for 100+ node simulations)
- **CPU**: 4+ cores recommended
- **Disk**: ~2GB for build artifacts
- **Root**: Not required (major advantage!)

## Post-Installation

### Verify Shadow Works

Create a simple test configuration:

```bash
# Create test directory
mkdir -p ~/shadow-test
cd ~/shadow-test

# Create minimal shadow.yaml
cat > shadow.yaml << 'EOF'
general:
  stop_time: 10s
  seed: 1

network:
  graph:
    type: 1_gbit_switch

hosts:
  test_host:
    network_node_id: 0
    processes:
      - path: /bin/echo
        args: "Hello from Shadow!"
        start_time: 1s
EOF

# Run test
shadow shadow.yaml

# Check output
cat shadow.data/hosts/test_host/stdout.log
```

**Expected**: You should see "Hello from Shadow!" in the log

### Reset to Stable Rust (Optional)

If you want to use stable Rust for other projects:

```bash
# Remove override (only affects Shadow directory)
cd ~/Code/revolve/shadow
rustup override unset

# Or set a different default
rustup default stable
```

Shadow will still use nightly when building in its directory due to the override.

## Integration with CAP Project

### Add Shadow to PATH

For persistent access, add to `~/.bashrc`:

```bash
# Add to ~/.bashrc
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### Verify from CAP Directory

```bash
cd ~/Code/revolve/hive
shadow --version  # Should work from anywhere
```

### CAP Project Rust Version

The CAP project uses **stable Rust**, not nightly. This is fine! Only Shadow needs nightly for building. Your CAP binaries will run under Shadow's simulated environment.

## Next Steps

✅ **Shadow is installed!** You're ready to begin E8.0 POC.

**Next**: [Issue #36 - E8.0: Shadow + Ditto POC](https://github.com/kitplummer/hive/issues/36)

Tasks:
1. Create `hive-protocol/examples/shadow_poc.rs` - Minimal Ditto sync test
2. Create `hive-sim/scenarios/poc-ditto-sync.yaml` - Shadow configuration
3. Run: `shadow hive-sim/scenarios/poc-ditto-sync.yaml`
4. Document results: GO/NO-GO decision

## Resources

- **Shadow Docs**: https://shadow.github.io/docs/guide/
- **Shadow GitHub**: https://github.com/shadow/shadow
- **E8 Implementation Plan**: [docs/E8-IMPLEMENTATION-PLAN.md](E8-IMPLEMENTATION-PLAN.md)
- **E8 Getting Started**: [docs/E8-GETTING-STARTED.md](E8-GETTING-STARTED.md)

---

**Document Version**: 1.0
**Last Updated**: 2025-11-05
**Tested By**: Kit Plummer
