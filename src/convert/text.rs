use super::*;

pub(super) fn convert(value: &mut V, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    let json_source = context.source < crate::versions::NBT_TEXT_COMPONENTS;
    let json_target = context.target.data_version < crate::versions::NBT_TEXT_COMPONENTS;
    if json_source {
        let input = crate::nbt::string(value)?;
        if input.len() > 1024 * 1024 {
            return Err("text exceeds 1 MiB".into());
        }
        let json = serde_json::from_str(input).map_err(|e| format!("text JSON: {e}"))?;
        let mut parsed = from_json(json, level + 1)?;
        let original = parsed.clone();
        walk(&mut parsed, context, level + 1)?;
        if !json_target {
            *value = parsed;
        } else if parsed != original {
            *value = V::String(
                serde_json::to_string(&to_json(&parsed, level + 1)?).map_err(|e| e.to_string())?,
            );
        }
    } else {
        walk(value, context, level + 1)?;
        if json_target {
            *value = V::String(
                serde_json::to_string(&to_json(value, level + 1)?).map_err(|e| e.to_string())?,
            );
        }
    }
    Ok(())
}

pub(super) fn lines(value: &mut V, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    let values = list_mut(value)?;
    for (index, value) in values.iter_mut().enumerate() {
        convert(value, context, level + 1).map_err(|e| format!("[{index}].{e}"))?;
    }
    if context.target.data_version >= crate::versions::NBT_TEXT_COMPONENTS {
        mixed_list(values);
    }
    Ok(())
}

pub(super) fn book(value: &mut V, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    if let Some(pages) = map_mut(value)?.get_mut("pages") {
        for (index, page) in list_mut(pages)?.iter_mut().enumerate() {
            if let V::Compound(fields) = page
                && fields.len() == 1
                && let Some(inner) = fields.remove("")
            {
                *page = inner;
            }
            if !matches!(page, V::Compound(fields) if fields.contains_key("raw")) {
                let raw = std::mem::replace(page, V::Compound(Compound::new()));
                *page = V::Compound(Compound::from([("raw".into(), raw)]));
            }
            for key in ["raw", "filtered"] {
                if let Some(value) = map_mut(page)?.get_mut(key) {
                    convert(value, context, level + 1)
                        .map_err(|e| format!("written_book_content.pages[{index}].{key}.{e}"))?;
                }
            }
        }
    }
    Ok(())
}

fn from_json(value: serde_json::Value, level: usize) -> Result<V> {
    depth(level)?;
    Ok(match value {
        serde_json::Value::String(s) => V::String(s),
        serde_json::Value::Bool(b) => V::Byte(i8::from(b)),
        serde_json::Value::Number(n) => {
            if let Some(n) = n.as_i64() {
                i32::try_from(n).map(V::Int).unwrap_or(V::Long(n))
            } else {
                V::Double(n.as_f64().ok_or("invalid text number")?)
            }
        }
        serde_json::Value::Array(a) => V::List(
            a.into_iter()
                .map(|v| from_json(v, level + 1))
                .collect::<Result<_>>()?,
        ),
        serde_json::Value::Object(o) => V::Compound(
            o.into_iter()
                .map(|(k, v)| Ok((k, from_json(v, level + 1)?)))
                .collect::<Result<_>>()?,
        ),
        serde_json::Value::Null => return Err("text JSON:null is not a text component".into()),
    })
}

fn to_json(value: &V, level: usize) -> Result<serde_json::Value> {
    depth(level)?;
    Ok(match value {
        V::String(s) => serde_json::Value::String(s.clone()),
        V::Byte(n) => serde_json::json!(*n),
        V::Short(n) => serde_json::json!(*n),
        V::Int(n) => serde_json::json!(*n),
        V::Long(n) => serde_json::json!(*n),
        V::Float(n) => serde_json::json!(*n),
        V::Double(n) => serde_json::json!(*n),
        V::List(a) => serde_json::Value::Array(
            a.iter()
                .map(|v| to_json(v, level + 1))
                .collect::<Result<_>>()?,
        ),
        V::Compound(o) => {
            if o.len() == 1
                && let Some(value) = o.get("")
            {
                return to_json(value, level + 1);
            }
            serde_json::Value::Object(
                o.iter()
                    .map(|(k, v)| {
                        let value = if matches!(
                            k.as_str(),
                            "bold"
                                | "italic"
                                | "underlined"
                                | "strikethrough"
                                | "obfuscated"
                                | "interpret"
                        ) {
                            if let V::Byte(n) = v {
                                serde_json::Value::Bool(*n != 0)
                            } else {
                                to_json(v, level + 1)?
                            }
                        } else {
                            to_json(v, level + 1)?
                        };
                        Ok((k.clone(), value))
                    })
                    .collect::<Result<_>>()?,
            )
        }
        _ => return Err("text: typed arrays cannot be represented as JSON text".into()),
    })
}

fn walk(value: &mut V, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    match value {
        V::String(_) => {}
        V::List(values) => {
            for value in values.iter_mut() {
                walk(value, context, level + 1)?;
            }
            if context.target.data_version >= crate::versions::NBT_TEXT_COMPONENTS {
                mixed_list(values);
            }
        }
        V::Compound(data) => {
            if data.len() == 1 && data.contains_key("") {
                return walk(data.get_mut("").unwrap(), context, level + 1);
            }
            if context.target.data_version < 4189 && data.contains_key("shadow_color") {
                return Err("text.shadow_color cannot be represented before 1.21.4".into());
            }
            if context.crosses(4786) {
                if context.forward() {
                    if let Some(value) = data.get_mut("selector") {
                        strict_selector(value, context)?;
                    }
                    if data.contains_key("nbt") {
                        if let Some(value) = data.get_mut("entity") {
                            strict_selector(value, context)?;
                        }
                        nbt_path(crate::nbt::string(&data["nbt"])?)?;
                        if let Some(value) = data.get("block") {
                            block_position(crate::nbt::string(value)?)?;
                        }
                        if data.contains_key("plain") {
                            return Err(
                                "text.plain: field is unavailable in the source schema".into()
                            );
                        }
                    }
                } else if data.contains_key("nbt")
                    && let Some(value) = data.remove("plain")
                    && crate::nbt::number(&value)? != 0
                {
                    return Err("text.plain: unstyled NBT pretty-printing cannot be represented before Java 26.1".into());
                }
            }

            if data.contains_key("object")
                && let Some(value) = data.get_mut("fallback")
            {
                if context.source < 4786 || context.target.data_version < 4786 {
                    return Err("text.object.fallback: requires Java 26.1 or newer".into());
                }
                walk(value, context, level + 1)?;
            }
            if let Some(value) = data.get_mut("extra") {
                list_mut(value)?;
                walk(value, context, level + 1)?;
            }
            if let Some(value) = data.get_mut("with") {
                let values = list_mut(value)?;
                for value in values.iter_mut() {
                    if let V::Compound(wrapper) = value
                        && wrapper.len() == 1
                        && let Some(inner) = wrapper.get("")
                    {
                        *value = inner.clone();
                    }
                    if matches!(value, V::String(_) | V::Compound(_) | V::List(_)) {
                        walk(value, context, level + 1)?;
                    } else if let V::Byte(n) = value
                        && context.source < crate::versions::NBT_TEXT_COMPONENTS
                    {
                        *value = V::String((*n != 0).to_string());
                    }
                }
                if context.target.data_version >= crate::versions::NBT_TEXT_COMPONENTS {
                    mixed_list(values);
                }
            }
            if let Some(value) = data.get_mut("separator") {
                walk(value, context, level + 1)?;
            }
            if context.crosses(crate::versions::NBT_TEXT_COMPONENTS) {
                for (old, new) in [("hoverEvent", "hover_event"), ("clickEvent", "click_event")] {
                    if context.forward() {
                        move_field(data, old, new)?;
                    } else {
                        move_field(data, new, old)?;
                    }
                }
            }
            let hover_key = if context.target.data_version >= crate::versions::NBT_TEXT_COMPONENTS {
                "hover_event"
            } else {
                "hoverEvent"
            };
            if let Some(value) = data.get_mut(hover_key) {
                let hover = map_mut(value)?;
                match text(hover, "action")?.as_str() {
                    "show_text" => {
                        let source = if hover.contains_key("contents") {
                            "contents"
                        } else {
                            "value"
                        };
                        let target = if context.target.data_version < 2566
                            || context.target.data_version >= crate::versions::NBT_TEXT_COMPONENTS
                        {
                            "value"
                        } else {
                            "contents"
                        };
                        if source != target {
                            move_field(hover, source, target)?;
                        }
                        if let Some(value) = hover.get_mut(target) {
                            walk(value, context, level + 1)?;
                        }
                    }
                    "show_item" => hover_item(hover, context, level + 1)?,
                    "show_entity" => hover_entity(hover, context, level + 1)?,
                    other => return Err(format!("hover_event.action: unknown action {other}")),
                }
            }
            let click_key = if context.target.data_version >= crate::versions::NBT_TEXT_COMPONENTS {
                "click_event"
            } else {
                "clickEvent"
            };
            if let Some(value) = data.get_mut(click_key) {
                let click = map_mut(value)?;
                let action = text(click, "action")?;
                if matches!(action.as_str(), "custom" | "show_dialog")
                    && (context.source < 4435 || context.target.data_version < 4435)
                {
                    return Err(format!(
                        "click_event.action.{action}: requires Java 1.21.6 or newer"
                    ));
                }
                if matches!(action.as_str(), "run_command" | "suggest_command") {
                    let key = if context.source >= crate::versions::NBT_TEXT_COMPONENTS {
                        "command"
                    } else {
                        "value"
                    };
                    let input = text(click, key)?;
                    if action == "run_command" || input.starts_with('/') {
                        click.insert(
                            key.into(),
                            V::String(commands::convert(&input, context, level + 1)?),
                        );
                    }
                }
                if context.crosses(crate::versions::NBT_TEXT_COMPONENTS) {
                    let modern = match action.as_str() {
                        "open_url" => "url",
                        "open_file" => "path",
                        "change_page" => "page",
                        "copy_to_clipboard" => "value",
                        "run_command" | "suggest_command" => "command",
                        _ => return Err(format!("click_event.action: unknown action {action}")),
                    };
                    if context.forward() {
                        move_field(click, "value", modern)?;
                        if modern == "page"
                            && let Some(value) = click.get_mut("page")
                        {
                            let page = crate::nbt::string(value)?
                                .parse::<i32>()
                                .map_err(|e| format!("click_event.page: {e}"))?;
                            *value = V::Int(page);
                        }
                    } else {
                        if modern == "page"
                            && let Some(value) = click.get_mut("page")
                        {
                            *value = V::String(crate::nbt::number(value)?.to_string());
                        }
                        move_field(click, modern, "value")?;
                    }
                }
            }
            if data.contains_key("object") {
                if context.source < 4554 || context.target.data_version < 4554 {
                    return Err("object text requires Java 1.21.9 or newer".into());
                }
                if data.get("object") == Some(&V::String("player".into()))
                    && let Some(value) = data.get_mut("player")
                {
                    super::profiles::convert(value, context)?;
                }
            }
        }
        _ => return Err("text: expected string, compound or list".into()),
    }
    Ok(())
}

fn strict_selector(value: &mut V, context: &Context) -> Result<()> {
    let input = crate::nbt::string(value)?;
    let end = if input.starts_with('@') {
        if !matches!(
            input.as_bytes().get(1),
            Some(b'a' | b'e' | b'n' | b'p' | b'r' | b's')
        ) {
            return Err("text selector: unknown selector type".into());
        }
        if input[2..].starts_with('[') {
            2 + commands::balanced(&input[2..])?
        } else {
            2
        }
    } else {
        input.find(char::is_whitespace).unwrap_or(input.len())
    };
    if end == 0 {
        return Err("text selector: empty selection".into());
    }
    if end < input.len() {
        context.loss(
            "text.selector",
            "trailing input ignored by the source selector parser was removed",
        );
        *value = V::String(input[..end].into());
    }
    Ok(())
}

fn block_position(input: &str) -> Result<()> {
    let fields = input.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 3 {
        return Err("text.block: expected three coordinates".into());
    }
    let local = fields.iter().filter(|value| value.starts_with('^')).count();
    if local != 0 && local != 3 {
        return Err("text.block: local and world coordinates cannot be mixed".into());
    }
    for field in fields {
        let number = field.strip_prefix(['~', '^']).unwrap_or(field);
        if number.is_empty() && field.starts_with(['~', '^']) {
            continue;
        }
        if !number
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'-' | b'.'))
            || !number.parse::<f64>().is_ok_and(f64::is_finite)
            || (!field.starts_with(['~', '^']) && number.parse::<i32>().is_err())
        {
            return Err("text.block: invalid coordinate".into());
        }
    }
    Ok(())
}

fn nbt_path(input: &str) -> Result<()> {
    let mut rest = input;
    let mut first = true;
    while !rest.is_empty() {
        let mut end = 0;
        if rest.starts_with(['"', '\'']) {
            let quote = rest.as_bytes()[0];
            let mut escaped = false;
            for (index, byte) in rest.bytes().enumerate().skip(1) {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == quote {
                    end = index + 1;
                    break;
                }
            }
            if end == 0 {
                return Err("text.nbt: unterminated quoted path name".into());
            }
        } else if !(first && rest.starts_with('{')) && !rest.starts_with('[') {
            end = rest
                .find(|c: char| {
                    c.is_whitespace() || matches!(c, '.' | '[' | ']' | '{' | '}' | '"' | '\'')
                })
                .unwrap_or(rest.len());
            if end == 0 {
                return Err("text.nbt: invalid path name".into());
            }
        }
        rest = &rest[end..];
        if rest.starts_with('{') {
            let end = commands::balanced(rest)?;
            let value: V =
                crate::nbt::parse_snbt(&rest[..end]).map_err(|e| format!("text.nbt: {e}"))?;
            crate::nbt::compound(&value)?;
            rest = &rest[end..];
        }
        while rest.starts_with('[') {
            let end = commands::balanced(rest)?;
            let index = &rest[1..end - 1];
            if index.starts_with('{') {
                let value: V =
                    crate::nbt::parse_snbt(index).map_err(|e| format!("text.nbt: {e}"))?;
                crate::nbt::compound(&value)?;
            } else if !index.is_empty() && index.parse::<i32>().is_err() {
                return Err("text.nbt: invalid list index".into());
            }
            rest = &rest[end..];
        }
        first = false;
        if rest.is_empty() {
            return Ok(());
        }
        rest = rest
            .strip_prefix('.')
            .ok_or("text.nbt: unexpected trailing path input")?;
        if rest.is_empty() {
            return Err("text.nbt: path ends with a separator".into());
        }
    }
    Err("text.nbt: empty path".into())
}

fn literal(value: &V) -> Result<String> {
    match value {
        V::String(value) => Ok(value.clone()),
        V::Compound(value) if value.len() == 1 => text(value, "text"),
        _ => Err("legacy hover payload must be a literal SNBT string".into()),
    }
}

fn hover_item(hover: &mut Compound, context: &Context, level: usize) -> Result<()> {
    let source_flat = context.source >= crate::versions::NBT_TEXT_COMPONENTS;
    let target_flat = context.target.data_version >= crate::versions::NBT_TEXT_COMPONENTS;
    let native_payload = !source_flat && !hover.contains_key("contents");
    let mut item = if source_flat {
        let mut item = Compound::new();
        for key in ["id", "count", "components"] {
            if let Some(value) = hover.remove(key) {
                item.insert(key.into(), value);
            }
        }
        item
    } else if let Some(value) = hover.remove("contents") {
        match value {
            V::String(id) => Compound::from([("id".into(), V::String(id))]),
            V::Compound(item) => item,
            _ => return Err("show_item.contents: expected item identifier or object".into()),
        }
    } else {
        let value = hover
            .remove("value")
            .ok_or("show_item: missing item payload")?;
        let mut item = crate::nbt::from_snbt(&literal(&value)?)?;
        if context.source >= crate::versions::ITEM_COMPONENTS {
            if item.contains_key("Count") || item.contains_key("tag") {
                return Err("legacy show_item.value needs its original item data version".into());
            }
        } else {
            if let Some(count) = item.remove("Count") {
                item.insert("count".into(), V::Int(crate::nbt::number(&count)?));
            }
            if let Some(value) = item.remove("tag") {
                item.insert(
                    "tag".into(),
                    V::String(crate::nbt::to_snbt(crate::nbt::compound(&value)?)?),
                );
            }
        }
        item
    };
    if context.source < crate::versions::ITEM_COMPONENTS {
        let count = item
            .remove("count")
            .map(|v| crate::nbt::number(&v))
            .transpose()?
            .unwrap_or(1);
        item.insert(
            "Count".into(),
            V::Byte(i8::try_from(count).map_err(|_| "hover item count exceeds legacy range")?),
        );
        if let Some(value) = item.get_mut("tag") {
            *value = V::Compound(crate::nbt::from_snbt(crate::nbt::string(value)?)?);
        }
    } else if !source_flat && !native_payload {
        hover_components(&mut item, true, level + 1)?;
    }
    items::convert(&mut item, context, level + 1)?;
    if item.is_empty() {
        return Err("show_item: empty item has no representable tooltip".into());
    }
    if context.target.data_version < 2566 {
        hover.insert("value".into(), V::String(crate::nbt::to_snbt(&item)?));
        return Ok(());
    }
    if context.target.data_version < crate::versions::ITEM_COMPONENTS {
        if let Some(count) = item.remove("Count") {
            item.insert("count".into(), V::Int(crate::nbt::number(&count)?));
        }
        if let Some(value) = item.get_mut("tag") {
            *value = V::String(crate::nbt::to_snbt(crate::nbt::compound(value)?)?);
        }
    } else if !target_flat {
        hover_components(&mut item, false, level + 1)?;
    }
    if target_flat {
        for (key, value) in item {
            super::insert(hover, &key, value)?;
        }
    } else {
        hover.insert("contents".into(), V::Compound(item));
    }
    Ok(())
}

fn hover_entity(hover: &mut Compound, context: &Context, level: usize) -> Result<()> {
    let source_flat = context.source >= crate::versions::NBT_TEXT_COMPONENTS;
    let target_flat = context.target.data_version >= crate::versions::NBT_TEXT_COMPONENTS;
    let mut entity = if source_flat {
        let mut result = Compound::new();
        for key in ["id", "uuid", "name"] {
            if let Some(value) = hover.remove(key) {
                result.insert(key.into(), value);
            }
        }
        result
    } else if let Some(value) = hover.remove("contents") {
        crate::nbt::compound(&value)?.clone()
    } else {
        let value = hover
            .remove("value")
            .ok_or("show_entity: missing entity payload")?;
        let mut entity = crate::nbt::from_snbt(&literal(&value)?)?;
        if let Some(value) = entity.get_mut("name") {
            *value = from_json(
                serde_json::from_str(crate::nbt::string(value)?)
                    .map_err(|e| format!("hover entity name: {e}"))?,
                level + 1,
            )?;
        }
        entity
    };
    let key = if source_flat { "id" } else { "type" };
    let uuid_key = if source_flat { "uuid" } else { "id" };
    match entity.get(uuid_key) {
        Some(V::String(_)) => {
            super::uuids::string(&mut entity, uuid_key, uuid_key, true)?;
        }
        Some(V::IntArray(value)) if source_flat && value.len() == 4 => {}
        _ => return Err("show_entity: expected a valid entity UUID".into()),
    }
    if !target_flat {
        super::uuids::string(&mut entity, uuid_key, uuid_key, false)?;
    }
    let id = context.rename("entity", &text(&entity, key)?)?;
    context.target.entity_id(&id)?;
    entity.insert(key.into(), V::String(id));
    if let Some(value) = entity.get_mut("name") {
        walk(value, context, level + 1)?;
    }
    if source_flat != target_flat {
        if target_flat {
            move_field(&mut entity, "id", "uuid")?;
            move_field(&mut entity, "type", "id")?;
        } else {
            move_field(&mut entity, "id", "type")?;
            move_field(&mut entity, "uuid", "id")?;
        }
    }
    if context.target.data_version < 2566 {
        if let Some(value) = entity.get_mut("name") {
            *value = V::String(
                serde_json::to_string(&to_json(value, level + 1)?).map_err(|e| e.to_string())?,
            );
        }
        hover.insert("value".into(), V::String(crate::nbt::to_snbt(&entity)?));
    } else if target_flat {
        for (key, value) in entity {
            super::insert(hover, &key, value)?;
        }
    } else {
        hover.insert("contents".into(), V::Compound(entity));
    }
    Ok(())
}

fn hover_components(item: &mut Compound, decode: bool, level: usize) -> Result<()> {
    depth(level)?;
    let Some(value) = item.get_mut("components") else {
        return Ok(());
    };
    let components = map_mut(value)?;
    super::components::normalize(components)?;
    for (key, value) in components {
        match key.as_str() {
            "minecraft:custom_data"
            | "minecraft:entity_data"
            | "minecraft:block_entity_data"
            | "minecraft:bucket_entity_data" => {
                if decode {
                    if let V::String(snbt) = value {
                        *value = V::Compound(crate::nbt::from_snbt(snbt)?);
                    } else {
                        json_custom_data(value, level + 1)?;
                    }
                } else {
                    *value = V::String(crate::nbt::to_snbt(crate::nbt::compound(value)?)?);
                }
                continue;
            }
            "minecraft:bundle_contents" | "minecraft:charged_projectiles" => {
                for item in list_mut(value)? {
                    hover_components(map_mut(item)?, decode, level + 1)?;
                }
            }
            "minecraft:container" => {
                for entry in list_mut(value)? {
                    if let Some(item) = map_mut(entry)?.get_mut("item") {
                        hover_components(map_mut(item)?, decode, level + 1)?;
                    }
                }
            }
            "minecraft:food" => {
                if let Some(item) = map_mut(value)?.get_mut("using_converts_to") {
                    hover_components(map_mut(item)?, decode, level + 1)?;
                }
            }
            "minecraft:use_remainder" => hover_components(map_mut(value)?, decode, level + 1)?,
            _ => {}
        }
        if decode
            && matches!(
                key.as_str(),
                "minecraft:firework_explosion"
                    | "minecraft:fireworks"
                    | "minecraft:attribute_modifiers"
                    | "minecraft:food"
                    | "minecraft:consumable"
                    | "minecraft:custom_model_data"
            )
        {
            json_component_numbers(value, None, level + 1)?;
        } else if !decode {
            json_component_output(value, level + 1)?;
        }
    }
    Ok(())
}

fn json_component_numbers(value: &mut V, field: Option<&str>, level: usize) -> Result<()> {
    depth(level)?;
    match value {
        V::Compound(data) => {
            for (key, value) in data {
                if key != "components" {
                    json_component_numbers(value, Some(key), level + 1)?;
                }
            }
        }
        V::List(values) => {
            if matches!(field, Some("colors" | "fade_colors" | "uuid")) {
                *value = V::IntArray(fastnbt::IntArray::new(
                    values
                        .iter()
                        .map(crate::nbt::number)
                        .collect::<Result<_>>()?,
                ));
            } else {
                for value in values {
                    json_component_numbers(value, field, level + 1)?;
                }
            }
        }
        V::Double(number)
            if matches!(
                field,
                Some("probability" | "saturation" | "eat_seconds" | "consume_seconds" | "floats")
            ) =>
        {
            *value = V::Float(*number as f32);
        }
        _ => {}
    }
    Ok(())
}

fn json_component_output(value: &mut V, level: usize) -> Result<()> {
    depth(level)?;
    match value {
        V::Byte(number) => *value = V::Int(i32::from(*number)),
        V::Short(number) => *value = V::Int(i32::from(*number)),
        V::ByteArray(values) => {
            *value = V::List(values.iter().map(|n| V::Int(i32::from(*n))).collect())
        }
        V::IntArray(values) => *value = V::List(values.iter().copied().map(V::Int).collect()),
        V::LongArray(values) => *value = V::List(values.iter().copied().map(V::Long).collect()),
        V::List(values) => {
            for value in values {
                json_component_output(value, level + 1)?;
            }
        }
        V::Compound(data) => {
            for value in data.values_mut() {
                json_component_output(value, level + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn mixed_list(values: &mut [V]) {
    if values.first().is_some_and(|first| {
        values
            .iter()
            .any(|value| std::mem::discriminant(value) != std::mem::discriminant(first))
    }) {
        for value in values {
            if !matches!(value, V::Compound(_)) {
                *value = V::Compound(Compound::from([("".into(), value.clone())]));
            }
        }
    }
}

fn compact_integer(value: i64) -> V {
    if let Ok(value) = i8::try_from(value) {
        V::Byte(value)
    } else if let Ok(value) = i16::try_from(value) {
        V::Short(value)
    } else if let Ok(value) = i32::try_from(value) {
        V::Int(value)
    } else {
        V::Long(value)
    }
}

fn json_custom_data(value: &mut V, level: usize) -> Result<()> {
    depth(level)?;
    match value {
        V::Int(n) => *value = compact_integer(i64::from(*n)),
        V::Long(n) => *value = compact_integer(*n),
        V::Double(n) => {
            if !n.is_finite() {
                return Err("custom-data JSON number must be finite".into());
            }
            if *n >= -9_223_372_036_854_775_808.0
                && *n < 9_223_372_036_854_775_808.0
                && (*n as i64) as f64 == *n
            {
                *value = compact_integer(*n as i64);
            } else if f64::from(*n as f32) == *n {
                *value = V::Float(*n as f32);
            }
        }
        V::Compound(data) => {
            for value in data.values_mut() {
                json_custom_data(value, level + 1)?;
            }
        }
        V::List(values) => {
            for value in values.iter_mut() {
                json_custom_data(value, level + 1)?;
            }
            mixed_list(values);
            if !values.is_empty() && values.iter().all(|v| matches!(v, V::Byte(_))) {
                *value = V::ByteArray(fastnbt::ByteArray::new(
                    values
                        .iter()
                        .map(|v| {
                            if let V::Byte(n) = v {
                                *n
                            } else {
                                unreachable!()
                            }
                        })
                        .collect(),
                ));
            } else if !values.is_empty() && values.iter().all(|v| matches!(v, V::Int(_))) {
                *value = V::IntArray(fastnbt::IntArray::new(
                    values
                        .iter()
                        .map(|v| {
                            if let V::Int(n) = v {
                                *n
                            } else {
                                unreachable!()
                            }
                        })
                        .collect(),
                ));
            } else if !values.is_empty() && values.iter().all(|v| matches!(v, V::Long(_))) {
                *value = V::LongArray(fastnbt::LongArray::new(
                    values
                        .iter()
                        .map(|v| {
                            if let V::Long(n) = v {
                                *n
                            } else {
                                unreachable!()
                            }
                        })
                        .collect(),
                ));
            }
        }
        _ => {}
    }
    Ok(())
}
