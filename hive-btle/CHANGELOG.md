# Changelog

All notable changes to hive-btle will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- ESP32 platform support with NimBLE integration
- macOS platform support with CoreBluetooth
- iOS platform support with CoreBluetooth
- EmergencyEvent CRDT with distributed ACK tracking
- Peer connection tracking and management
- GATT server and client implementations for all platforms
- Cross-platform CRDT document synchronization
- Swift test application for macOS/iOS testing

### Changed
- Centralized peer management and document sync
- Improved peer connection tracking and display updates

### Fixed
- EMERGENCY/ACK sync between ESP32 and macOS/iOS devices

## [0.1.0] - 2024-12-01

### Added
- Initial release
- Linux platform support with BlueZ/bluer
- Core BLE transport architecture
- BleAdapter trait for platform abstraction
- HIVE beacon format and discovery protocol
- GATT service definition (0xF47A)
- Power profile management (Aggressive, Balanced, LowPower, UltraLow)
- Lightweight CRDT implementations (GCounter, LWWRegister, ORSet)
- Hierarchical mesh topology support
- Delta-based synchronization protocol
- Coded PHY support for extended range (BLE 5.0+)

### Platform Support
- Linux (BlueZ 5.48+) - Complete
- macOS (CoreBluetooth) - Complete
- iOS (CoreBluetooth) - Complete
- ESP32 (NimBLE) - Complete
- Android - In Progress
- Windows - Planned
