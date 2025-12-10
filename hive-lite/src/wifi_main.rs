//! HIVE-Lite M5Stack Core2 Demo with WiFi
//!
//! Demonstrates HIVE-Lite running on ESP32 with WiFi mesh networking.

#![no_std]
#![no_main]

extern crate alloc;

use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};
use esp_hal::rng::Rng;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("PANIC: {:?}", info);
    loop {}
}

// Required for ESP-IDF bootloader compatibility
esp_bootloader_esp_idf::esp_app_desc!();

// Import HIVE-Lite
use hive_lite::prelude::*;

// WiFi credentials from environment at compile time
const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PWD");

// UDP broadcast port for HIVE mesh
const HIVE_UDP_PORT: u16 = 5555;

// Display support
#[cfg(feature = "m5stack-core2")]
use {
    display_interface_spi::SPIInterface,
    embedded_graphics::{
        mono_font::{ascii::FONT_9X15_BOLD, ascii::FONT_10X20, MonoTextStyle},
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

/// Simple busy-wait delay
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

/// FT6336U Touch Controller (I2C address 0x38)
#[cfg(feature = "m5stack-core2")]
const FT6336_ADDR: u8 = 0x38;

/// Check for screen touch using FT6336U touch controller
/// Returns Some((x, y)) if touched, None otherwise
#[cfg(feature = "m5stack-core2")]
fn ft6336_check_touch<I2C>(i2c: &mut I2C) -> Option<(u16, u16)>
where
    I2C: embedded_hal::i2c::I2c,
{
    let mut buf = [0u8; 5];
    // Read touch status (reg 0x02) and touch point data (regs 0x03-0x06)
    if i2c.write_read(FT6336_ADDR, &[0x02], &mut buf).is_ok() {
        let num_touches = buf[0] & 0x0F;
        if num_touches > 0 {
            // Extract X and Y coordinates
            let x = (((buf[1] & 0x0F) as u16) << 8) | (buf[2] as u16);
            let y = (((buf[3] & 0x0F) as u16) << 8) | (buf[4] as u16);
            return Some((x, y));
        }
    }
    None
}


/// Get current timestamp for smoltcp
fn timestamp() -> smoltcp::time::Instant {
    smoltcp::time::Instant::from_micros(
        Instant::now().duration_since_epoch().as_micros() as i64
    )
}

/// Entry point for ESP32 with WiFi
#[main]
fn main() -> ! {
    // Initialize heap for WiFi - must be before esp_hal::init
    esp_alloc::heap_allocator!(size: 72 * 1024);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_println::println!("========================================");
    esp_println::println!("  HIVE-Lite v{} (WiFi)", env!("CARGO_PKG_VERSION"));
    esp_println::println!("  Protocol version: {}", PROTOCOL_VERSION);
    esp_println::println!("========================================");

    // Initialize timer for esp-rtos
    let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);

    // Start the RTOS scheduler - required for WiFi
    esp_rtos::start(timg0.timer0);
    esp_println::println!("RTOS started");

    // Initialize random number generator
    let rng = Rng::new();

    // Initialize the radio controller
    esp_println::println!("Initializing radio controller...");
    let radio_controller = esp_radio::init().expect("Failed to init radio");

    // Create WiFi controller and interfaces
    esp_println::println!("Creating WiFi controller...");
    esp_println::println!("  SSID: {}", SSID);

    let (mut wifi_controller, interfaces) = esp_radio::wifi::new(
        &radio_controller,
        peripherals.WIFI,
        esp_radio::wifi::Config::default(),
    ).expect("Failed to create WiFi");

    let mut wifi_device = interfaces.sta;

    // Configure WiFi in client mode
    use esp_radio::wifi::{ClientConfig, ModeConfig};

    let client_config = ClientConfig::default()
        .with_ssid(SSID.try_into().unwrap())
        .with_password(PASSWORD.try_into().unwrap());

    wifi_controller.set_config(&ModeConfig::Client(client_config)).unwrap();

    // Start WiFi
    esp_println::println!("Starting WiFi...");
    wifi_controller.start().unwrap();

    // Connect to AP
    esp_println::println!("Connecting to AP...");
    wifi_controller.connect().unwrap();

    // Wait for connection with timeout and better error handling
    esp_println::println!("Waiting for connection...");
    let connect_start = Instant::now();
    let connect_timeout = Duration::from_secs(30);

    loop {
        match wifi_controller.is_connected() {
            Ok(true) => {
                esp_println::println!("WiFi connected!");
                break;
            }
            Ok(false) => {
                // Still connecting
            }
            Err(e) => {
                esp_println::println!("  Connection error: {:?}", e);
                // Try reconnecting
                let _ = wifi_controller.connect();
            }
        }

        if connect_start.elapsed() > connect_timeout {
            esp_println::println!("Connection timeout! Check WiFi credentials and WPA2 compatibility.");
            esp_println::println!("Note: esp-hal does NOT support WPA3. Use WPA2 or WPA2/WPA3 mixed mode.");
            // Continue anyway to see what happens
            break;
        }

        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(500) {}
        esp_println::println!("  Still connecting... ({:.1}s)",
            connect_start.elapsed().as_millis() as f32 / 1000.0);
    }

    // Set up network stack with smoltcp
    use blocking_network_stack::Stack;
    use smoltcp::iface::{Config as IfaceConfig, Interface, SocketSet, SocketStorage};
    use smoltcp::wire::{EthernetAddress, HardwareAddress};

    // Get MAC address
    let mac = esp_radio::wifi::sta_mac();
    esp_println::println!("MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);

    // Create smoltcp interface
    let iface_config = IfaceConfig::new(HardwareAddress::Ethernet(
        EthernetAddress::from_bytes(&mac)
    ));
    let iface = Interface::new(iface_config, &mut wifi_device, timestamp());

    // Create socket storage - need enough for DHCP + UDP
    let mut socket_storage: [SocketStorage; 4] = Default::default();
    let mut sockets = SocketSet::new(&mut socket_storage[..]);

    // Add DHCP socket
    let dhcp_socket = smoltcp::socket::dhcpv4::Socket::new();
    sockets.add(dhcp_socket);

    let seed = rng.random() as u32;

    // Create network stack
#[allow(unused_mut)]
    let mut stack = Stack::new(
        iface,
        wifi_device,
        sockets,
        || Instant::now().duration_since_epoch().as_millis() as u64,
        seed,
    );

    // Wait for DHCP with timeout and debug output
    esp_println::println!("Waiting for DHCP...");
    let dhcp_start = Instant::now();
    let dhcp_timeout = Duration::from_secs(30);
    let mut last_status = Instant::now();

    loop {
        stack.work();

        // Print status every 2 seconds
        if last_status.elapsed() > Duration::from_secs(2) {
            last_status = Instant::now();
            let iface_up = stack.is_iface_up();
            esp_println::println!("  DHCP: iface_up={}, elapsed={:.1}s",
                iface_up, dhcp_start.elapsed().as_millis() as f32 / 1000.0);
        }

        if stack.is_iface_up() {
            match stack.get_ip_info() {
                Ok(ip_info) => {
                    esp_println::println!("Got IP: {:?}", ip_info.ip);
                    break;
                }
                Err(_) => {}
            }
        }

        if dhcp_start.elapsed() > dhcp_timeout {
            esp_println::println!("DHCP timeout - continuing without IP");
            break;
        }

        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(50) {}
    }

    esp_println::println!("Network ready!");

    // Initialize display
    #[cfg(feature = "m5stack-core2")]
    let mut display = {
        let sck = peripherals.GPIO18;
        let mosi = peripherals.GPIO23;
        let miso = peripherals.GPIO38;
        let cs = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
        let dc = Output::new(peripherals.GPIO15, Level::Low, OutputConfig::default());

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

        let spi_device = ExclusiveDevice::new_no_delay(spi, cs).unwrap();
        let spi_iface = SPIInterface::new(spi_device, dc);

        let mut delay = BusyWaitDelay;
        let mut disp = Builder::new(ILI9342CRgb565, spi_iface)
            .display_size(320, 240)
            .color_order(ColorOrder::Bgr)
            .invert_colors(mipidsi::options::ColorInversion::Inverted)
            .init(&mut delay)
            .unwrap();

        // Black background
        disp.clear(Rgb565::new(0, 0, 0)).unwrap();

        // Styles - larger fonts, high contrast on black
        let title_style = MonoTextStyle::new(&FONT_10X20, Rgb565::new(0, 63, 31)); // Cyan
        let text_style = MonoTextStyle::new(&FONT_9X15_BOLD, Rgb565::WHITE);
        let counter_style = MonoTextStyle::new(&FONT_10X20, Rgb565::new(31, 63, 0)); // Green-yellow

        // Title
        Text::new("HIVE-Lite", Point::new(90, 30), title_style)
            .draw(&mut disp)
            .unwrap();

        // Show IP address
        let mut ip_buf = heapless::String::<32>::new();
        if let Ok(ip_info) = stack.get_ip_info() {
            let _ = core::write!(ip_buf, "IP: {:?}", ip_info.ip);
        } else {
            let _ = core::write!(ip_buf, "IP: acquiring...");
        }
        Text::new(&ip_buf, Point::new(70, 55), text_style)
            .draw(&mut disp)
            .unwrap();

        // Divider line
        Rectangle::new(Point::new(10, 70), Size::new(300, 2))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::new(0, 31, 31))) // Cyan line
            .draw(&mut disp)
            .unwrap();

        // Label for counter
        Text::new("Button Count:", Point::new(80, 100), text_style)
            .draw(&mut disp)
            .unwrap();

        // Large counter display
        Text::new("0", Point::new(130, 135), counter_style)
            .draw(&mut disp)
            .unwrap();

        // Instructions
        Text::new("Tap screen to increment", Point::new(55, 180), text_style)
            .draw(&mut disp)
            .unwrap();

        esp_println::println!("Display initialized");
        disp
    };

    // Initialize I2C for power button
    #[cfg(feature = "m5stack-core2")]
    let mut i2c = {
        let sda = peripherals.GPIO21;
        let scl = peripherals.GPIO22;
        I2c::new(peripherals.I2C0, I2cConfig::default())
            .unwrap()
            .with_sda(sda)
            .with_scl(scl)
    };

    // Node setup
    let node_id: u32 = 0x4D355443;
    let mut capabilities = NodeCapabilities::lite();
    capabilities.set(NodeCapabilities::DISPLAY_OUTPUT);
    capabilities.set(NodeCapabilities::SENSOR_INPUT);

    let mut button_presses: GCounter = GCounter::new(node_id);

    esp_println::println!("Node ID: 0x{:08X}", node_id);
    esp_println::println!("Capabilities: {:?}", capabilities);

    // Set up UDP socket for broadcasting
    let mut rx_meta = [smoltcp::socket::udp::PacketMetadata::EMPTY; 4];
    let mut rx_buffer = [0u8; 512];
    let mut tx_meta = [smoltcp::socket::udp::PacketMetadata::EMPTY; 4];
    let mut tx_buffer = [0u8; 512];

    let mut udp_socket = stack.get_udp_socket(
        &mut rx_meta, &mut rx_buffer,
        &mut tx_meta, &mut tx_buffer,
    );

    udp_socket.bind(HIVE_UDP_PORT).unwrap();
    esp_println::println!("UDP socket bound to port {}", HIVE_UDP_PORT);

    // Broadcast address for local network
    use smoltcp::wire::Ipv4Address;
    let broadcast_addr = Ipv4Address::new(255, 255, 255, 255);

    // Sequence number for messages
    let mut seq_num: u32 = 0;

    // Send initial ANNOUNCE message with capabilities
    {
        let announce_msg = Message::announce(node_id, seq_num, capabilities);
        seq_num += 1;
        let mut pkt_buf = [0u8; MAX_PACKET_SIZE];
        if let Ok(len) = announce_msg.encode(&mut pkt_buf) {
            let _ = udp_socket.send(broadcast_addr.into(), HIVE_UDP_PORT, &pkt_buf[..len]);
            esp_println::println!("[TX] ANNOUNCE sent ({} bytes)", len);
        }
    }

    let mut last_broadcast = Instant::now();
    let broadcast_interval = Duration::from_secs(2);

    // Touch debounce state
    #[cfg(feature = "m5stack-core2")]
    let mut last_touch = Instant::now();
    #[cfg(feature = "m5stack-core2")]
    let mut was_touched = false;
    #[cfg(feature = "m5stack-core2")]
    let touch_debounce = Duration::from_millis(300);

    esp_println::println!("Entering main loop - ADR-035 protocol active");
    esp_println::println!("Tap screen to increment counter!");

    // Main loop
    loop {
        // Process network
        stack.work();

        // Check for screen tap (with debounce)
        #[cfg(feature = "m5stack-core2")]
        let tapped = {
            let is_touched = ft6336_check_touch(&mut i2c).is_some();
            let tapped = is_touched && !was_touched && last_touch.elapsed() > touch_debounce;
            if tapped {
                last_touch = Instant::now();
            }
            was_touched = is_touched;
            tapped
        };

        #[cfg(feature = "m5stack-core2")]
        if tapped {
            button_presses.increment();
            let count = button_presses.count();
            esp_println::println!("[TAP] Screen tapped! Count: {}", count);

            // Broadcast DATA message with full CRDT state
            let mut crdt_buf = [0u8; 128];
            if let Ok(crdt_len) = button_presses.encode(&mut crdt_buf) {
                if let Some(data_msg) = Message::data(node_id, seq_num, CrdtType::GCounter as u8, &crdt_buf[..crdt_len]) {
                    seq_num += 1;
                    let mut pkt_buf = [0u8; MAX_PACKET_SIZE];
                    if let Ok(len) = data_msg.encode(&mut pkt_buf) {
                        if let Err(e) = udp_socket.send(broadcast_addr.into(), HIVE_UDP_PORT, &pkt_buf[..len]) {
                            esp_println::println!("[TX] Send error: {:?}", e);
                        } else {
                            esp_println::println!("[TX] DATA GCounter ({} bytes, count={})", len, count);
                        }
                    }
                }
            }

            // Update display with large counter
            #[cfg(feature = "m5stack-core2")]
            {
                let counter_style = MonoTextStyle::new(&FONT_10X20, Rgb565::new(31, 63, 0)); // Green-yellow
                let clear_style = PrimitiveStyle::with_fill(Rgb565::new(0, 0, 0));

                // Clear counter area
                Rectangle::new(Point::new(100, 115), Size::new(120, 30))
                    .into_styled(clear_style)
                    .draw(&mut display)
                    .unwrap();

                // Draw new count
                let mut buf = heapless::String::<16>::new();
                let _ = core::write!(buf, "{}", count);
                Text::new(&buf, Point::new(130, 135), counter_style)
                    .draw(&mut display)
                    .unwrap();
            }
        }

        // Periodic heartbeat broadcast (with CRDT state)
        if last_broadcast.elapsed() >= broadcast_interval {
            last_broadcast = Instant::now();

            // Send HEARTBEAT message
            let hb_msg = Message::heartbeat(node_id, seq_num);
            seq_num += 1;
            let mut pkt_buf = [0u8; MAX_PACKET_SIZE];
            if let Ok(len) = hb_msg.encode(&mut pkt_buf) {
                let _ = udp_socket.send(broadcast_addr.into(), HIVE_UDP_PORT, &pkt_buf[..len]);
                esp_println::println!("[TX] HEARTBEAT seq={}", seq_num - 1);
            }

            // Also send current CRDT state periodically
            let mut crdt_buf = [0u8; 128];
            if let Ok(crdt_len) = button_presses.encode(&mut crdt_buf) {
                if let Some(data_msg) = Message::data(node_id, seq_num, CrdtType::GCounter as u8, &crdt_buf[..crdt_len]) {
                    seq_num += 1;
                    let mut pkt_buf2 = [0u8; MAX_PACKET_SIZE];
                    if let Ok(len) = data_msg.encode(&mut pkt_buf2) {
                        let _ = udp_socket.send(broadcast_addr.into(), HIVE_UDP_PORT, &pkt_buf2[..len]);
                    }
                }
            }
        }

        // Check for incoming messages and process CRDT merges
        let mut recv_buf = [0u8; MAX_PACKET_SIZE];
        if let Ok((len, src_ip, _src_port)) = udp_socket.receive(&mut recv_buf) {
            if let Ok(msg) = Message::decode(&recv_buf[..len]) {
                // Don't process our own messages
                if msg.node_id != node_id {
                    match msg.msg_type {
                        MessageType::Data => {
                            // Check CRDT type byte
                            if !msg.payload.is_empty() {
                                let crdt_type = msg.payload[0];

                                if crdt_type == CrdtType::GCounter as u8 {
                                    // GCounter merge
                                    if let Ok(remote_counter) = GCounter::decode(&msg.payload[1..]) {
                                        let old_count = button_presses.count();
                                        button_presses.merge(&remote_counter);
                                        let new_count = button_presses.count();
                                        if new_count != old_count {
                                            esp_println::println!("[RX] Merged GCounter from {:08X}: {} -> {}",
                                                msg.node_id, old_count, new_count);

                                            // Update display
                                            #[cfg(feature = "m5stack-core2")]
                                            {
                                                let counter_style = MonoTextStyle::new(&FONT_10X20, Rgb565::new(31, 63, 0));
                                                let clear_style = PrimitiveStyle::with_fill(Rgb565::new(0, 0, 0));
                                                Rectangle::new(Point::new(100, 115), Size::new(120, 30))
                                                    .into_styled(clear_style)
                                                    .draw(&mut display)
                                                    .unwrap();
                                                let mut buf = heapless::String::<16>::new();
                                                let _ = core::write!(buf, "{}", new_count);
                                                Text::new(&buf, Point::new(130, 135), counter_style)
                                                    .draw(&mut display)
                                                    .unwrap();
                                            }
                                        }
                                    }
                                } else if crdt_type == CrdtType::LwwRegister as u8 {
                                    // LWW-Register: Alert from Full node
                                    // Format: [crdt_type:1][timestamp:8][node_id:4][json_payload:...]
                                    if msg.payload.len() > 13 {
                                        let _timestamp = u64::from_le_bytes(msg.payload[1..9].try_into().unwrap_or([0;8]));
                                        let sender_node = u32::from_le_bytes(msg.payload[9..13].try_into().unwrap_or([0;4]));
                                        let json_data = &msg.payload[13..];

                                        esp_println::println!("[RX] ALERT from Full node {:08X} ({} bytes JSON)",
                                            sender_node, json_data.len());

                                        // Try to extract message from JSON (simple parse)
                                        if let Ok(json_str) = core::str::from_utf8(json_data) {
                                            esp_println::println!("     {}", json_str);

                                            // Display alert on screen
                                            #[cfg(feature = "m5stack-core2")]
                                            {
                                                // Orange alert banner at bottom
                                                Rectangle::new(Point::new(0, 200), Size::new(320, 40))
                                                    .into_styled(PrimitiveStyle::with_fill(Rgb565::new(31, 20, 0))) // Orange
                                                    .draw(&mut display)
                                                    .unwrap();

                                                // Show alert text with larger font
                                                let alert_style = MonoTextStyle::new(&FONT_9X15_BOLD, Rgb565::WHITE);

                                                // Extract message field from JSON (simple approach)
                                                let mut alert_text = heapless::String::<64>::new();
                                                if let Some(start) = json_str.find("\"message\":\"") {
                                                    let msg_start = start + 11;
                                                    if let Some(end) = json_str[msg_start..].find('"') {
                                                        let msg_content = &json_str[msg_start..msg_start+end];
                                                        let _ = core::write!(alert_text, "{}", msg_content);
                                                    }
                                                }
                                                if alert_text.is_empty() {
                                                    let _ = core::write!(alert_text, "Alert from {:08X}", sender_node);
                                                }

                                                Text::new(&alert_text, Point::new(30, 225), alert_style)
                                                    .draw(&mut display)
                                                    .unwrap();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        MessageType::Announce => {
                            esp_println::println!("[RX] ANNOUNCE from {:08X} @ {:?}", msg.node_id, src_ip);
                        }
                        MessageType::Heartbeat => {
                            // Suppress heartbeat logging to reduce noise
                            // esp_println::println!("[RX] HEARTBEAT from {:08X}", msg.node_id);
                        }
                        _ => {}
                    }
                }
            }
        }

        // Small delay
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(10) {}
    }
}
