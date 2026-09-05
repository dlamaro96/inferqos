fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    let mut prost = prost_build::Config::new();
    prost.protoc_executable(protoc);
    prost.bytes([
        ".inferqos.provider.v1.EstimateRequest.body",
        ".inferqos.provider.v1.DispatchRequest.body",
        ".inferqos.provider.v1.DispatchResponse.data",
    ]);
    tonic_prost_build::configure().compile_with_config(
        prost,
        &["../../protocol/provider/v1/provider.proto"],
        &["../../protocol"],
    )?;
    println!("cargo:rerun-if-changed=../../protocol/provider/v1/provider.proto");
    Ok(())
}
