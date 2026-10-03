use super::*;

pub(super) fn predicate(input: &str, context: &Context) -> Result<V> {
    if input.len() > 1024 * 1024 {
        return Err("block predicate exceeds 1 MiB".into());
    }
    let input = input.trim();
    let end = input.find(['[', '{']).unwrap_or(input.len());
    let id = input[..end].trim();
    if id.is_empty() || id.chars().any(char::is_whitespace) {
        return Err("block predicate: invalid identifier".into());
    }
    let mut result = Compound::new();
    result.insert("blocks".into(), V::String(crate::catalog::namespace(id)));
    let mut suffix = input[end..].trim();
    if suffix.starts_with('[') {
        let end = balanced(suffix)?;
        let fields = &suffix[1..end - 1];
        let mut states = Compound::new();
        if !fields.trim().is_empty() {
            for field in fields.split(',') {
                let (key, value) = field
                    .split_once('=')
                    .ok_or("block predicate: expected property=value")?;
                let key = key.trim();
                let value = value.trim();
                if key.is_empty()
                    || value.is_empty()
                    || key.contains(['"', '\''])
                    || value.contains(['"', '\''])
                {
                    return Err("block predicate: invalid state constraint".into());
                }
                if states.insert(key.into(), V::String(value.into())).is_some() {
                    return Err(format!("block predicate: duplicate property {key}"));
                }
            }
        }
        result.insert("state".into(), V::Compound(states));
        suffix = suffix[end..].trim();
    }
    if suffix.starts_with('{') {
        let end = balanced(suffix)?;
        let nbt: V = crate::nbt::parse_snbt(&suffix[..end])
            .map_err(|e| format!("block predicate NBT: {e}"))?;
        crate::nbt::compound(&nbt)?;
        result.insert(
            "nbt".into(),
            V::String(fastsnbt::to_string(&nbt).map_err(|e| e.to_string())?),
        );
        suffix = suffix[end..].trim();
    }
    if !suffix.is_empty() {
        return Err("block predicate: unexpected trailing input".into());
    }
    let mut result = V::Compound(result);
    convert_predicate(&mut result, context)?;
    Ok(result)
}

pub(super) fn balanced(input: &str) -> Result<usize> {
    let mut stack = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == delimiter {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '[' | '{' => {
                stack.push(ch);
                depth(stack.len())?;
            }
            ']' | '}' => {
                let expected = if ch == ']' { '[' } else { '{' };
                if stack.pop() != Some(expected) {
                    return Err("unbalanced Minecraft argument".into());
                }
                if stack.is_empty() {
                    return Ok(index + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    Err("unterminated Minecraft argument".into())
}

pub(super) fn legacy_predicate(value: &V, context: &Context) -> Result<V> {
    let mut value = value.clone();
    convert_predicate(&mut value, context)?;
    encode_predicate(&value)
}

pub(super) fn historical_predicate(input: &str, context: &Context) -> Result<V> {
    encode_predicate(&predicate(input, context)?)
}

fn encode_predicate(value: &V) -> Result<V> {
    let data = crate::nbt::compound(value)?;
    if data
        .keys()
        .any(|k| !matches!(k.as_str(), "blocks" | "state" | "nbt"))
    {
        return Err("block predicate: modern fields cannot be represented".into());
    }
    let mut output = text(data, "blocks")?;
    if let Some(value) = data.get("state") {
        let states = crate::nbt::compound(value)?;
        let fields = states
            .iter()
            .map(|(key, value)| Ok(format!("{key}={}", crate::nbt::string(value)?)))
            .collect::<Result<Vec<_>>>()?;
        output.push('[');
        output.push_str(&fields.join(","));
        output.push(']');
    }
    if let Some(value) = data.get("nbt") {
        let snbt = crate::nbt::string(value)?;
        let parsed: V = crate::nbt::parse_snbt(snbt).map_err(|e| e.to_string())?;
        crate::nbt::compound(&parsed)?;
        output.push_str(snbt);
    }
    Ok(V::String(output))
}

pub(super) fn adventure(data: &mut Compound, context: &Context) -> Result<()> {
    for key in ["minecraft:can_break", "minecraft:can_place_on"] {
        let Some(value) = data.get_mut(key) else {
            continue;
        };
        let values = if context.source < crate::versions::NBT_TEXT_COMPONENTS {
            map_mut(value)?
                .get_mut("predicates")
                .ok_or("adventure predicate list is missing")?
        } else {
            value
        };
        for predicate in list_mut(values)? {
            convert_predicate(predicate, context)?;
        }
    }
    Ok(())
}

fn convert_predicate(value: &mut V, context: &Context) -> Result<()> {
    let data = map_mut(value)?;
    if let Some(value) = data.get("nbt") {
        let parsed;
        let predicate = if let V::String(value) = value {
            parsed = crate::nbt::parse_snbt(value)?;
            crate::nbt::compound(&parsed)?
        } else {
            crate::nbt::compound(value)?
        };
        if !predicate.is_empty() {
            return Err("block predicate.nbt: exact NBT matching requires a predicate-specific owner schema conversion".into());
        }
    }
    let states = data
        .get("state")
        .map(crate::nbt::compound)
        .transpose()?
        .cloned()
        .unwrap_or_default();
    let Some(value) = data.get_mut("blocks") else {
        return if states.is_empty() {
            Ok(())
        } else {
            Err(
                "block predicate: unconstrained block identities require state-matching context"
                    .into(),
            )
        };
    };
    let ids = if let V::List(values) = value {
        values.as_mut_slice()
    } else {
        std::slice::from_mut(value)
    };
    let mut converted_states = None;
    for id in ids {
        let source = crate::nbt::string(id)?;
        if source.starts_with('#') {
            return Err(format!("block predicate: unresolved block tag {source}"));
        }
        let properties = states
            .iter()
            .filter(|(_, value)| !matches!(value, V::Compound(_)))
            .map(|(key, value)| Ok((key.clone(), crate::nbt::string(value)?.into())))
            .collect::<Result<_>>()?;
        let block = blocks::constraint(&Block::new(source, properties)?, context)?;
        let source_properties = context
            .source_registry
            .describe(&crate::catalog::namespace(source))?;
        let target_properties = context.target.describe(&block.id)?;
        let mut fields: Compound = block
            .properties
            .into_iter()
            .map(|(key, value)| (key, V::String(value)))
            .collect();
        for (key, value) in &states {
            if let V::Compound(range) = value {
                if source_properties["properties"].get(key).is_none()
                    || source_properties["properties"].get(key)
                        != target_properties["properties"].get(key)
                {
                    return Err(format!(
                        "block predicate.{key}: range conversion requires equivalent property domains"
                    ));
                }
                for (bound, value) in range {
                    if !matches!(bound.as_str(), "min" | "max") {
                        return Err(format!(
                            "block predicate.{key}: unknown range bound {bound}"
                        ));
                    }
                    crate::nbt::string(value)?;
                }
                fields.insert(key.clone(), value.clone());
            }
        }
        if converted_states
            .as_ref()
            .is_some_and(|previous| *previous != fields)
        {
            return Err(
                "block predicate: referenced blocks need different target state constraints".into(),
            );
        }
        converted_states = Some(fields);
        *id = V::String(block.id);
    }
    let states = converted_states.unwrap_or(states);
    if !states.is_empty() || data.contains_key("state") {
        data.insert("state".into(), V::Compound(states));
    }
    Ok(())
}

fn arguments(input: &str) -> Result<Vec<String>> {
    if input.len() > 1024 * 1024 {
        return Err("command exceeds1MiB".into());
    }
    let mut values = Vec::new();
    let mut current = String::new();
    let mut stack = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    for ch in input.chars() {
        if let Some(delimiter) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == delimiter {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                current.push(ch);
            }
            '[' | '{' => {
                stack.push(ch);
                depth(stack.len())?;
                current.push(ch);
            }
            ']' | '}' => {
                let expected = if ch == ']' { '[' } else { '{' };
                if stack.pop() != Some(expected) {
                    return Err("command: mismatched argument delimiter".into());
                }
                current.push(ch);
            }
            c if c.is_whitespace() && stack.is_empty() => {
                if !current.is_empty() {
                    values.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if quote.is_some() || !stack.is_empty() {
        return Err("command: unterminated quoted or nested argument".into());
    }
    if !current.is_empty() {
        values.push(current);
    }
    Ok(values)
}

fn selector(value: &mut String, context: &Context) -> Result<()> {
    if !value.starts_with('@') {
        return Ok(());
    }
    let head = value
        .as_bytes()
        .get(1)
        .ok_or("selector: missing selector type")?;
    if !matches!(head, b'a' | b'e' | b'p' | b'r' | b's') {
        return Err("selector: unsupported selector type".into());
    }
    if value.len() == 2 {
        return Ok(());
    }
    if !value[2..].starts_with('[') || balanced(&value[2..])? != value.len() - 2 {
        return Err("selector: invalid selector argument".into());
    }
    let mut fields = Vec::new();
    for field in value[3..value.len() - 1].split(',') {
        let (key, data) = field
            .split_once('=')
            .ok_or("selector: expected key=value")?;
        if matches!(key, "nbt" | "predicate" | "advancements" | "scores") {
            return Err(format!(
                "selector.{key}: structured selector conversion is not implemented"
            ));
        }
        if data.contains(['{', '[', '"', '\'']) {
            return Err("selector: structured selector value is not implemented".into());
        }
        if key == "type" {
            if data.starts_with('#') || data.starts_with("!#") {
                return Err("selector.type: unresolved entity tag".into());
            }
            let (negated, id) = data
                .strip_prefix('!')
                .map(|id| ("!", id))
                .unwrap_or(("", data));
            let id = context.rename("entity", id)?;
            context.target.entity_id(&id)?;
            fields.push(format!("type={negated}{id}"));
        } else if matches!(
            key,
            "x" | "y"
                | "z"
                | "dx"
                | "dy"
                | "dz"
                | "distance"
                | "limit"
                | "sort"
                | "name"
                | "team"
                | "tag"
                | "gamemode"
                | "level"
                | "x_rotation"
                | "y_rotation"
        ) {
            fields.push(field.into());
        } else {
            return Err(format!("selector.{key}: unknown selector field"));
        }
    }
    *value = format!("{}[{}]", &value[..2], fields.join(","));
    Ok(())
}

pub(super) fn block_argument(value: &mut String, context: &Context) -> Result<()> {
    let value_source = value.trim();
    let split = value_source.find('{').unwrap_or(value_source.len());
    let state = Block::parse(value_source[..split].trim())?;
    let block = blocks::state(&state, context)?;
    let mut output = block.text();
    let suffix = value_source[split..].trim();
    if !suffix.is_empty() {
        let end = balanced(suffix)?;
        if !suffix[end..].trim().is_empty() {
            return Err("block argument: unexpected trailing input".into());
        }
        let mut entity: Compound =
            crate::nbt::parse_snbt(&suffix[..end]).map_err(|e| e.to_string())?;
        if !entity.is_empty() {
            if !entity.contains_key("id") {
                return Err(
                    "block argument NBT: block-entity ID is required for typed conversion".into(),
                );
            }
            super::block_entities::convert(&mut entity, context, 0)?;
            output.push_str(&fastsnbt::to_string(&entity).map_err(|e| e.to_string())?);
        }
    }
    *value = output;
    Ok(())
}

pub(super) fn convert(input: &str, context: &Context, level: usize) -> Result<String> {
    depth(level)?;
    if input.is_empty() {
        return Ok(String::new());
    }
    let prefix = if input.starts_with('/') { "/" } else { "" };
    let mut args = arguments(input.trim_start_matches('/'))?;
    let command = args
        .first()
        .ok_or("empty command")?
        .trim_start_matches("minecraft:")
        .to_owned();
    match command.as_str() {
        "say" => {
            if args.len() < 2 {
                return Err("say: expected message".into());
            }
        }
        "setblock" => {
            if !(5..=6).contains(&args.len()) {
                return Err("setblock: expected position, block, optional mode".into());
            }
            block_argument(&mut args[4], context)?;
            if args
                .get(5)
                .is_some_and(|mode| !matches!(mode.as_str(), "destroy" | "keep" | "replace"))
            {
                return Err("setblock: invalid placement mode".into());
            }
        }
        "fill" => {
            if !(8..=10).contains(&args.len()) {
                return Err("fill: unsupported argument shape".into());
            }
            block_argument(&mut args[7], context)?;
            if args.get(8).is_some_and(|mode| {
                !matches!(
                    mode.as_str(),
                    "destroy" | "keep" | "replace" | "hollow" | "outline"
                )
            }) {
                return Err("fill: invalid mode".into());
            }
            if args.len() == 10 {
                if args[8] != "replace" {
                    return Err("fill: filter requires replace mode".into());
                }
                block_argument(&mut args[9], context)?;
            }
        }
        "give" => {
            if !(3..=4).contains(&args.len()) {
                return Err("give: expected target, item, optional count".into());
            }
            selector(&mut args[1], context)?;
            if args[2].contains(['{', '[']) {
                return Err("give: versioned item argument conversion is not implemented".into());
            }
            let id = context.rename("item", &args[2])?;
            context.target.item(&id)?;
            args[2] = id;
            if let Some(count) = args.get(3) {
                let count = count.parse::<i32>().map_err(|_| "give: invalid count")?;
                if count < 1 {
                    return Err("give: expected positive count".into());
                }
            }
        }
        "summon" => {
            if !matches!(args.len(), 2 | 5 | 6) {
                return Err("summon: expected entity, optional position and NBT".into());
            }
            args[1] = context.rename("entity", &args[1])?;
            context.target.entity_id(&args[1])?;
            if args.len() == 6 {
                let mut data: Compound =
                    crate::nbt::parse_snbt(&args[5]).map_err(|e| format!("summon NBT: {e}"))?;
                if data.contains_key("id") {
                    return Err("summon: NBT id conflicts with command entity type".into());
                }
                data.insert("id".into(), V::String(args[1].clone()));
                entities::entity(&mut data, context, level + 1)?;
                data.remove("id");
                args[5] = fastsnbt::to_string(&data).map_err(|e| e.to_string())?;
            }
        }
        "tellraw" => {
            if args.len() != 3 {
                return Err("tellraw: expected target and text".into());
            }
            selector(&mut args[1], context)?;
            let mut value = if context.source < crate::versions::NBT_TEXT_COMPONENTS {
                V::String(args[2].clone())
            } else {
                crate::nbt::parse_snbt(&args[2]).map_err(|e| format!("tellraw: {e}"))?
            };
            super::text::convert(&mut value, context, level + 1)?;
            args[2] = if context.target.data_version < crate::versions::NBT_TEXT_COMPONENTS {
                crate::nbt::string(&value)?.into()
            } else {
                fastsnbt::to_string(&value).map_err(|e| e.to_string())?
            };
        }
        "execute" => {
            let run = args
                .iter()
                .position(|s| s == "run")
                .ok_or("execute: command must have an explicit run clause")?;
            let mut index = 1;
            while index < run {
                match args[index].as_str() {
                    "as" | "at" => {
                        if index + 1 >= run {
                            return Err("execute: missing selector".into());
                        }
                        selector(&mut args[index + 1], context)?;
                        index += 2;
                    }
                    "positioned" => {
                        if index + 3 >= run {
                            return Err("execute positioned: missing position".into());
                        }
                        index += 4;
                    }
                    _ => {
                        return Err(format!(
                            "execute.{}: clause conversion is not implemented",
                            args[index]
                        ));
                    }
                }
            }
            let nested = convert(&args[run + 1..].join(" "), context, level + 1)?;
            args.truncate(run + 1);
            args.push(nested);
        }
        _ => {
            return Err(format!(
                "{command}: command grammar conversion is not implemented"
            ));
        }
    }
    Ok(format!("{prefix}{}", args.join(" ")))
}
