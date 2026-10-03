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
        let mut schematic = Schematic::new("java", "1.21.1", data.clone())?;
        let floor = Selection::new(
            schematic.region("main")?,
            Bounds::new([0, 0, 0], [7, 1, 7])?,
        );
        schematic.fill("main", &floor, &Block::parse("stone_bricks")?)?;
        println!("Validation: {:?}", schematic.validate());

        let output = Path::new("examples/output");
        fs::create_dir_all(output)?;
        fs::write(
            output.join("floor.schem"),
            formats::encode(&schematic, "schem", None, false, false).await?,
        )?;
        println!("Saved floor.schem");
        Ok(())
    })
}
