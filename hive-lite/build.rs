use std::io::Write;

fn main() {
    // Use the esp-hal linkall.x linker script for ESP32 targets only
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("xtensa") || target.contains("riscv32imc-esp") {
        println!("cargo:rustc-link-arg=-Tlinkall.x");
    }

    // OTA signing public key: if OTA_SIGNING_PUBKEY is set (64 hex chars = 32 bytes),
    // generate a const with the key bytes. Otherwise generate None (skip verification).
    println!("cargo:rerun-if-env-changed=OTA_SIGNING_PUBKEY");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let pubkey_path = std::path::Path::new(&out_dir).join("ota_pubkey.rs");
    let mut f = std::fs::File::create(pubkey_path).unwrap();

    if let Ok(hex_key) = std::env::var("OTA_SIGNING_PUBKEY") {
        let hex_key = hex_key.trim();
        if hex_key.len() == 64 {
            // Decode 64 hex chars to 32 bytes
            let mut bytes = [0u8; 32];
            let mut valid = true;
            for i in 0..32 {
                match u8::from_str_radix(&hex_key[i * 2..i * 2 + 2], 16) {
                    Ok(b) => bytes[i] = b,
                    Err(_) => {
                        valid = false;
                        break;
                    }
                }
            }
            if valid {
                write!(
                    f,
                    "pub const OTA_SIGNING_PUBKEY: Option<[u8; 32]> = Some([{}]);\n",
                    bytes
                        .iter()
                        .map(|b| format!("0x{:02x}", b))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
                .unwrap();
                return;
            } else {
                panic!(
                    "OTA_SIGNING_PUBKEY contains invalid hex characters: {}",
                    hex_key
                );
            }
        } else {
            panic!(
                "OTA_SIGNING_PUBKEY must be exactly 64 hex characters (32 bytes), got {} chars",
                hex_key.len()
            );
        }
    }

    // No pubkey configured — signature verification disabled
    writeln!(f, "pub const OTA_SIGNING_PUBKEY: Option<[u8; 32]> = None;").unwrap();
}
