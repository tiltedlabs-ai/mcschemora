use super::*;

pub(super) fn convert(data: &mut Compound, context: &Context) -> Result<()> {
    if let Some(value) = data.get_mut("listener") {
        let listener = map_mut(value)?;
        event(listener, context, "listener")?;
        if let Some(value) = listener.get_mut("selector") {
            event(map_mut(value)?, context, "listener.selector")?;
        }
    }
    Ok(())
}

fn event(data: &mut Compound, context: &Context, path: &str) -> Result<()> {
    if let Some(value) = data.get_mut("event") {
        let event = map_mut(value)?;
        if let Some(value) = event.get_mut("game_event") {
            let mut id = crate::catalog::namespace(crate::nbt::string(value)?);
            if !id.starts_with("minecraft:") {
                return Err(format!(
                    "{path}.event.game_event: unresolved external game event{id}"
                ));
            }
            if context.crosses(3568) {
                if context.forward()
                    && matches!(
                        id.as_str(),
                        "minecraft:entity_roar" | "minecraft:entity_shake"
                    )
                {
                    context.loss(&format!("{path}.event.game_event"),"entity_roar/entity_shake collapse to entity_action and cannot be distinguished after conversion");
                    id = "minecraft:entity_action".into();
                } else if !context.forward() && id == "minecraft:entity_action" {
                    return Err(format!(
                        "{path}.event.game_event: entity_action has multiple legacy identities"
                    ));
                }
            }
            *value = V::String(id);
        }
    }
    Ok(())
}
