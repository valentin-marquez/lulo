//! Generates the Steam message types from the descriptor set extracted from steamclient.so.
//! Using the binary set means no `protoc` is needed to build.

use prost::Message;

fn main() {
    println!("cargo:rerun-if-changed=proto/steam.fds.pb");
    let bytes = std::fs::read("proto/steam.fds.pb").expect("proto/steam.fds.pb exists");
    let set =
        prost_types::FileDescriptorSet::decode(bytes.as_slice()).expect("valid descriptor set");
    prost_build::Config::new()
        .compile_fds(set)
        .expect("generate Steam protobuf types");
}
