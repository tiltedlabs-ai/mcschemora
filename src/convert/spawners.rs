use super::*;

pub(super) fn convert(data: &mut Compound, context: &Context) -> Result<()> {
    if !context.crosses(2831) {
        return Ok(());
    }
    if let Some(value) = data.remove("SpawnData") {
        let value = if context.forward() {
            V::Compound(Compound::from([("entity".into(), value)]))
        } else {
            let mut data = crate::nbt::compound(&value)?.clone();
            let entity = data
                .remove("entity")
                .ok_or("SpawnData.entity: missing entity")?;
            if !data.is_empty() {
                return Err(
                    "SpawnData: custom spawn rules cannot be represented before1.18".into(),
                );
            }
            entity
        };
        data.insert("SpawnData".into(), value);
    }
    if let Some(value) = data.get_mut("SpawnPotentials") {
        for (index, value) in list_mut(value)?.iter_mut().enumerate() {
            let entry = map_mut(value)?;
            if context.forward() {
                move_field(entry, "Weight", "weight")?;
                if let Some(value) = entry.remove("Entity") {
                    super::insert(
                        entry,
                        "data",
                        V::Compound(Compound::from([("entity".into(), value)])),
                    )?;
                }
            } else {
                move_field(entry, "weight", "Weight")?;
                let mut data = take_map(entry, "data")?;
                let entity = data.remove("entity").ok_or_else(|| {
                    format!("SpawnPotentials[{index}].data.entity: missing entity")
                })?;
                if !data.is_empty() {
                    return Err(format!(
                        "SpawnPotentials[{index}].data: custom spawn rules cannot be represented before1.18"
                    ));
                }
                super::insert(entry, "Entity", entity)?;
            }
        }
    }
    Ok(())
}
