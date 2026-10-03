use super::*;

const SIDES: [&str; 4] = ["back", "left", "right", "front"];

pub(super) fn convert(value: &mut V, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    if context.source < 4996 {
        let values = list_mut(value)?;
        if values.len() > 4 {
            return Err("pot decorations: at most four sides can be represented".into());
        }
        for value in values.iter_mut() {
            let source = crate::catalog::namespace(crate::nbt::string(value)?);
            context.source_registry.item(&source)?;
            let target = context.rename("item", &source)?;
            context.target.item(&target)?;
            *value = V::String(target);
        }
        if context.target.data_version >= 4996 {
            let mut sides = Compound::new();
            for (index, side) in SIDES.iter().enumerate() {
                let id = values
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| V::String("minecraft:brick".into()));
                sides.insert(
                    (*side).into(),
                    V::Compound(Compound::from([("id".into(), id)])),
                );
            }
            *value = V::Compound(sides);
        }
        return Ok(());
    }
    let sides = map_mut(value)?;
    for (side, value) in sides.iter_mut() {
        if !SIDES.contains(&side.as_str()) {
            return Err(format!("pot decorations.{side}: unknown side"));
        }
        if let V::String(id) = value {
            *value = V::Compound(Compound::from([("id".into(), V::String(id.clone()))]));
        }
        let stack = map_mut(value)?;
        super::items::convert(stack, context, level + 1)
            .map_err(|e| format!("pot decorations.{side}.{e}"))?;
        if stack.is_empty() {
            return Err(format!(
                "pot decorations.{side}: decoration item must be nonempty"
            ));
        }
    }
    if context.target.data_version < 4996 {
        let mut values = Vec::with_capacity(4);
        for side in SIDES {
            let stack = sides.get(side).ok_or_else(|| {
                format!("pot decorations.{side}: an absent decoration has no older item equivalent")
            })?;
            let stack = crate::nbt::compound(stack)?;
            for (key, value) in stack {
                let representable = match key.as_str() {
                    "id" => true,
                    "count" => crate::nbt::number(value)? == 1,
                    "components" => crate::nbt::compound(value)?.is_empty(),
                    _ => false,
                };
                if !representable {
                    return Err(format!(
                        "pot decorations.{side}.{key}: decorated item data has no older plain-item equivalent"
                    ));
                }
            }
            values.push(crate::nbt::get(stack, "id")?.clone());
        }
        *value = V::List(values);
    }
    Ok(())
}
