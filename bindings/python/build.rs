use std::{env, fs, io, path::PathBuf};

fn main() -> io::Result<()> {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    for (source, target) in [
        ("LICENSE", "licenses/LICENSE"),
        ("bindings/python/NOTICE", "licenses/NOTICE"),
        (
            "data/block-models/SOURCE.md",
            "licenses/block-models/SOURCE.md",
        ),
        ("data/block-models/LICENSE", "licenses/block-models/LICENSE"),
        (
            "data/wiki-sprites/SOURCE.md",
            "licenses/wiki-sprites/SOURCE.md",
        ),
        (
            "data/entity-models/README.md",
            "licenses/entity-models/README.md",
        ),
        (
            "src/catalog/storage/native/SOURCE.md",
            "licenses/runtime-data/SOURCE.md",
        ),
    ] {
        let source = root.join(source);
        let target = output.join(target);
        println!("cargo:rerun-if-changed={}", source.display());
        fs::create_dir_all(target.parent().unwrap())?;
        fs::copy(source, target)?;
    }
    for (source, target, fields) in [
        (
            "data/entity-models/entities.json",
            "licenses/entity-models/ATTRIBUTION.json",
            &["source"][..],
        ),
        (
            "data/wiki-sprites/sprites.json",
            "licenses/wiki-sprites/ATTRIBUTION.json",
            &["source", "files"][..],
        ),
    ] {
        let source = root.join(source);
        println!("cargo:rerun-if-changed={}", source.display());
        let data: serde_json::Value = serde_json::from_slice(&fs::read(source)?)?;
        let attribution: serde_json::Map<String, serde_json::Value> = fields
            .iter()
            .map(|&key| (key.to_owned(), data[key].clone()))
            .collect();
        let target = output.join(target);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::write(target, serde_json::to_vec(&attribution)?)?;
    }
    Ok(())
}
