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

/// MPU6886 IMU (I2C address 0x68)
#[cfg(feature = "m5stack-core2")]
const MPU6886_ADDR: u8 = 0x68;

/// AXP192 Power Management IC (I2C address 0x34)
#[cfg(feature = "m5stack-core2")]
const AXP192_ADDR: u8 = 0x34;

// MPU6886 Register addresses (compatible with MPU6050 family)
#[cfg(feature = "m5stack-core2")]
mod mpu6886_regs {
    pub const PWR_MGMT_1: u8 = 0x6B;    // Power management
    pub const PWR_MGMT_2: u8 = 0x6C;    // Power management 2
    pub const SMPLRT_DIV: u8 = 0x19;    // Sample rate divider
    pub const CONFIG: u8 = 0x1A;        // Configuration
    pub const GYRO_CONFIG: u8 = 0x1B;   // Gyroscope configuration
    pub const ACCEL_CONFIG: u8 = 0x1C;  // Accelerometer configuration
    pub const ACCEL_CONFIG2: u8 = 0x1D; // Accelerometer configuration 2
    pub const ACCEL_XOUT_H: u8 = 0x3B;  // Accelerometer X high byte
    pub const GYRO_XOUT_H: u8 = 0x43;   // Gyroscope X high byte
    pub const WHO_AM_I: u8 = 0x75;      // Device ID (should return 0x19 for MPU6886)
}

// AXP192 Register addresses
#[cfg(feature = "m5stack-core2")]
mod axp192_regs {
    pub const POWER_STATUS: u8 = 0x00;      // Power status
    pub const CHARGE_STATUS: u8 = 0x01;     // Charge status
    pub const EXTEN_DCDC2: u8 = 0x10;       // EXTEN & DC-DC2 control
    pub const DCDC13_LDO23: u8 = 0x12;      // DC-DC1/3 & LDO2/3 control
    pub const DCDC2_VOLTAGE: u8 = 0x23;     // DC-DC2 voltage setting
    pub const DCDC2_SLOPE: u8 = 0x25;       // DC-DC2 voltage slope
    pub const DCDC1_VOLTAGE: u8 = 0x26;     // DC-DC1 voltage setting
    pub const DCDC3_VOLTAGE: u8 = 0x27;     // DC-DC3 voltage setting
    pub const LDO23_VOLTAGE: u8 = 0x28;     // LDO2/3 voltage setting
    pub const VBUS_IPSOUT: u8 = 0x30;       // VBUS-IPSOUT path setting
    pub const VOFF_SETTING: u8 = 0x31;      // VOFF shutdown voltage
    pub const POWEROFF_SETTING: u8 = 0x32;  // Shutdown/battery detection
    pub const CHARGE_CTRL1: u8 = 0x33;      // Charge control 1
    pub const CHARGE_CTRL2: u8 = 0x34;      // Charge control 2
    pub const BACKUP_BATT: u8 = 0x35;       // Backup battery charging
    pub const PEK_SETTING: u8 = 0x36;       // Power key (PEK) settings
    pub const DCDC_FREQ: u8 = 0x37;         // DC-DC converter frequency
    pub const VLTF_CHARGE: u8 = 0x38;       // Low temp threshold (charge)
    pub const VHTF_CHARGE: u8 = 0x39;       // High temp threshold (charge)
    pub const APS_LOW_WARN1: u8 = 0x3A;     // APS low voltage warning 1
    pub const APS_LOW_WARN2: u8 = 0x3B;     // APS low voltage warning 2
    pub const ADC_ENABLE1: u8 = 0x82;       // ADC enable 1
    pub const ADC_ENABLE2: u8 = 0x83;       // ADC enable 2
    pub const GPIO0_CTRL: u8 = 0x90;        // GPIO0 control
    pub const GPIO0_LDO_VOLT: u8 = 0x91;    // GPIO0 LDO voltage
    pub const GPIO1_CTRL: u8 = 0x92;        // GPIO1 control
    pub const GPIO2_CTRL: u8 = 0x93;        // GPIO2 control
    pub const GPIO012_STATE: u8 = 0x94;     // GPIO0/1/2 signal status
    pub const GPIO34_CTRL: u8 = 0x95;       // GPIO3/4 control
    pub const BAT_POWER_H: u8 = 0x70;       // Battery power high byte
    pub const BAT_VOLTAGE_H: u8 = 0x78;     // Battery voltage high byte
    pub const BAT_VOLTAGE_L: u8 = 0x79;     // Battery voltage low byte
    pub const BAT_PERCENT: u8 = 0xB9;       // Battery percentage (fuel gauge)
}

/// Sensor readings from MPU6886
#[cfg(feature = "m5stack-core2")]
#[derive(Clone, Copy, Default)]
pub struct ImuData {
    pub accel_x: i16,
    pub accel_y: i16,
    pub accel_z: i16,
    pub gyro_x: i16,
    pub gyro_y: i16,
    pub gyro_z: i16,
}

/// Posture detection result
#[cfg(feature = "m5stack-core2")]
#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum Posture {
    Unknown = 0,
    Standing = 1,
    Prone = 2,
    Moving = 3,
}

/// Initialize MPU6886 IMU sensor
#[cfg(feature = "m5stack-core2")]
fn mpu6886_init<I2C>(i2c: &mut I2C) -> bool
where
    I2C: embedded_hal::i2c::I2c,
{
    use mpu6886_regs::*;

    // Check WHO_AM_I register (should be 0x19 for MPU6886)
    let mut buf = [0u8; 1];
    if i2c.write_read(MPU6886_ADDR, &[WHO_AM_I], &mut buf).is_err() {
        esp_println::println!("MPU6886: Failed to read WHO_AM_I");
        return false;
    }
    esp_println::println!("MPU6886: WHO_AM_I = 0x{:02X}", buf[0]);

    // Reset device
    if i2c.write(MPU6886_ADDR, &[PWR_MGMT_1, 0x80]).is_err() {
        return false;
    }
    // Wait for reset
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(100) {}

    // Wake up (clear sleep bit), use internal oscillator
    if i2c.write(MPU6886_ADDR, &[PWR_MGMT_1, 0x00]).is_err() {
        return false;
    }

    // Set sample rate divider (1kHz / (1 + 9) = 100Hz)
    if i2c.write(MPU6886_ADDR, &[SMPLRT_DIV, 9]).is_err() {
        return false;
    }

    // Configure gyro: ±500 dps (FS_SEL = 1)
    if i2c.write(MPU6886_ADDR, &[GYRO_CONFIG, 0x08]).is_err() {
        return false;
    }

    // Configure accelerometer: ±4g (AFS_SEL = 1)
    if i2c.write(MPU6886_ADDR, &[ACCEL_CONFIG, 0x08]).is_err() {
        return false;
    }

    // Configure accelerometer filter
    if i2c.write(MPU6886_ADDR, &[ACCEL_CONFIG2, 0x00]).is_err() {
        return false;
    }

    esp_println::println!("MPU6886: Initialized");
    true
}

/// Read accelerometer and gyroscope data from MPU6886
#[cfg(feature = "m5stack-core2")]
fn mpu6886_read<I2C>(i2c: &mut I2C) -> Option<ImuData>
where
    I2C: embedded_hal::i2c::I2c,
{
    let mut buf = [0u8; 14];
    // Read 14 bytes starting from ACCEL_XOUT_H (accel: 6 bytes, temp: 2 bytes, gyro: 6 bytes)
    if i2c.write_read(MPU6886_ADDR, &[mpu6886_regs::ACCEL_XOUT_H], &mut buf).is_err() {
        return None;
    }

    Some(ImuData {
        accel_x: i16::from_be_bytes([buf[0], buf[1]]),
        accel_y: i16::from_be_bytes([buf[2], buf[3]]),
        accel_z: i16::from_be_bytes([buf[4], buf[5]]),
        // buf[6], buf[7] = temperature (skip)
        gyro_x: i16::from_be_bytes([buf[8], buf[9]]),
        gyro_y: i16::from_be_bytes([buf[10], buf[11]]),
        gyro_z: i16::from_be_bytes([buf[12], buf[13]]),
    })
}

/// Calculate activity level (0-100) from accelerometer magnitude
#[cfg(feature = "m5stack-core2")]
fn calculate_activity_level(imu: &ImuData) -> u8 {
    // Calculate magnitude of acceleration (in raw units)
    // At ±4g setting, 1g ≈ 8192 counts
    let ax = imu.accel_x as i32;
    let ay = imu.accel_y as i32;
    let az = imu.accel_z as i32;

    // Magnitude squared (avoid sqrt for speed)
    let mag_sq = ax * ax + ay * ay + az * az;

    // At rest (1g), mag_sq ≈ 8192² ≈ 67M
    // Activity is deviation from 1g
    let rest_mag_sq: i32 = 8192 * 8192;
    let deviation = ((mag_sq - rest_mag_sq).abs() / 1000000) as u8;

    // Dead zone: readings below 15 are sensor noise, treat as 0
    let filtered = if deviation < 15 { 0 } else { deviation - 15 };

    // Scale to 0-100, cap at 100
    filtered.min(100)
}

/// Detect posture from accelerometer data
#[cfg(feature = "m5stack-core2")]
fn detect_posture(imu: &ImuData, activity: u8) -> Posture {
    // If high activity, we're moving
    if activity > 30 {
        return Posture::Moving;
    }

    // At ±4g, 1g ≈ 8192 counts
    // Check which axis has gravity
    let ax = imu.accel_x.abs();
    let ay = imu.accel_y.abs();
    let az = imu.accel_z.abs();

    // Core2 orientation when worn on chest:
    // - Standing/upright: X or Y axis has gravity (screen facing out)
    // - Prone/lying: Z axis has gravity (screen facing up/down)
    if (ax > az || ay > az) && (ax > 6000 || ay > 6000) {
        Posture::Standing
    } else if az > ax && az > ay && az > 6000 {
        Posture::Prone
    } else {
        Posture::Unknown
    }
}

/// Read battery percentage from AXP192
#[cfg(feature = "m5stack-core2")]
fn axp192_read_battery<I2C>(i2c: &mut I2C) -> Option<u8>
where
    I2C: embedded_hal::i2c::I2c,
{
    // Read battery voltage (12-bit ADC split across two registers)
    let mut buf = [0u8; 2];
    if i2c.write_read(AXP192_ADDR, &[axp192_regs::BAT_VOLTAGE_H], &mut buf).is_err() {
        return None;
    }

    // Battery voltage: (H << 4) | L, in 1.1mV units
    let voltage = ((buf[0] as u16) << 4) | ((buf[1] as u16) & 0x0F);
    let mv = voltage as u32 * 11 / 10; // Convert to mV

    // Estimate percentage from voltage (Li-ion: 3.0V=0%, 4.2V=100%)
    // 3000mV = 0%, 4200mV = 100%
    let percent = if mv < 3000 {
        0
    } else if mv > 4200 {
        100
    } else {
        ((mv - 3000) * 100 / 1200) as u8
    };

    Some(percent)
}

/// Check if battery is charging from AXP192
#[cfg(feature = "m5stack-core2")]
fn axp192_is_charging<I2C>(i2c: &mut I2C) -> bool
where
    I2C: embedded_hal::i2c::I2c,
{
    let mut buf = [0u8; 1];
    if i2c.write_read(AXP192_ADDR, &[axp192_regs::CHARGE_STATUS], &mut buf).is_ok() {
        // Bit 6 indicates charging
        (buf[0] & 0x40) != 0
    } else {
        false
    }
}

/// Initialize AXP192 Power Management IC - SAFE VERSION
///
/// ⚠️ WARNING: Only modifies the PEK (power button) register.
/// DO NOT modify voltage rail registers (0x12, 0x26, 0x27, 0x28) - this can
/// permanently brick the device! See ADR-035 Appendix C for details.
///
/// If the power button doesn't work properly after flashing custom firmware,
/// use M5Burner to restore factory firmware instead of trying to fix it here.
#[cfg(feature = "m5stack-core2")]
fn axp192_init<I2C>(i2c: &mut I2C) -> bool
where
    I2C: embedded_hal::i2c::I2c,
{
    use axp192_regs::*;

    esp_println::println!("AXP192: Configuring power button (safe mode)...");

    // Check if AXP192 is present by reading power status
    let mut buf = [0u8; 1];
    if i2c.write_read(AXP192_ADDR, &[POWER_STATUS], &mut buf).is_err() {
        esp_println::println!("AXP192: Not found!");
        return false;
    }
    esp_println::println!("AXP192: Power status = 0x{:02X}", buf[0]);

    // === Power Key (PEK) Settings - ONLY SAFE REGISTER TO MODIFY ===
    // Register 0x36: PEK_SETTING
    // Bits 7-6: Boot time (00=128ms, 01=512ms, 10=1s, 11=2s)
    // Bits 5-4: Long press time (00=1s, 01=1.5s, 10=2s, 11=2.5s)
    // Bit 3: Long press power off enable (1=enable)
    // Bit 2: PWROK signal delay after power on (1=64ms, 0=32ms)
    // Bits 1-0: Shutdown delay (00=4s, 01=6s, 10=8s, 11=10s)
    //
    // Value 0x4C = 0b01001100:
    //   Boot time: 512ms (01)
    //   Long press time: 1s (00)
    //   Long press power off: enabled (1)
    //   PWROK delay: 64ms (1)
    //   Shutdown delay: 4s (00)
    if i2c.write(AXP192_ADDR, &[PEK_SETTING, 0x4C]).is_err() {
        esp_println::println!("AXP192: Failed to set PEK");
        return false;
    }

    esp_println::println!("AXP192: PEK configured (boot=512ms, long=1s, shutdown=4s)");

    // NOTE: We intentionally DO NOT touch any other registers.
    // The factory firmware has already configured voltage rails correctly.
    // Modifying DCDC1, DCDC3, LDO2/3 voltages can brick the device permanently.
    // See ADR-035 Appendix C for the incident report.

    true
}

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

    // Initialize I2C for power management and sensors (MUST be before display)
    #[cfg(feature = "m5stack-core2")]
    let mut i2c = {
        let sda = peripherals.GPIO21;
        let scl = peripherals.GPIO22;
        I2c::new(peripherals.I2C0, I2cConfig::default())
            .unwrap()
            .with_sda(sda)
            .with_scl(scl)
    };

    // NOTE: We do NOT initialize the AXP192 Power Management IC.
    // The factory firmware already configures it correctly, including power button behavior.
    // Modifying AXP192 registers can permanently brick the device - see ADR-035 Appendix C.

    // Initialize display (after AXP192 enables backlight power on GPIO0)
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

        // Sensor dashboard layout
        // Left column: labels
        Text::new("Activity:", Point::new(20, 95), text_style).draw(&mut disp).unwrap();
        Text::new("Posture:", Point::new(20, 115), text_style).draw(&mut disp).unwrap();
        Text::new("Battery:", Point::new(20, 135), text_style).draw(&mut disp).unwrap();
        Text::new("Taps:", Point::new(20, 155), text_style).draw(&mut disp).unwrap();

        // Right column: initial values
        Text::new("--", Point::new(130, 95), counter_style).draw(&mut disp).unwrap();
        Text::new("--", Point::new(130, 115), counter_style).draw(&mut disp).unwrap();
        Text::new("--%", Point::new(130, 135), counter_style).draw(&mut disp).unwrap();
        Text::new("0", Point::new(130, 155), counter_style).draw(&mut disp).unwrap();

        // Instructions at bottom
        Text::new("Tap screen for panic button", Point::new(40, 185), text_style)
            .draw(&mut disp)
            .unwrap();

        esp_println::println!("Display initialized");
        disp
    };

    // Initialize MPU6886 IMU
    #[cfg(feature = "m5stack-core2")]
    let imu_available = mpu6886_init(&mut i2c);

    // Node setup
    let node_id: u32 = 0x4D355443;
    let mut capabilities = NodeCapabilities::lite();
    capabilities.set(NodeCapabilities::DISPLAY_OUTPUT);
    capabilities.set(NodeCapabilities::SENSOR_INPUT);

    let mut button_presses: GCounter = GCounter::new(node_id);

    // Sensor state LWW-Registers (initialized with default values)
    let mut activity_level: LwwRegister<u8, 1> = LwwRegister::new(0, 0, node_id);
    let mut posture_reg: LwwRegister<u8, 1> = LwwRegister::new(0, 0, node_id);
    let mut battery_percent: LwwRegister<u8, 1> = LwwRegister::new(0, 0, node_id);

    // Sensor reading state
    #[cfg(feature = "m5stack-core2")]
    let mut last_sensor_read = Instant::now();
    #[cfg(feature = "m5stack-core2")]
    let sensor_read_interval = Duration::from_millis(500); // Read sensors at 2Hz
    #[cfg(feature = "m5stack-core2")]
    let mut last_sensor_publish = Instant::now();
    #[cfg(feature = "m5stack-core2")]
    let sensor_publish_interval = Duration::from_secs(1); // Publish to mesh at 1Hz
    #[cfg(feature = "m5stack-core2")]
    let mut current_activity: u8 = 0;
    #[cfg(feature = "m5stack-core2")]
    let mut current_posture: Posture = Posture::Unknown;
    #[cfg(feature = "m5stack-core2")]
    let mut current_battery: u8 = 0;

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

            // Update display - Taps row
            #[cfg(feature = "m5stack-core2")]
            {
                let counter_style = MonoTextStyle::new(&FONT_10X20, Rgb565::new(31, 63, 0)); // Green-yellow
                let clear_style = PrimitiveStyle::with_fill(Rgb565::new(0, 0, 0));

                // Clear taps value area
                Rectangle::new(Point::new(125, 140), Size::new(80, 20))
                    .into_styled(clear_style)
                    .draw(&mut display)
                    .unwrap();

                // Draw new count
                let mut buf = heapless::String::<16>::new();
                let _ = core::write!(buf, "{}", count);
                Text::new(&buf, Point::new(130, 155), counter_style)
                    .draw(&mut display)
                    .unwrap();
            }
        }

        // Read sensors periodically
        #[cfg(feature = "m5stack-core2")]
        if imu_available && last_sensor_read.elapsed() >= sensor_read_interval {
            last_sensor_read = Instant::now();

            // Read IMU data
            if let Some(imu_data) = mpu6886_read(&mut i2c) {
                current_activity = calculate_activity_level(&imu_data);
                current_posture = detect_posture(&imu_data, current_activity);
            }

            // Read battery
            if let Some(batt) = axp192_read_battery(&mut i2c) {
                current_battery = batt;
            }

            // Update LWW-Registers with current timestamp
            let now_ms = Instant::now().duration_since_epoch().as_millis() as u64;
            activity_level.set(current_activity, now_ms, node_id);
            posture_reg.set(current_posture as u8, now_ms, node_id);
            battery_percent.set(current_battery, now_ms, node_id);

            // Update display
            {
                let counter_style = MonoTextStyle::new(&FONT_10X20, Rgb565::new(31, 63, 0)); // Green-yellow
                let clear_style = PrimitiveStyle::with_fill(Rgb565::new(0, 0, 0));

                // Update Activity value
                Rectangle::new(Point::new(125, 80), Size::new(100, 20))
                    .into_styled(clear_style)
                    .draw(&mut display)
                    .unwrap();
                let mut buf = heapless::String::<16>::new();
                let _ = core::write!(buf, "{}%", current_activity);
                Text::new(&buf, Point::new(130, 95), counter_style)
                    .draw(&mut display)
                    .unwrap();

                // Update Posture value
                Rectangle::new(Point::new(125, 100), Size::new(100, 20))
                    .into_styled(clear_style)
                    .draw(&mut display)
                    .unwrap();
                let posture_str = match current_posture {
                    Posture::Unknown => "???",
                    Posture::Standing => "STAND",
                    Posture::Prone => "PRONE",
                    Posture::Moving => "MOVE",
                };
                Text::new(posture_str, Point::new(130, 115), counter_style)
                    .draw(&mut display)
                    .unwrap();

                // Update Battery value
                Rectangle::new(Point::new(125, 120), Size::new(100, 20))
                    .into_styled(clear_style)
                    .draw(&mut display)
                    .unwrap();
                let mut bat_buf = heapless::String::<16>::new();
                let _ = core::write!(bat_buf, "{}%", current_battery);
                // Color code battery: green > 50%, yellow 20-50%, red < 20%
                let bat_color = if current_battery > 50 {
                    Rgb565::new(0, 63, 0) // Green
                } else if current_battery > 20 {
                    Rgb565::new(31, 63, 0) // Yellow
                } else {
                    Rgb565::new(31, 0, 0) // Red
                };
                let bat_style = MonoTextStyle::new(&FONT_10X20, bat_color);
                Text::new(&bat_buf, Point::new(130, 135), bat_style)
                    .draw(&mut display)
                    .unwrap();
            }
        }

        // Publish sensors to mesh periodically
        #[cfg(feature = "m5stack-core2")]
        if last_sensor_publish.elapsed() >= sensor_publish_interval {
            last_sensor_publish = Instant::now();

            // Publish activity level as LWW-Register
            let mut crdt_buf = [0u8; 64];
            if let Ok(crdt_len) = activity_level.encode(&mut crdt_buf) {
                if let Some(data_msg) = Message::data(node_id, seq_num, CrdtType::LwwRegister as u8, &crdt_buf[..crdt_len]) {
                    seq_num += 1;
                    let mut pkt_buf = [0u8; MAX_PACKET_SIZE];
                    if let Ok(len) = data_msg.encode(&mut pkt_buf) {
                        let _ = udp_socket.send(broadcast_addr.into(), HIVE_UDP_PORT, &pkt_buf[..len]);
                    }
                }
            }

            // Publish posture as LWW-Register
            if let Ok(crdt_len) = posture_reg.encode(&mut crdt_buf) {
                if let Some(data_msg) = Message::data(node_id, seq_num, CrdtType::LwwRegister as u8, &crdt_buf[..crdt_len]) {
                    seq_num += 1;
                    let mut pkt_buf = [0u8; MAX_PACKET_SIZE];
                    if let Ok(len) = data_msg.encode(&mut pkt_buf) {
                        let _ = udp_socket.send(broadcast_addr.into(), HIVE_UDP_PORT, &pkt_buf[..len]);
                    }
                }
            }

            // Publish battery as LWW-Register
            if let Ok(crdt_len) = battery_percent.encode(&mut crdt_buf) {
                if let Some(data_msg) = Message::data(node_id, seq_num, CrdtType::LwwRegister as u8, &crdt_buf[..crdt_len]) {
                    seq_num += 1;
                    let mut pkt_buf = [0u8; MAX_PACKET_SIZE];
                    if let Ok(len) = data_msg.encode(&mut pkt_buf) {
                        let _ = udp_socket.send(broadcast_addr.into(), HIVE_UDP_PORT, &pkt_buf[..len]);
                    }
                }
            }

            esp_println::println!("[SENSORS] activity={}% posture={:?} battery={}%",
                current_activity, current_posture as u8, current_battery);
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
