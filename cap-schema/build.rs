// Build script for generating Rust code from protobuf definitions

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_files = [
        "proto/common.proto",
        "proto/capability.proto",
        "proto/node.proto",
        "proto/cell.proto",
        "proto/beacon.proto",
        "proto/composition.proto",
        "proto/zone.proto",
        "proto/role.proto",
    ];

    // Configure prost to generate Rust code from .proto files
    let mut config = prost_build::Config::new();

    // Enable derive for common traits
    config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");

    // Enable proto3 optional fields
    config.protoc_arg("--experimental_allow_proto3_optional");

    // Generate code
    config.compile_protos(&proto_files, &["proto/"])?;

    // Tell cargo to recompile if any proto file changes
    for file in &proto_files {
        println!("cargo:rerun-if-changed={}", file);
    }

    Ok(())
}
