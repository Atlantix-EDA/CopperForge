use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

fn main() {
    generate_protobuf_bindings();
}

fn generate_protobuf_bindings() {
    println!("cargo::rerun-if-changed=proto");

    let api_path = PathBuf::from("proto");

    if !api_path.exists() || !api_path.is_dir() {
        println!("The protobuf files are not present in the proto directory.");
        std::process::exit(1);
    }

    let mut protos: Vec<PathBuf> = vec![];
    let walker = WalkDir::new(api_path).into_iter();

    fn is_proto(e: &DirEntry) -> bool {
        e.file_name()
            .to_str()
            .map(|s| s.ends_with(".proto"))
            .unwrap_or(false)
    }

    for entry in walker.filter_entry(|e| is_proto(e) || e.file_type().is_dir()) {
        if !entry.as_ref().unwrap().file_type().is_dir() {
            protos.push(entry.unwrap().into_path());
        }
    }

    protobuf_codegen::Codegen::new()
        .protoc()
        .protoc_extra_arg("--experimental_allow_proto3_optional")
        .include(Path::new("proto"))
        .inputs(&protos)
        .cargo_out_dir("proto")
        .run_from_script();
}