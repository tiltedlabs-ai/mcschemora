use super::*;

pub(super) fn state(block: &Block, context: &Context) -> Result<Block> {
    let mut block = block.clone();
    block.id = context.rename("block", &block.id)?;
    context.target.resolve(&block)
}

pub(super) fn nbt(data: &mut Compound, context: &Context) -> Result<()> {
    let name = text(data, "Name")?;
    let mut properties = std::collections::BTreeMap::new();
    if let Some(value) = data.get("Properties") {
        for (k, v) in crate::nbt::compound(value)? {
            properties.insert(k.clone(), crate::nbt::string(v)?.into());
        }
    }
    let block = state(&Block::new(&name, properties)?, context)?;
    data.insert("Name".into(), V::String(block.id));
    if !block.properties.is_empty() {
        data.insert(
            "Properties".into(),
            V::Compound(
                block
                    .properties
                    .into_iter()
                    .map(|(k, v)| (k, V::String(v)))
                    .collect(),
            ),
        );
    }
    Ok(())
}
