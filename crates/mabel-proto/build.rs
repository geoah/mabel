fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc");
    // Safety: build scripts are single-threaded at this point.
    unsafe { std::env::set_var("PROTOC", protoc) };

    let protos = [
        "../../proto/mabel/v0/ledger.proto",
        "../../proto/mabel/v0/files.proto",
        "../../proto/mabel/v0/sync.proto",
    ];
    for p in &protos {
        println!("cargo:rerun-if-changed={p}");
    }
    prost_build::Config::new()
        .compile_protos(&protos, &["../../proto"])
        .expect("compile mabel protos");
}
