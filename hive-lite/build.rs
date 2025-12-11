fn main() {
    // Use the esp-hal linkall.x linker script which includes all necessary sections
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}
