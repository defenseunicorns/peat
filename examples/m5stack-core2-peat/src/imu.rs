// MPU6886 IMU driver for M5Stack Core2
// Provides accelerometer reading and simple activity classification.

use esp_idf_hal::i2c::I2cDriver;
use log::{info, warn};

const MPU6886_ADDR: u8 = 0x68;
const REG_WHO_AM_I: u8 = 0x75;
const REG_PWR_MGMT_1: u8 = 0x6B;
const REG_ACCEL_CONFIG: u8 = 0x1C;
const REG_ACCEL_XOUT_H: u8 = 0x3B;

const EXPECTED_WHO_AM_I: u8 = 0x19; // MPU6886

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Activity {
    /// Device upright (gravity not on the z-axis).
    Standing,
    /// Device lying flat (gravity dominantly on the z-axis).
    Prone,
    /// Sudden high-g spike or freefall signature.
    PossibleFall,
}

#[derive(Debug, Clone, Copy)]
pub struct Accel {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Accel {
    pub fn magnitude(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
}

/// |accel.z| (in g) above this means the device is lying flat — taken as prone.
const PRONE_Z_THRESHOLD_G: f32 = 0.7;

/// High-g threshold for fall-impact detection. Set well above anything a human
/// can produce handling the device (a device flip briefly crosses 3 g; actual
/// fall impacts typically exceed 6 g). Raised from an earlier 3.0 g after the
/// demo flip accidentally auto-triggered EMERGENCY.
const FALL_IMPACT_G: f32 = 5.0;

/// Low-g (freefall) threshold.
const FALL_FREEFALL_G: f32 = 0.3;

/// Number of back-to-back fall-candidate samples required before we call it.
/// At the 100 ms IMU poll cadence this is ~`N * 100 ms` of sustained signal.
const FALL_CONFIRM_SAMPLES: u8 = 3;

pub struct ImuState {
    last_classify_ms: u64,
    last_activity: Activity,
    /// Consecutive samples currently matching the fall signature. Resets as
    /// soon as one sample disagrees, so a single noisy spike never escalates.
    fall_streak: u8,
}

impl Default for ImuState {
    fn default() -> Self {
        Self {
            last_classify_ms: 0,
            last_activity: Activity::Standing,
            fall_streak: 0,
        }
    }
}

impl ImuState {
    pub fn update(&mut self, accel: &Accel, now_ms: u64) -> Activity {
        let mag = accel.magnitude();

        // Per-sample fall-candidate check + streak debounce. Only declare a
        // PossibleFall once we've seen FALL_CONFIRM_SAMPLES in a row — this
        // filters out incidental high-g spikes from device handling.
        let sample_looks_like_fall = mag > FALL_IMPACT_G || mag < FALL_FREEFALL_G;
        if sample_looks_like_fall {
            self.fall_streak = self.fall_streak.saturating_add(1);
        } else {
            self.fall_streak = 0;
        }
        let confirmed_fall = self.fall_streak >= FALL_CONFIRM_SAMPLES;

        // Fast path between classifications: fall only fires after a confirmed
        // streak; otherwise keep the last stable classification so the badge
        // doesn't flicker Standing/Prone on a single noisy sample.
        if now_ms.saturating_sub(self.last_classify_ms) < 500 {
            if confirmed_fall {
                return Activity::PossibleFall;
            }
            return self.last_activity;
        }

        self.last_classify_ms = now_ms;

        let activity = if confirmed_fall {
            Activity::PossibleFall
        } else if accel.z.abs() > PRONE_Z_THRESHOLD_G {
            Activity::Prone
        } else {
            Activity::Standing
        };

        self.last_activity = activity;
        activity
    }
}

pub fn init(i2c: &mut I2cDriver) -> bool {
    let mut buf = [0u8; 1];

    // Check WHO_AM_I
    if i2c.write_read(MPU6886_ADDR, &[REG_WHO_AM_I], &mut buf, 100).is_err() {
        warn!("MPU6886: I2C read failed");
        return false;
    }
    if buf[0] != EXPECTED_WHO_AM_I {
        warn!("MPU6886: unexpected WHO_AM_I=0x{:02X} (expected 0x{:02X})", buf[0], EXPECTED_WHO_AM_I);
        // Continue anyway — some Core2 v1.1 units have a different ID
    }

    // Wake up (clear sleep bit)
    if i2c.write(MPU6886_ADDR, &[REG_PWR_MGMT_1, 0x00], 100).is_err() {
        warn!("MPU6886: failed to wake");
        return false;
    }
    esp_idf_hal::delay::FreeRtos::delay_ms(10);

    // Set accel range to ±4g (enough for fall detection)
    let _ = i2c.write(MPU6886_ADDR, &[REG_ACCEL_CONFIG, 0x08], 100);

    info!("MPU6886: initialized (±4g)");
    true
}

pub fn read_accel(i2c: &mut I2cDriver) -> Option<Accel> {
    let mut buf = [0u8; 6];
    if i2c.write_read(MPU6886_ADDR, &[REG_ACCEL_XOUT_H], &mut buf, 100).is_err() {
        return None;
    }

    let raw_x = i16::from_be_bytes([buf[0], buf[1]]);
    let raw_y = i16::from_be_bytes([buf[2], buf[3]]);
    let raw_z = i16::from_be_bytes([buf[4], buf[5]]);

    // ±4g range: 8192 LSB/g
    let scale = 4.0 / 32768.0;
    Some(Accel {
        x: raw_x as f32 * scale,
        y: raw_y as f32 * scale,
        z: raw_z as f32 * scale,
    })
}
