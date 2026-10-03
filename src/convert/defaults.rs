use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
struct Source {
    attributes: BTreeMap<String, String>,
    fallback_items: BTreeSet<String>,
}

struct Defaults {
    attributes: BTreeMap<String, Compound>,
    fallback: BTreeSet<String>,
}

fn table() -> Result<&'static Defaults> {
    static TABLE: OnceLock<std::result::Result<Defaults, String>> = OnceLock::new();
    TABLE
        .get_or_init(|| {
            let source: Source =
                serde_json::from_str(include_str!("data/item-attributes-1.20.5.json"))
                    .map_err(|e| e.to_string())?;
            let attributes = source
                .attributes
                .into_iter()
                .map(|(id, snbt)| Ok((id, fastsnbt::from_str(&snbt).map_err(|e| e.to_string())?)))
                .collect::<Result<_>>()?;
            Ok(Defaults {
                attributes,
                fallback: source.fallback_items,
            })
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub(super) fn attributes(id: &str) -> Result<Compound> {
    Ok(table()?
        .attributes
        .get(id)
        .cloned()
        .unwrap_or_else(|| Compound::from([("modifiers".into(), V::List(Vec::new()))])))
}

pub(super) fn fallback(id: &str) -> Result<bool> {
    Ok(table()?.fallback.contains(id))
}

pub(super) fn modifier_name(id: Option<&str>, uuid: &V, kind: &str) -> Result<Option<String>> {
    let mut names = BTreeSet::new();
    for (owner, data) in &table()?.attributes {
        if id.is_some_and(|id| id != owner) {
            continue;
        }
        for value in crate::nbt::list(crate::nbt::get(data, "modifiers")?)? {
            let modifier = crate::nbt::compound(value)?;
            if modifier.get("uuid") == Some(uuid)
                && modifier.get("type") == Some(&V::String(crate::catalog::namespace(kind)))
            {
                names.insert(text(modifier, "name")?);
            }
        }
    }
    if names.len() == 1 {
        Ok(names.into_iter().next())
    } else {
        Ok(None)
    }
}
