use super::*;

pub(super) fn convert(data: &mut Compound, context: &Context) -> Result<()> {
    if !context.crosses(3439) {
        return Ok(());
    }
    if context.forward() {
        let mut front = Compound::new();
        let mut messages = Vec::new();
        let mut filtered = Vec::new();
        let mut any_filtered = false;
        for index in 1..=4 {
            let mut text = data
                .remove(&format!("Text{index}"))
                .unwrap_or_else(|| V::String("{\"text\":\"\"}".into()));
            items::text_component(&mut text)?;
            let filtered_text = data.remove(&format!("FilteredText{index}"));
            any_filtered |= filtered_text.is_some();
            filtered.push(filtered_text.unwrap_or_else(|| text.clone()));
            messages.push(text);
        }
        front.insert("messages".into(), V::List(messages));
        front.insert("_filtered_correct".into(), V::Byte(1));
        if any_filtered {
            front.insert("filtered_messages".into(), V::List(filtered));
        }
        front.insert(
            "color".into(),
            data.remove("Color")
                .unwrap_or_else(|| V::String("black".into())),
        );
        front.insert(
            "has_glowing_text".into(),
            data.remove("GlowingText").unwrap_or(V::Byte(0)),
        );
        super::insert(data, "front_text", V::Compound(front))?;
        let back = Compound::from([
            (
                "messages".into(),
                V::List(vec![V::String("{\"text\":\"\"}".into()); 4]),
            ),
            ("color".into(), V::String("black".into())),
            ("has_glowing_text".into(), V::Byte(0)),
        ]);
        super::insert(data, "back_text", V::Compound(back))?;
        super::insert(data, "is_waxed", V::Byte(0))?;
    } else {
        if let Some(value) = data.remove("is_waxed")
            && crate::nbt::number(&value)? != 0
        {
            return Err("is_waxed: waxed signs cannot be represented before1.20".into());
        }
        if let Some(value) = data.remove("back_text") {
            let mut back = crate::nbt::compound(&value)?.clone();
            for key in ["messages", "filtered_messages"] {
                if let Some(value) = back.remove(key) {
                    let messages = crate::nbt::list(&value)?;
                    if messages.len() != 4 {
                        return Err(format!("back_text.{key}: expected four lines"));
                    }
                    for value in messages {
                        let value: serde_json::Value =
                            serde_json::from_str(crate::nbt::string(value)?)
                                .map_err(|e| e.to_string())?;
                        if value != serde_json::json!({"text":""}) && value != serde_json::json!("")
                        {
                            return Err(format!(
                                "back_text.{key}: nonempty back text cannot be represented before1.20"
                            ));
                        }
                    }
                }
            }
            if let Some(value) = back.remove("color")
                && crate::nbt::string(&value)? != "black"
            {
                return Err("back_text.color: nondefault back color cannot be represented".into());
            }
            if let Some(value) = back.remove("has_glowing_text")
                && crate::nbt::number(&value)? != 0
            {
                return Err("back_text.has_glowing_text: back glow cannot be represented".into());
            }
            if !back.is_empty() {
                return Err("back_text: unknown fields cannot be represented".into());
            }
        }
        let mut front = take_map(data, "front_text")?;
        front.remove("_filtered_correct");
        for (key, prefix) in [("messages", "Text"), ("filtered_messages", "FilteredText")] {
            if let Some(value) = front.remove(key) {
                let messages = crate::nbt::list(&value)?;
                if messages.len() != 4 {
                    return Err(format!("front_text.{key}: expected four lines"));
                }
                for (index, value) in messages.iter().enumerate() {
                    super::insert(data, &format!("{prefix}{}", index + 1), value.clone())?;
                }
            }
        }
        for (new, old) in [("color", "Color"), ("has_glowing_text", "GlowingText")] {
            if let Some(value) = front.remove(new) {
                super::insert(data, old, value)?;
            }
        }
        if !front.is_empty() {
            return Err("front_text: unknown fields cannot be represented".into());
        }
    }
    Ok(())
}

pub(super) fn reconcile(data: &mut Compound, context: &Context) -> Result<()> {
    if !context.crosses(3564) {
        return Ok(());
    }
    if !context.forward() {
        if let Some(value) = data.get_mut("front_text") {
            map_mut(value)?.insert("_filtered_correct".into(), V::Byte(1));
        }
        return Ok(());
    }
    for side in ["front_text", "back_text"] {
        if let Some(value) = data.get_mut(side) {
            let text = map_mut(value)?;
            if let Some(value) = text.remove("_filtered_correct")
                && crate::nbt::number(&value)? != 0
            {
                continue;
            }
            let messages = text
                .get("messages")
                .map(crate::nbt::list)
                .transpose()?
                .cloned()
                .unwrap_or_default();
            if let Some(value) = text.get_mut("filtered_messages") {
                let filtered = list_mut(value)?;
                for (index, value) in filtered.iter_mut().enumerate() {
                    let json: serde_json::Value = serde_json::from_str(crate::nbt::string(value)?)
                        .map_err(|e| e.to_string())?;
                    if json == serde_json::json!({"text":""}) {
                        *value = messages
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| V::String("{\"text\":\"\"}".into()));
                    }
                }
                if filtered
                    .iter()
                    .all(|value| matches!(value,V::String(s) if s=="{\"text\":\"\"}"))
                {
                    text.remove("filtered_messages");
                }
            }
        }
    }
    for index in 0..=4 {
        data.remove(&format!("Text{index}"));
        data.remove(&format!("FilteredText{index}"));
    }
    data.remove("Color");
    data.remove("GlowingText");
    Ok(())
}

fn needs_operator(value: &V) -> bool {
    match value {
        V::Compound(data) => {
            if ["selector", "nbt", "score", "click_event", "clickEvent"]
                .iter()
                .any(|key| data.contains_key(*key))
            {
                return true;
            }
            ["extra", "with", ""]
                .iter()
                .filter_map(|key| data.get(*key))
                .any(needs_operator)
        }
        V::List(values) => values.iter().any(needs_operator),
        _ => false,
    }
}

pub(super) fn operator_features(data: &mut Compound, context: &Context) -> Result<()> {
    if !context.crosses(5002) {
        return Ok(());
    }
    if context.forward() {
        if let Some(value) = data.get("allow_op_features")
            && crate::nbt::number(value)? == 0
        {
            context.loss(
                "allow_op_features",
                "previously ignored flag replaced to preserve existing sign operator behavior",
            );
        }
        data.insert("allow_op_features".into(), V::Byte(1));
    } else {
        let enabled = data
            .remove("allow_op_features")
            .map(|value| crate::nbt::number(&value))
            .transpose()?
            .unwrap_or(0)
            != 0;
        if !enabled {
            for side in ["front_text", "back_text"] {
                if let Some(value) = data.get(side) {
                    let side = crate::nbt::compound(value)?;
                    for key in ["messages", "filtered_messages"] {
                        if side.get(key).is_some_and(needs_operator) {
                            return Err(format!(
                                "{key}: disabled sign operator features would become active in the older target"
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
