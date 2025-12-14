//! Power Management for HIVE-Lite
//!
//! Provides power-efficient radio scheduling and profile management
//! for battery-constrained BLE devices.
//!
//! ## Overview
//!
//! This module implements the power management layer that enables
//! HIVE-Lite devices to achieve 20+ hour battery life on typical
//! smartwatch hardware (300mAh battery).
//!
//! ## Key Components
//!
//! - **Power Profiles**: Predefined configurations balancing latency vs battery
//! - **Radio Scheduler**: Coordinates scan, advertise, and sync activities
//! - **Battery Awareness**: Auto-adjusts profile based on battery state
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────┐
//! │                    Application                          │
//! │              (sync requests, alerts)                    │
//! └─────────────────────┬──────────────────────────────────┘
//!                       │
//!                       ▼
//! ┌────────────────────────────────────────────────────────┐
//! │                 RadioScheduler                          │
//! │  ┌──────────────┐  ┌────────────┐  ┌────────────────┐  │
//! │  │ PowerProfile │  │   Pending  │  │   Battery      │  │
//! │  │   Timing     │──│   Syncs    │──│   Monitor      │  │
//! │  └──────────────┘  └────────────┘  └────────────────┘  │
//! └─────────────────────┬──────────────────────────────────┘
//!                       │
//!                       ▼
//! ┌────────────────────────────────────────────────────────┐
//! │              BLE Radio Hardware                         │
//! │         (scan, advertise, connect)                      │
//! └────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Power Profiles
//!
//! | Profile | Duty Cycle | Battery Life | Use Case |
//! |---------|------------|--------------|----------|
//! | Aggressive | ~20% | ~6h | Emergency response |
//! | Balanced | ~10% | ~12h | Active tracking |
//! | **LowPower** | **~2%** | **~20h** | Normal operation |
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::power::{RadioScheduler, PowerProfile, SyncPriority};
//! use hive_btle::NodeId;
//!
//! // Create scheduler with low-power profile
//! let mut scheduler = RadioScheduler::with_profile(PowerProfile::LowPower);
//!
//! // Queue a normal sync
//! scheduler.queue_sync(peer_id, SyncPriority::Normal, 100, current_time);
//!
//! // Main loop
//! loop {
//!     if let Some((event, time)) = scheduler.next_event(current_time) {
//!         match event {
//!             SchedulerEvent::StartScan => {
//!                 radio.start_scan();
//!                 scheduler.process_event(event, current_time);
//!             }
//!             SchedulerEvent::SyncNow => {
//!                 if let Some(sync) = scheduler.next_pending_sync(current_time) {
//!                     perform_sync(&sync);
//!                     scheduler.complete_sync(current_time);
//!                 }
//!             }
//!             SchedulerEvent::EnterSleep => {
//!                 let sleep_time = scheduler.time_until_next_activity(current_time);
//!                 mcu.sleep_ms(sleep_time);
//!             }
//!             _ => scheduler.process_event(event, current_time),
//!         }
//!     }
//! }
//! ```
//!
//! ## Critical Sync
//!
//! For urgent data (SOS alerts, medical emergencies), use `SyncPriority::Critical`:
//!
//! ```ignore
//! // This bypasses normal scheduling and syncs immediately
//! scheduler.queue_sync(peer_id, SyncPriority::Critical, data_size, current_time);
//! ```
//!
//! ## Battery Auto-Adjustment
//!
//! The scheduler can automatically switch to lower power profiles as battery depletes:
//!
//! ```ignore
//! scheduler.set_auto_adjust(true);
//! scheduler.update_battery(BatteryState::new(15, false), current_time);
//! // Automatically switches from Aggressive/Balanced to LowPower
//! ```

pub mod profile;
pub mod scheduler;

pub use profile::{BatteryState, PowerProfile, RadioTiming};
pub use scheduler::{
    PendingSync, RadioScheduler, RadioState, SchedulerConfig, SchedulerEvent, SchedulerStats,
    SyncPriority,
};
