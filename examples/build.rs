use mcschemora::{
    catalog::MinecraftData,
    formats,
    model::{Block, Bounds, Schematic, Selection},
};
use std::{error::Error, fs, path::Path, sync::Arc};

fn main() -> Result<(), Box<dyn Error>> {
    pollster::block_on(async {
        let data = Arc::new(MinecraftData::new(None, false)?);
        data.load("1.21.1").await?;
        let mut scene = Schematic::new("java", "1.21.1", data.clone())?;
        let floor = Selection::new(scene.region("main")?, Bounds::new([0, 0, 0], [7, 1, 7])?);
        scene.fill("main", &floor, &Block::parse("stone_bricks")?)?;
        println!("Validation: {:?}", scene.validate());

        let output = Path::new("examples/output");
        fs::create_dir_all(output)?;
        fs::write(
            output.join("floor.schem"),
            formats::encode(&scene, "schem", None, false, false).await?,
        )?;
        println!("Saved floor.schem");
        Ok(())
    })
}
