use std::{env, fs, io, path::PathBuf};

fn main() -> io::Result<()> {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    for (source, target) in [
        (
            "data/block-models/SOURCE.md",
            "licenses/block-models/SOURCE.md",
        ),
        ("data/block-models/LICENSE", "licenses/block-models/LICENSE"),
        (
            "data/wiki-sprites/SOURCE.md",
            "licenses/wiki-sprites/SOURCE.md",
        ),
    ] {
        let source = root.join(source);
        let target = output.join(target);
        println!("cargo:rerun-if-changed={}", source.display());
        fs::create_dir_all(target.parent().unwrap())?;
        fs::copy(source, target)?;
    }
    Ok(())
}
