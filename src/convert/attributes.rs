use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
struct Mappings {
    uuids: BTreeMap<String, String>,
    names: BTreeMap<String, String>,
    attributes: Vec<String>,
}

fn mappings() -> Result<&'static Mappings> {
    static MAPPINGS: OnceLock<std::result::Result<Mappings, String>> = OnceLock::new();
    MAPPINGS
        .get_or_init(|| {
            serde_json::from_str(include_str!("data/attributes.json")).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn registered(id: &str, version: i32) -> Result<()> {
    #[derive(Deserialize)]
    struct Registry {
        data_version: i32,
        ids: BTreeSet<String>,
    }
    static REGISTRIES: OnceLock<std::result::Result<Vec<Registry>, String>> = OnceLock::new();
    let registries = REGISTRIES
        .get_or_init(|| {
            serde_json::from_str(include_str!("data/attribute-ids.json")).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    if let Some(registry) = registries
        .iter()
        .rev()
        .find(|registry| registry.data_version <= version)
        && !registry.ids.contains(id)
    {
        return Err(format!(
            "attribute {id} is unavailable in Java data version {version}"
        ));
    }
    Ok(())
}

fn attribute_id(value: &mut V, context: &Context) -> Result<()> {
    let id = crate::catalog::namespace(crate::nbt::string(value)?);
    registered(&id, context.source)?;
    if context.crosses(4055) {
        let mapping = mappings()?;
        if context.forward() {
            if let Some(old) = mapping.attributes.iter().find(|old| **old == id) {
                let (_, suffix) = old.split_once('.').ok_or("invalid attribute mapping")?;
                *value = V::String(format!("minecraft:{suffix}"));
            } else {
                return Err(format!(
                    "attribute {id} is outside the audited source registry"
                ));
            }
        } else {
            let old = mapping.attributes.iter().find(|old| {
                old.split_once('.')
                    .is_some_and(|(_, suffix)| id == format!("minecraft:{suffix}"))
            });
            *value = V::String(
                old.ok_or_else(|| {
                    format!("attribute {id} cannot be represented before Java 1.21.2")
                })?
                .clone(),
            );
        }
    }
    registered(
        &crate::catalog::namespace(crate::nbt::string(value)?),
        context.target.data_version,
    )
}

fn modifier(data: &mut Compound, context: &Context, owner: Option<&str>, kind: &str) -> Result<()> {
    let mapping = mappings()?;
    if context.forward() {
        let uuid = super::uuids::format(data.get("uuid").ok_or("modifier.uuid is missing")?)?;
        let name = data
            .get("name")
            .map(crate::nbt::string)
            .transpose()?
            .unwrap_or("");
        let id = mapping
            .uuids
            .get(&uuid)
            .or_else(|| mapping.names.get(name))
            .cloned()
            .unwrap_or_else(|| format!("minecraft:{uuid}"));
        let standard_name = super::defaults::modifier_name(owner, &data["uuid"], kind)?;
        if !name.is_empty() && standard_name.as_deref() != Some(name) {
            context.loss(
                "name",
                "legacy attribute modifier names are absent from the target schema",
            );
        }
        if mapping.names.contains_key(name) && !mapping.uuids.contains_key(&uuid) {
            context.loss("uuid", "named built-in modifier replaces its original UUID");
        }
        if mapping.uuids.values().filter(|v| **v == id).count() > 1 {
            context.loss(
                "uuid",
                "multiple legacy modifier UUIDs map to the same identifier",
            );
        }
        super::insert(data, "id", V::String(id))?;
        data.remove("uuid");
        data.remove("name");
    } else {
        let id = crate::catalog::namespace(&text(data, "id")?);
        let candidates: Vec<_> = mapping
            .uuids
            .iter()
            .filter(|(_, modern)| **modern == id)
            .map(|(uuid, _)| uuid.as_str())
            .collect();
        let uuid = match candidates.as_slice() {
            [uuid] => super::uuids::parse(uuid)?,
            [] => super::uuids::parse(
                id.strip_prefix("minecraft:")
                    .ok_or_else(|| format!("modifier ID {id} has no equivalent legacy UUID"))?,
            )?,
            _ => {
                return Err(format!(
                    "modifier ID {id} has multiple possible legacy UUIDs"
                ));
            }
        };
        let name = super::defaults::modifier_name(owner, &uuid, kind)?.unwrap_or(id);
        super::insert(data, "uuid", uuid)?;
        super::insert(data, "name", V::String(name))?;
        data.remove("id");
    }
    Ok(())
}

pub(super) fn item(value: &mut V, context: &Context, id: &str, level: usize) -> Result<()> {
    depth(level)?;
    let value = match value {
        V::Compound(data) => data
            .get_mut("modifiers")
            .ok_or("attribute_modifiers.modifiers is missing")?,
        value => value,
    };
    let mut identities = BTreeSet::new();
    for (index, value) in list_mut(value)?.iter_mut().enumerate() {
        context.scoped(&format!("modifiers[{index}]"), || {
            let data = map_mut(value)?;
            attribute_id(
                data.get_mut("type").ok_or("modifier.type is missing")?,
                context,
            )?;
            if let Some(value) = data.get_mut("display") {
                if context.source < 4435 {
                    return Err("attribute_modifiers.display: field is unavailable before 1.21.6".into());
                }
                let display = map_mut(value)?;
                let kind = text(display, "type")?;
                if context.target.data_version < 4435 {
                    if kind != "default" {
                        return Err("attribute_modifiers.display: per-modifier display cannot be represented before 1.21.6".into());
                    }
                    data.remove("display");
                } else {
                    match kind.as_str() {
                        "default" | "hidden" => {},
                        "override" => super::text::convert(
                            display.get_mut("value").ok_or("attribute_modifiers.display.value is missing")?,
                            context, level + 1,
                        )?,
                        _ => return Err(format!("attribute_modifiers.display: unknown type {kind}")),
                    }
                }
            }
            if context.crosses(3945) {
                let kind = text(data, "type")?;
                modifier(data, context, Some(id), &kind)?;
            }
            if context.target.data_version >= 3945 {
                let identity = (text(data, "type")?, text(data, "id")?);
                if !identities.insert(identity) {
                    return Err(
                        "duplicate modifier ID for one attribute cannot be preserved".into(),
                    );
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}

pub(super) fn entity(data: &mut Compound, context: &Context) -> Result<()> {
    if context.crosses(3945) && context.forward() {
        move_field(data, "Attributes", "attributes")?;
    }
    let key = if context.source >= 3945 || context.target.data_version >= 3945 {
        "attributes"
    } else {
        return Ok(());
    };
    if let Some(value) = data.get_mut(key) {
        for (index, value) in list_mut(value)?.iter_mut().enumerate() {
            context.scoped(&format!("attributes[{index}]"), || {
                let attribute = map_mut(value)?;
                if context.crosses(3945) && context.forward() {
                    for (old, new) in [("Name", "id"), ("Base", "base"), ("Modifiers", "modifiers")] {
                        move_field(attribute, old, new)?;
                    }
                }
                attribute_id(attribute.get_mut("id").ok_or("attribute.id is missing")?, context)?;
                if context.crosses(3945) {
                    let kind = text(attribute, "id")?;
                    if let Some(value) = attribute.get_mut("modifiers") {
                        let mut identities = BTreeSet::new();
                        for (index, value) in list_mut(value)?.iter_mut().enumerate() {
                            context.scoped(&format!("modifiers[{index}]"), || {
                                let data = map_mut(value)?;
                                if context.forward() {
                                    for (old, new) in [("Name", "name"), ("UUID", "uuid"), ("Amount", "amount")] {
                                        move_field(data, old, new)?;
                                    }
                                    if let Some(operation) = data.remove("Operation") {
                                        let name = match crate::nbt::number(&operation)? {
                                            0 => "add_value",
                                            1 => "add_multiplied_base",
                                            2 => "add_multiplied_total",
                                            _ => return Err("invalid modifier Operation".into()),
                                        };
                                        super::insert(data, "operation", V::String(name.into()))?;
                                    }
                                }
                                modifier(data, context, None, &kind)?;
                                if context.forward() {
                                    if !identities.insert(text(data, "id")?) {
                                        return Err("duplicate modifier ID for one attribute cannot be preserved".into());
                                    }
                                } else {
                                    for (old, new) in [("name", "Name"), ("uuid", "UUID"), ("amount", "Amount")] {
                                        move_field(data, old, new)?;
                                    }
                                    if let Some(operation) = data.remove("operation") {
                                        let number = match crate::nbt::string(&operation)? {
                                            "add_value" => 0,
                                            "add_multiplied_base" => 1,
                                            "add_multiplied_total" => 2,
                                            _ => return Err("invalid modifier operation".into()),
                                        };
                                        super::insert(data, "Operation", V::Int(number))?;
                                    }
                                }
                                Ok(())
                            })?;
                        }
                    }
                    if !context.forward() {
                        for (old, new) in [("id", "Name"), ("base", "Base"), ("modifiers", "Modifiers")] {
                            move_field(attribute, old, new)?;
                        }
                    }
                }
                Ok(())
            })?;
        }
    }
    if context.crosses(3945) && !context.forward() {
        move_field(data, "attributes", "Attributes")?;
    }
    Ok(())
}
