use super::*;

pub(super) fn convert(value: &mut V, context: &Context) -> Result<()> {
    if let V::String(name) = value {
        *value = V::Compound(Compound::from([("name".into(), V::String(name.clone()))]));
    }
    let profile = map_mut(value)?;
    if let Some(value) = profile.get_mut("id") {
        super::uuids::normalize(value)?;
    }
    if let Some(value) = profile.get_mut("properties") {
        if let V::Compound(properties) = value {
            let mut entries = Vec::new();
            let mut keys = properties.keys().collect::<Vec<_>>();
            keys.sort();
            for name in keys {
                for value in crate::nbt::list(&properties[name])? {
                    entries.push(V::Compound(Compound::from([
                        ("name".into(), V::String(name.clone())),
                        ("value".into(), V::String(crate::nbt::string(value)?.into())),
                    ])));
                }
            }
            *value = V::List(entries);
        }
        for value in crate::nbt::list(value)? {
            let entry = crate::nbt::compound(value)?;
            text(entry, "name")?;
            text(entry, "value")?;
            if let Some(value) = entry.get("signature") {
                crate::nbt::string(value)?;
            }
        }
    }
    for key in ["texture", "cape", "elytra", "model"] {
        if let Some(value) = profile.get(key) {
            if context.source < 4554 || context.target.data_version < 4554 {
                return Err(format!(
                    "profile.{key}: skin overrides require Java 1.21.9 or newer"
                ));
            }
            let value = crate::nbt::string(value)?;
            if key == "model" && !matches!(value, "wide" | "slim") {
                return Err("profile.model: expected wide or slim".into());
            }
        }
    }
    Ok(())
}

pub(super) fn forward(value: V) -> Result<V> {
    if let V::String(name) = value {
        return Ok(V::Compound(Compound::from([(
            "name".into(),
            V::String(name),
        )])));
    }
    let V::Compound(mut profile) = value else {
        return Err("SkullOwner: expected string or compound".into());
    };
    move_field(&mut profile, "Name", "name")?;
    move_field(&mut profile, "Id", "id")?;
    if let Some(value) = profile.remove("Properties") {
        let properties = crate::nbt::compound(&value)?;
        let mut output = Vec::new();
        for (name, value) in properties {
            for value in crate::nbt::list(value)? {
                let mut entry = crate::nbt::compound(value)?.clone();
                move_field(&mut entry, "Value", "value")?;
                move_field(&mut entry, "Signature", "signature")?;
                entry.insert("name".into(), V::String(name.clone()));
                output.push(V::Compound(entry));
            }
        }
        profile.insert("properties".into(), V::List(output));
    }
    Ok(V::Compound(profile))
}

pub(super) fn reverse(value: V) -> Result<V> {
    let mut data = crate::nbt::compound(&value)?.clone();
    move_field(&mut data, "name", "Name")?;
    move_field(&mut data, "id", "Id")?;
    if let Some(value) = data.remove("properties") {
        let mut properties = Compound::new();
        for value in crate::nbt::list(&value)? {
            let mut entry = crate::nbt::compound(value)?.clone();
            let name = entry
                .remove("name")
                .ok_or("profile.properties.name: missing name")?;
            let name = crate::nbt::string(&name)?;
            move_field(&mut entry, "value", "Value")?;
            move_field(&mut entry, "signature", "Signature")?;
            let list = properties
                .entry(name.into())
                .or_insert_with(|| V::List(Vec::new()));
            list_mut(list)?.push(V::Compound(entry));
        }
        insert(&mut data, "Properties", V::Compound(properties))?;
    }
    Ok(V::Compound(data))
}
