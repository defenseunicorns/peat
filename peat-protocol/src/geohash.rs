//! Vendored geohash implementation for supply chain security.
//!
//! This is a reviewed copy of the geohash algorithm from the `geohash` crate (v0.13.1),
//! with external dependencies (`geo_types`, `libm`) removed. The algorithm is the public
//! [Geohash](https://en.wikipedia.org/wiki/Geohash) specification.
//!
//! Vendored as part of supply chain audit (2026-03-26) to eliminate dependency on a
//! crate whose primary maintainer is based in a US-sensitive country. The algorithm
//! itself is a well-known, deterministic geographic encoding with no security implications.
//!
//! Source: <https://github.com/georust/geohash.rs> (v0.13.1)
//! Original authors: Ning Sun, contributors
//!
//! ## License
//!
//! The original `geohash` crate is dual-licensed under MIT and Apache-2.0.
//! This vendored copy is used under the MIT license:
//!
//! ```text
//! Copyright (c) 2016 Ning Sun
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in
//! all copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
//! ```

use std::error::Error;
use std::fmt;

// --- Error types ---

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum GeohashError {
    InvalidHashCharacter(char),
    InvalidCoordinateRange { lon: f64, lat: f64 },
    InvalidLength(usize),
    InvalidHash(String),
}

impl fmt::Display for GeohashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GeohashError::InvalidHashCharacter(c) => write!(f, "invalid hash character: {c}"),
            GeohashError::InvalidCoordinateRange { lon, lat } => {
                write!(f, "invalid coordinate range: lon={lon}, lat={lat}")
            }
            GeohashError::InvalidLength(len) => write!(
                f,
                "invalid length: {len}. Must be between 1 and 12 inclusive",
            ),
            GeohashError::InvalidHash(msg) => write!(f, "invalid hash: {msg}"),
        }
    }
}

impl Error for GeohashError {}

// --- Direction & Neighbors ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Direction {
    pub fn to_tuple(self) -> (f64, f64) {
        match self {
            Direction::SW => (-1.0, -1.0),
            Direction::S => (-1.0, 0.0),
            Direction::SE => (-1.0, 1.0),
            Direction::W => (0.0, -1.0),
            Direction::E => (0.0, 1.0),
            Direction::NW => (1.0, -1.0),
            Direction::N => (1.0, 0.0),
            Direction::NE => (1.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Neighbors {
    pub sw: String,
    pub s: String,
    pub se: String,
    pub w: String,
    pub e: String,
    pub nw: String,
    pub n: String,
    pub ne: String,
}

// --- Base32 tables ---

#[rustfmt::skip]
const BASE32_CODES: [char; 32] = [
    '0', '1', '2', '3', '4', '5', '6', '7',
    '8', '9', 'b', 'c', 'd', 'e', 'f', 'g',
    'h', 'j', 'k', 'm', 'n', 'p', 'q', 'r',
    's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
];

#[rustfmt::skip]
const DECODER: [u8; 256] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
    0x08, 0x09, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0xff, 0x11, 0x12, 0xff, 0x13, 0x14, 0xff,
    0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
    0x1d, 0x1e, 0x1f, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

// --- Bit interleaving ---

#[inline]
fn spread(x: u32) -> u64 {
    let mut v = x as u64;
    v = (v | (v << 16)) & 0x0000ffff0000ffff;
    v = (v | (v << 8)) & 0x00ff00ff00ff00ff;
    v = (v | (v << 4)) & 0x0f0f0f0f0f0f0f0f;
    v = (v | (v << 2)) & 0x3333333333333333;
    v = (v | (v << 1)) & 0x5555555555555555;
    v
}

#[inline]
fn interleave(x: u32, y: u32) -> u64 {
    spread(x) | (spread(y) << 1)
}

#[inline]
fn squash(x: u64) -> u32 {
    let mut v = x & 0x5555555555555555;
    v = (v | (v >> 1)) & 0x3333333333333333;
    v = (v | (v >> 2)) & 0x0f0f0f0f0f0f0f0f;
    v = (v | (v >> 4)) & 0x00ff00ff00ff00ff;
    v = (v | (v >> 8)) & 0x0000ffff0000ffff;
    v = (v | (v >> 16)) & 0x00000000ffffffff;
    v as u32
}

#[inline]
fn deinterleave(x: u64) -> (u32, u32) {
    (squash(x), squash(x >> 1))
}

// --- Core functions ---

/// Encode a (lon, lat) coordinate to a geohash string of the given length.
pub fn encode(lon: f64, lat: f64, len: usize) -> Result<String, GeohashError> {
    if !(-180.0..=180.0).contains(&lon) || !(-90.0..=90.0).contains(&lat) {
        return Err(GeohashError::InvalidCoordinateRange { lon, lat });
    }
    if !(1..=12).contains(&len) {
        return Err(GeohashError::InvalidLength(len));
    }

    let lat32 = ((lat * 0.005555555555555556 + 1.5).to_bits() >> 20) as u32;
    let lon32 = ((lon * 0.002777777777777778 + 1.5).to_bits() >> 20) as u32;

    let mut interleaved = interleave(lat32, lon32);
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        let code = (interleaved >> 59) as usize & 0x1f;
        out.push(BASE32_CODES[code]);
        interleaved <<= 5;
    }
    Ok(out)
}

fn decode_range(x: u32, r: f64) -> f64 {
    let p = f64::from_bits(((x as u64) << 20) | (1023 << 52));
    2.0 * r * (p - 1.0) - r
}

fn error_with_precision(bits: u32) -> (f64, f64) {
    let lat_bits = bits / 2;
    let lon_bits = bits - lat_bits;
    // ldexp(x, n) = x * 2^n
    let lat_err = 180.0 * 2f64.powi(-(lat_bits as i32));
    let lon_err = 360.0 * 2f64.powi(-(lon_bits as i32));
    (lat_err, lon_err)
}

/// Decode a geohash string to ((lon, lat), lon_error, lat_error).
pub fn decode(hash_str: &str) -> Result<((f64, f64), f64, f64), GeohashError> {
    let bits = hash_str.len() * 5;
    if hash_str.len() > 12 {
        return Err(GeohashError::InvalidHash(
            "hash length exceeds maximum of 12".to_string(),
        ));
    }

    let mut int_hash: u64 = 0;
    for c in hash_str.bytes() {
        let val = DECODER[c as usize];
        if val == 0xff {
            return Err(GeohashError::InvalidHashCharacter(c as char));
        }
        int_hash <<= 5;
        int_hash |= val as u64;
    }

    let full_hash = int_hash << (64 - bits);
    let (lat_int, lon_int) = deinterleave(full_hash);
    let lat = decode_range(lat_int, 90.0);
    let lon = decode_range(lon_int, 180.0);
    let (lat_err, lon_err) = error_with_precision(bits as u32);

    Ok((
        ((lon + lon_err / 2.0), (lat + lat_err / 2.0)),
        lon_err / 2.0,
        lat_err / 2.0,
    ))
}

/// Find a neighboring geohash in the given direction.
pub fn neighbor(hash_str: &str, direction: Direction) -> Result<String, GeohashError> {
    let ((lon, lat), lon_err, lat_err) = decode(hash_str)?;
    let (dlat, dlon) = direction.to_tuple();
    let neighbor_lon = ((lon + 2.0 * lon_err.abs() * dlon) + 180.0).rem_euclid(360.0) - 180.0;
    let neighbor_lat = ((lat + 2.0 * lat_err.abs() * dlat) + 90.0).rem_euclid(180.0) - 90.0;
    encode(neighbor_lon, neighbor_lat, hash_str.len())
}

/// Find all 8 neighboring geohashes.
pub fn neighbors(hash_str: &str) -> Result<Neighbors, GeohashError> {
    Ok(Neighbors {
        sw: neighbor(hash_str, Direction::SW)?,
        s: neighbor(hash_str, Direction::S)?,
        se: neighbor(hash_str, Direction::SE)?,
        w: neighbor(hash_str, Direction::W)?,
        e: neighbor(hash_str, Direction::E)?,
        nw: neighbor(hash_str, Direction::NW)?,
        n: neighbor(hash_str, Direction::N)?,
        ne: neighbor(hash_str, Direction::NE)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode() {
        let hash = encode(-120.6623, 35.3003, 5).unwrap();
        assert_eq!(hash, "9q60y");
    }

    #[test]
    fn test_encode_long() {
        let hash = encode(-120.6623, 35.3003, 10).unwrap();
        assert_eq!(hash, "9q60y60rhs");
    }

    #[test]
    fn test_decode() {
        let ((lon, lat), _, _) = decode("9q60y").unwrap();
        assert!((lon - (-120.65185546875)).abs() < 1e-10);
        assert!((lat - 35.31005859375).abs() < 1e-10);
    }

    #[test]
    fn test_roundtrip() {
        let original_lon = -122.4194;
        let original_lat = 37.7749;
        let hash = encode(original_lon, original_lat, 7).unwrap();
        let ((lon, lat), _, _) = decode(&hash).unwrap();
        assert!((lon - original_lon).abs() < 0.001);
        assert!((lat - original_lat).abs() < 0.001);
    }

    #[test]
    fn test_neighbor() {
        let n = neighbor("9q60y60rhs", Direction::N).unwrap();
        assert_eq!(n, "9q60y60rht");
    }

    #[test]
    fn test_neighbors() {
        let n = neighbors("9q60y60rhs").unwrap();
        assert_eq!(n.n, "9q60y60rht");
        assert_eq!(n.ne, "9q60y60rhv");
        assert_eq!(n.e, "9q60y60rhu");
        assert_eq!(n.se, "9q60y60rhg");
        assert_eq!(n.s, "9q60y60rhe");
        assert_eq!(n.sw, "9q60y60rh7");
        assert_eq!(n.w, "9q60y60rhk");
        assert_eq!(n.nw, "9q60y60rhm");
    }

    #[test]
    fn test_invalid_coordinate() {
        assert!(encode(200.0, 0.0, 5).is_err());
        assert!(encode(0.0, 100.0, 5).is_err());
    }

    #[test]
    fn test_invalid_hash() {
        assert!(decode("invalid!").is_err());
    }

    #[test]
    fn test_sf_geohash() {
        let hash = encode(-122.4194, 37.7749, 7).unwrap();
        assert!(hash.starts_with("9q8yy"));
    }
}
