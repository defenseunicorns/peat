//! HIVE-Lite M5Stack Core2 Demo
//!
//! Demonstrates HIVE-Lite running on ESP32 with display output.

#![no_std]
#![no_main]

use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// Required for ESP-IDF bootloader compatibility
esp_bootloader_esp_idf::esp_app_desc!();

// Import HIVE-Lite
use hive_lite::prelude::*;

// Display support (only when m5stack-core2 feature is enabled)
#[cfg(feature = "m5stack-core2")]
use {
    display_interface_spi::SPIInterface,
    embedded_graphics::{
        mono_font::{ascii::FONT_6X10, ascii::FONT_10X20, MonoTextStyle},
        pixelcolor::Rgb565,
        prelude::*,
        primitives::{PrimitiveStyle, Rectangle},
        text::Text,
    },
    embedded_hal_bus::spi::ExclusiveDevice,
    esp_hal::gpio::{Level, Output, OutputConfig},
    esp_hal::i2c::master::{Config as I2cConfig, I2c},
    esp_hal::spi::{master::Spi, Mode as SpiMode},
    mipidsi::{models::ILI9342CRgb565, options::ColorOrder, Builder},
};

#[cfg(feature = "m5stack-core2")]
use core::fmt::Write as FmtWrite;

/// Simple busy-wait delay for display initialization
#[cfg(feature = "m5stack-core2")]
struct BusyWaitDelay;

#[cfg(feature = "m5stack-core2")]
impl embedded_hal::delay::DelayNs for BusyWaitDelay {
    fn delay_ns(&mut self, ns: u32) {
        let start = Instant::now();
        let duration = Duration::from_micros((ns / 1000).max(1) as u64);
        while start.elapsed() < duration {}
    }
}

/// AXP192 Power Management IC interface for M5Stack Core2
#[cfg(feature = "m5stack-core2")]
const AXP192_ADDR: u8 = 0x34;

/// Check if power button was short-pressed (register 0x46, bit 1)
#[cfg(feature = "m5stack-core2")]
fn axp192_check_button_press<I2C>(i2c: &mut I2C) -> bool
where
    I2C: embedded_hal::i2c::I2c,
{
    let mut buf = [0u8; 1];
    // Read IRQ status register 3 (0x46)
    if i2c.write_read(AXP192_ADDR, &[0x46], &mut buf).is_ok() {
        let pressed = (buf[0] & 0x02) != 0; // Bit 1 = short press
        if pressed {
            // Clear the interrupt by writing 0x02 back
            let _ = i2c.write(AXP192_ADDR, &[0x46, 0x02]);
        }
        pressed
    } else {
        false
    }
}

/// Entry point for ESP32
#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_println::println!("========================================");
    esp_println::println!("  HIVE-Lite v{}", env!("CARGO_PKG_VERSION"));
    esp_println::println!("  Protocol version: {}", PROTOCOL_VERSION);
    esp_println::println!("========================================");

    // Initialize display for M5Stack Core2
    #[cfg(feature = "m5stack-core2")]
    let mut display = {
        // M5Stack Core2 LCD pins:
        // SCK: GPIO18, MOSI: GPIO23, MISO: GPIO38
        // CS: GPIO5, DC: GPIO15
        let sck = peripherals.GPIO18;
        let mosi = peripherals.GPIO23;
        let miso = peripherals.GPIO38;
        let cs = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
        let dc = Output::new(peripherals.GPIO15, Level::Low, OutputConfig::default());

        // Create SPI bus
        let spi = Spi::new(
            peripherals.SPI2,
            esp_hal::spi::master::Config::default()
                .with_frequency(esp_hal::time::Rate::from_mhz(40))
                .with_mode(SpiMode::_0),
        )
        .unwrap()
        .with_sck(sck)
        .with_mosi(mosi)
        .with_miso(miso);

        // Wrap SPI bus with CS pin to create SpiDevice
        let spi_device = ExclusiveDevice::new_no_delay(spi, cs).unwrap();

        // Create display interface (SPI device + DC pin)
        let spi_iface = SPIInterface::new(spi_device, dc);

        // Initialize ILI9342C display (320x240)
        // Use a simple busy-wait delay implementation
        let mut delay = BusyWaitDelay;
        let mut disp = Builder::new(ILI9342CRgb565, spi_iface)
            .display_size(320, 240)
            .color_order(ColorOrder::Bgr)
            .init(&mut delay)
            .unwrap();

        // Clear to dark blue background
        disp.clear(Rgb565::new(0, 0, 8)).unwrap();

        // Draw HIVE-Lite banner
        let title_style = MonoTextStyle::new(&FONT_10X20, Rgb565::new(31, 63, 0)); // Yellow-green
        let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);

        Text::new("HIVE-Lite", Point::new(100, 30), title_style)
            .draw(&mut disp)
            .unwrap();

        Text::new("Mesh Protocol for ESP32", Point::new(70, 55), text_style)
            .draw(&mut disp)
            .unwrap();

        // Draw separator line
        Rectangle::new(Point::new(10, 65), Size::new(300, 2))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::new(0, 31, 0)))
            .draw(&mut disp)
            .unwrap();

        esp_println::println!("Display initialized");
        disp
    };

    // Initialize I2C for AXP192 power button
    #[cfg(feature = "m5stack-core2")]
    let mut i2c = {
        // M5Stack Core2 I2C pins: SDA=GPIO21, SCL=GPIO22
        let sda = peripherals.GPIO21;
        let scl = peripherals.GPIO22;

        I2c::new(peripherals.I2C0, I2cConfig::default())
            .unwrap()
            .with_sda(sda)
            .with_scl(scl)
    };

    #[cfg(feature = "m5stack-core2")]
    {
        // Clear any pending button interrupts on startup
        let _ = i2c.write(AXP192_ADDR, &[0x46, 0xFF]);
        esp_println::println!("I2C initialized for AXP192");
    }

    // Generate a node ID (in production, derive from chip MAC)
    let node_id: u32 = 0x4D355443; // "M5TC" in hex

    // Create gossip state with Lite capabilities
    let mut capabilities = NodeCapabilities::lite();
    capabilities.set(NodeCapabilities::DISPLAY_OUTPUT);
    capabilities.set(NodeCapabilities::SENSOR_INPUT);

    let mut gossip = GossipState::new(node_id, capabilities);

    esp_println::println!("Node ID: 0x{:08X}", node_id);
    esp_println::println!("Capabilities: {}", capabilities);

    // Show node info on display
    #[cfg(feature = "m5stack-core2")]
    {
        let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
        let label_style = MonoTextStyle::new(&FONT_6X10, Rgb565::new(0, 63, 31)); // Cyan

        Text::new("Node:", Point::new(10, 85), label_style)
            .draw(&mut display)
            .unwrap();
        Text::new("0x4D355443", Point::new(50, 85), text_style)
            .draw(&mut display)
            .unwrap();

        Text::new("Caps:", Point::new(150, 85), label_style)
            .draw(&mut display)
            .unwrap();
        Text::new("lite,sensor,disp", Point::new(190, 85), text_style)
            .draw(&mut display)
            .unwrap();

        // Status section labels
        Text::new("--- Status ---", Point::new(110, 110), label_style)
            .draw(&mut display)
            .unwrap();

        Text::new("Temp:", Point::new(10, 130), label_style)
            .draw(&mut display)
            .unwrap();
        Text::new("Activity:", Point::new(10, 150), label_style)
            .draw(&mut display)
            .unwrap();
        Text::new("Buttons:", Point::new(10, 170), label_style)
            .draw(&mut display)
            .unwrap();
        Text::new("Peers:", Point::new(10, 190), label_style)
            .draw(&mut display)
            .unwrap();
        Text::new("Uptime:", Point::new(10, 210), label_style)
            .draw(&mut display)
            .unwrap();
        Text::new("TX Msgs:", Point::new(160, 210), label_style)
            .draw(&mut display)
            .unwrap();
    }

    // Create CRDTs for sensor data
    let mut temperature: LwwRegister<i32> = LwwRegister::default();
    let mut activity_level: LwwRegister<u8> = LwwRegister::default();
    let mut button_presses: GCounter = GCounter::new(node_id);

    // Track time and message count
    let mut last_status = Instant::now();
    let mut last_heartbeat = Instant::now();
    let status_interval = Duration::from_secs(1); // Update display every second
    let heartbeat_interval = Duration::from_secs(5);

    let mut loop_count: u32 = 0;
    let mut tx_count: u32 = 0;

    esp_println::println!("Entering main loop...");

    // Main loop
    loop {
        let elapsed_ms = Instant::now().duration_since_epoch().as_millis() as u64;

        // Simulate sensor readings
        let temp_reading = 2300 + ((elapsed_ms / 100) % 100) as i32;
        temperature.set(temp_reading, elapsed_ms, node_id);

        let activity = ((elapsed_ms / 50) % 100) as u8;
        activity_level.set(activity, elapsed_ms, node_id);

        // Check for real power button press via AXP192
        #[cfg(feature = "m5stack-core2")]
        if axp192_check_button_press(&mut i2c) {
            button_presses.increment();
            esp_println::println!("[EVENT] Power button pressed! Total: {}", button_presses.count());
        }

        // Fallback for non-Core2 builds: simulate button press every ~10 seconds
        #[cfg(not(feature = "m5stack-core2"))]
        if loop_count % 1000 == 0 && loop_count > 0 {
            button_presses.increment();
            esp_println::println!("[EVENT] Simulated button press! Total: {}", button_presses.count());
        }

        // Run gossip protocol tick
        gossip.tick(elapsed_ms);

        // Process outbound messages
        let outbound = gossip.take_outbound();
        if !outbound.is_empty() {
            tx_count += outbound.len() as u32;
            for msg in outbound.iter() {
                match msg.target {
                    MessageTarget::Multicast => {
                        esp_println::println!("[TX] Multicast {} bytes", msg.data.len());
                    }
                    _ => {}
                }
            }
        }

        // Update display periodically
        if last_status.elapsed() >= status_interval {
            last_status = Instant::now();
            let uptime_secs = elapsed_ms / 1000;

            #[cfg(feature = "m5stack-core2")]
            {
                let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
                let clear_style = PrimitiveStyle::with_fill(Rgb565::new(0, 0, 8));

                // Clear value areas and redraw
                // Temperature
                Rectangle::new(Point::new(70, 122), Size::new(80, 12))
                    .into_styled(clear_style)
                    .draw(&mut display)
                    .unwrap();
                let mut buf = heapless::String::<16>::new();
                let _ = core::write!(buf, "{}.{:02} C", temp_reading / 100, (temp_reading % 100).unsigned_abs());
                Text::new(&buf, Point::new(70, 130), text_style)
                    .draw(&mut display)
                    .unwrap();

                // Activity
                Rectangle::new(Point::new(70, 142), Size::new(80, 12))
                    .into_styled(clear_style)
                    .draw(&mut display)
                    .unwrap();
                buf.clear();
                let _ = core::write!(buf, "{}%", activity);
                Text::new(&buf, Point::new(70, 150), text_style)
                    .draw(&mut display)
                    .unwrap();

                // Buttons
                Rectangle::new(Point::new(70, 162), Size::new(80, 12))
                    .into_styled(clear_style)
                    .draw(&mut display)
                    .unwrap();
                buf.clear();
                let _ = core::write!(buf, "{}", button_presses.count());
                Text::new(&buf, Point::new(70, 170), text_style)
                    .draw(&mut display)
                    .unwrap();

                // Peers
                Rectangle::new(Point::new(70, 182), Size::new(80, 12))
                    .into_styled(clear_style)
                    .draw(&mut display)
                    .unwrap();
                buf.clear();
                let _ = core::write!(buf, "{}", gossip.peers.len());
                Text::new(&buf, Point::new(70, 190), text_style)
                    .draw(&mut display)
                    .unwrap();

                // Uptime
                Rectangle::new(Point::new(70, 202), Size::new(80, 12))
                    .into_styled(clear_style)
                    .draw(&mut display)
                    .unwrap();
                buf.clear();
                let _ = core::write!(buf, "{}s", uptime_secs);
                Text::new(&buf, Point::new(70, 210), text_style)
                    .draw(&mut display)
                    .unwrap();

                // TX count
                Rectangle::new(Point::new(220, 202), Size::new(80, 12))
                    .into_styled(clear_style)
                    .draw(&mut display)
                    .unwrap();
                buf.clear();
                let _ = core::write!(buf, "{}", tx_count);
                Text::new(&buf, Point::new(220, 210), text_style)
                    .draw(&mut display)
                    .unwrap();
            }

            // Also log to serial every 5 seconds
            if uptime_secs % 5 == 0 {
                esp_println::println!("--- Status @ {}s ---", uptime_secs);
                esp_println::println!("  Temp: {}.{:02}C", temp_reading / 100, (temp_reading % 100).unsigned_abs());
                esp_println::println!("  Activity: {}%", activity);
                esp_println::println!("  Buttons: {}", button_presses.count());
                esp_println::println!("  Peers: {}", gossip.peers.len());
                esp_println::println!("  TX: {}", tx_count);
            }
        }

        // Queue heartbeat periodically
        if last_heartbeat.elapsed() >= heartbeat_interval {
            last_heartbeat = Instant::now();
            gossip.queue_heartbeat();
        }

        loop_count = loop_count.wrapping_add(1);

        // Small delay
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(10) {}
    }
}
