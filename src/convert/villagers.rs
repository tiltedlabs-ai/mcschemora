use super::*;

fn profession(profession: i32, career: i32) -> Result<&'static str> {
    match (profession, career) {
        (0, 0 | 1) => Ok("farmer"),
        (0, 2) => Ok("fisherman"),
        (0, 3) => Ok("shepherd"),
        (0, 4) => Ok("fletcher"),
        (1, 0 | 1) => Ok("librarian"),
        (1, 2) => Ok("cartographer"),
        (2, _) => Ok("cleric"),
        (3, 0 | 1) => Ok("armorer"),
        (3, 2) => Ok("weaponsmith"),
        (3, 3) => Ok("toolsmith"),
        (4, 0 | 1) => Ok("butcher"),
        (4, 2) => Ok("leatherworker"),
        (5, _) => Ok("nitwit"),
        _ => Err("Profession/Career: unknown legacy profession".into()),
    }
}

pub(super) fn convert(data: &mut Compound, context: &Context) -> Result<()> {
    if context.crosses(4763) {
        if context.forward() {
            if let Some(value) = data.get("VillagerDataFinalized")
                && crate::nbt::number(value)? == 0
            {
                context.loss("VillagerDataFinalized", "previously ignored finalization flag replaced to preserve existing villager data");
            }
            data.insert("VillagerDataFinalized".into(), V::Byte(1));
        } else {
            let finalized = data
                .remove("VillagerDataFinalized")
                .map(|value| crate::nbt::number(&value))
                .transpose()?
                .unwrap_or(0);
            if finalized == 0 {
                return Err("VillagerDataFinalized: deferred villager data generation requires target world and trade registry context".into());
            }
        }
    }
    if text(data, "id")? == "minecraft:villager"
        && context.crosses(1963)
        && context.forward()
        && let Some(value) = data.get_mut("Gossips")
    {
        let entries = list_mut(value)?;
        let mut retained = Vec::with_capacity(entries.len());
        for (index, entry) in entries.drain(..).enumerate() {
            if text(crate::nbt::compound(&entry)?, "Type")? == "golem" {
                context.loss(
                    &format!("Gossips[{index}]"),
                    "obsolete golem gossip has no modern representation",
                );
            } else {
                retained.push(entry);
            }
        }
        *entries = retained;
    }

    if !context.crosses(1918) {
        return experience(data, context);
    }
    if context.forward() {
        let old = data
            .remove("Profession")
            .map(|v| crate::nbt::number(&v))
            .transpose()?
            .unwrap_or(0);
        let career = data
            .remove("Career")
            .map(|v| crate::nbt::number(&v))
            .transpose()?
            .unwrap_or(0);
        let level = data
            .remove("CareerLevel")
            .map(|v| crate::nbt::number(&v))
            .transpose()?
            .unwrap_or(1)
            .max(1);
        let profession = profession(old, career)?;
        super::insert(
            data,
            "VillagerData",
            V::Compound(Compound::from([
                ("type".into(), V::String("minecraft:plains".into())),
                (
                    "profession".into(),
                    V::String(format!("minecraft:{profession}")),
                ),
                ("level".into(), V::Int(level)),
            ])),
        )?;
    } else if let Some(value) = data.remove("VillagerData") {
        let mut villager = crate::nbt::compound(&value)?.clone();
        let kind = villager
            .remove("type")
            .unwrap_or_else(|| V::String("minecraft:plains".into()));
        if crate::nbt::string(&kind)? != "minecraft:plains" {
            return Err(
                "VillagerData.type: biome-specific villager cannot be represented before1.14"
                    .into(),
            );
        }
        let name = villager
            .remove("profession")
            .unwrap_or_else(|| V::String("minecraft:none".into()));
        let name = crate::catalog::namespace(crate::nbt::string(&name)?);
        let (old, career) = match name.as_str() {
            "minecraft:farmer" => (0, 1),
            "minecraft:fisherman" => (0, 2),
            "minecraft:shepherd" => (0, 3),
            "minecraft:fletcher" => (0, 4),
            "minecraft:librarian" => (1, 1),
            "minecraft:cartographer" => (1, 2),
            "minecraft:cleric" => (2, 1),
            "minecraft:armorer" => (3, 1),
            "minecraft:weaponsmith" => (3, 2),
            "minecraft:toolsmith" => (3, 3),
            "minecraft:butcher" => (4, 1),
            "minecraft:leatherworker" => (4, 2),
            "minecraft:nitwit" => (5, 1),
            _ => {
                return Err(format!(
                    "VillagerData.profession: {name} cannot be represented before1.14"
                ));
            }
        };
        super::insert(data, "Profession", V::Int(old))?;
        super::insert(data, "Career", V::Int(career))?;
        let level = villager.remove("level").unwrap_or(V::Int(1));
        super::insert(data, "CareerLevel", level)?;
        if !villager.is_empty() {
            return Err("VillagerData: unknown fields cannot be represented".into());
        }
        if let Some(value) = data.remove("Xp") {
            let xp = crate::nbt::number(&value)?;
            let level = crate::nbt::number(crate::nbt::get(data, "CareerLevel")?)?;
            let minimum = [0, 0, 10, 50, 100, 150]
                .get(level as usize)
                .copied()
                .ok_or("CareerLevel: outside1..5")?;
            if xp != minimum {
                return Err(
                    "Xp: nondefault villager experience cannot be represented before1.14".into(),
                );
            }
        }
        if let Some(value) = data.remove("Gossips")
            && !crate::nbt::list(&value)?.is_empty()
        {
            return Err(
                "Gossips: modern villager reputation cannot be represented before1.14".into(),
            );
        }
        if let Some(value) = data.remove("Brain") {
            let brain = crate::nbt::compound(&value)?;
            let empty = brain.is_empty()
                || (brain.len() == 1
                    && brain
                        .get("memories")
                        .map(crate::nbt::compound)
                        .transpose()?
                        .is_some_and(|memories| memories.is_empty()));
            if !empty {
                return Err(
                    "Brain: modern villager memories cannot be represented before1.14".into(),
                );
            }
        }
    }
    experience(data, context)
}

fn experience(data: &mut Compound, context: &Context) -> Result<()> {
    if context.crosses(1955) && context.forward() && !data.contains_key("Xp") {
        let level = data
            .get("VillagerData")
            .map(crate::nbt::compound)
            .transpose()?
            .and_then(|v| v.get("level"))
            .map(crate::nbt::number)
            .transpose()?
            .unwrap_or(1);
        let xp = [0, 0, 10, 50, 100, 150]
            .get(level as usize)
            .copied()
            .ok_or("VillagerData.level: outside1..5")?;
        data.insert("Xp".into(), V::Int(xp));
    }
    Ok(())
}
