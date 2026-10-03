use super::entities::{entity, item_field, item_list, patterns, reject_commands};
use super::*;

pub(super) fn removed(data: &Compound, context: &Context) -> Result<bool> {
    if crate::catalog::namespace(&text(data, "id")?) != "minecraft:bed" {
        return Ok(false);
    }
    if context.source >= 4903 {
        return Err("bed: block-entity schema is unavailable in Java26.2+".into());
    }
    if context.target.data_version < 4903 {
        return Ok(false);
    }
    if let Some(key) = data
        .keys()
        .find(|key| !matches!(key.as_str(), "id" | "x" | "y" | "z" | "keepPacked"))
    {
        return Err(format!(
            "{key}: bed block-entity payload cannot be represented after its removal in Java26.2"
        ));
    }
    Ok(true)
}

pub(super) fn convert(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    if removed(data, context)? {
        return Err(
            "bed: removed block-entity record must be omitted by its owning container".into(),
        );
    }
    depth(level)?;
    let id = context.rename("block_entity", &text(data, "id")?)?;
    data.insert("id".into(), V::String(id.clone()));
    let id = id
        .strip_prefix("minecraft:")
        .ok_or("modded block-entity conversion is not implemented")?;
    super::entity_changes::block_entity(data, id, context)?;
    if matches!(id, "furnace" | "blast_furnace" | "smoker")
        && let Some(value) = data.get_mut("RecipesUsed")
    {
        let recipes = map_mut(value)?;
        let mut converted = Compound::new();
        for (id, count) in std::mem::take(recipes) {
            insert(&mut converted, &context.rename("recipe", &id)?, count)?;
        }
        *recipes = converted;
    }
    if id == "beacon" && context.source >= 3568 {
        for key in ["primary_effect", "secondary_effect"] {
            if let Some(value) = data.get(key) {
                super::effects::registered(crate::nbt::string(value)?, context)?;
            }
        }
    }
    if id == "skull"
        && context.source >= crate::versions::ITEM_COMPONENTS
        && let Some(value) = data.get_mut("profile")
    {
        super::profiles::convert(value, context)?;
    }
    if matches!(
        id,
        "sculk_sensor" | "calibrated_sculk_sensor" | "sculk_shrieker"
    ) {
        super::game_events::convert(data, context)?;
    }
    if id == "creaking_heart" {
        if context.source < 4189 || context.target.data_version < 4189 {
            return Err("creaking_heart: payload requires stable Java1.21.4+".into());
        }
        if let Some(value) = data.get("creaking")
            && !matches!(value, V::IntArray(values) if values.len() == 4)
        {
            return Err("creaking: expected four-integer entity UUID".into());
        }
    }
    let minimum = match id {
        "barrel" | "blast_furnace" | "smoker" | "lectern" | "campfire" | "jigsaw" | "bell" => 1952,
        "beehive" => 2225,
        "sculk_sensor" => 2724,
        "sculk_shrieker" | "sculk_catalyst" => 3105,
        "chiseled_bookshelf"
        | "hanging_sign"
        | "decorated_pot"
        | "calibrated_sculk_sensor"
        | "brushable_block"
        | "suspicious_sand" => 3463,
        "crafter" | "trial_spawner" | "vault" => 3953,
        "creaking_heart" => 4189,
        "shelf" | "copper_golem_statue" => 4554,
        _ => 1519,
    };
    if context.source < minimum || context.target.data_version < minimum {
        return Err(format!(
            "{id}: block-entity schema is unavailable in the source or target stable release"
        ));
    }
    historical(data, id, context)?;
    if inventory_owner(&format!("minecraft:{id}"))
        && let Some(value) = data.get_mut("LootTable")
    {
        *value = V::String(context.rename("loot_table", crate::nbt::string(value)?)?);
    }
    if matches!(id, "trial_spawner" | "vault" | "crafter") {
        if context.source < 3953 || context.target.data_version < 3953 {
            return Err(format!(
                "{id}: experimental payloads before stable Java1.21 require feature context"
            ));
        }
        modern_children(data, id, context, level)?;
    }

    if matches!(
        id,
        "chest"
            | "trapped_chest"
            | "barrel"
            | "hopper"
            | "dispenser"
            | "dropper"
            | "shulker_box"
            | "furnace"
            | "blast_furnace"
            | "smoker"
            | "brewing_stand"
    ) {
        super::modern::lock_fields(data, context, level + 1)?;
    }
    if id == "banner"
        && let Some(value) = data.get_mut("components")
    {
        super::modern::banner(map_mut(value)?, context)?;
    }

    if id == "jukebox" {
        super::modern::jukebox(data, context)?;
    }
    if id == "decorated_pot" {
        if context.crosses(3448) {
            if context.forward() {
                move_field(data, "shards", "sherds")?;
            } else {
                move_field(data, "sherds", "shards")?;
            }
        }
        for key in ["shards", "sherds"] {
            if let Some(value) = data.get_mut(key) {
                super::pottery::convert(value, context, level + 1)?;
            }
        }
    }
    if id == "mob_spawner" {
        super::spawners::convert(data, context)?;
    }
    if id == "skull"
        && context.crosses(2514)
        && let Some(V::Compound(owner)) = data.get_mut("SkullOwner")
    {
        super::uuids::string(owner, "Id", "Id", context.forward())?;
    }
    if id == "conduit" && context.crosses(2514) {
        super::uuids::compound(data, "target_uuid", "Target", context.forward())?;
    }
    if matches!(id, "sign" | "hanging_sign") {
        super::signs::operator_features(data, context)?;
        super::signs::convert(data, context)?;
        super::signs::reconcile(data, context)?;
        if context.target.data_version < 2724 {
            if let Some(value) = data.remove("GlowingText")
                && crate::nbt::number(&value)? != 0
            {
                return Err(
                    "GlowingText: glowing signs cannot be represented before Java1.17".into(),
                );
            }
            for index in 1..=4 {
                let key = format!("FilteredText{index}");
                if let Some(value) = data.remove(&key)
                    && data.get(&format!("Text{index}")) != Some(&value)
                {
                    return Err(format!(
                        "{key}: distinct filtered text cannot be represented before Java1.17"
                    ));
                }
            }
        }
    }
    if id == "beacon" && context.crosses(3568) {
        for (old, new) in [
            ("Primary", "primary_effect"),
            ("Secondary", "secondary_effect"),
        ] {
            super::effects::identifier(data, old, new, context)?;
        }
    }
    if !matches!(
        id,
        "chest"
            | "trapped_chest"
            | "barrel"
            | "hopper"
            | "dispenser"
            | "dropper"
            | "shulker_box"
            | "furnace"
            | "blast_furnace"
            | "smoker"
            | "brewing_stand"
            | "campfire"
            | "chiseled_bookshelf"
            | "shelf"
            | "copper_golem_statue"
            | "brushable_block"
            | "suspicious_sand"
            | "decorated_pot"
            | "trial_spawner"
            | "vault"
            | "crafter"
            | "creaking_heart"
            | "jukebox"
            | "lectern"
            | "mob_spawner"
            | "beehive"
            | "banner"
            | "skull"
            | "sign"
            | "hanging_sign"
            | "end_gateway"
            | "beacon"
            | "conduit"
            | "bed"
            | "enchanting_table"
            | "ender_chest"
            | "end_portal"
            | "piston"
            | "structure_block"
            | "jigsaw"
            | "command_block"
            | "bell"
            | "sculk_sensor"
            | "sculk_shrieker"
            | "sculk_catalyst"
            | "calibrated_sculk_sensor"
            | "comparator"
            | "daylight_detector"
    ) {
        return Err(format!("{id}: unsupported block-entity schema"));
    }
    if context.legacy() {
        return legacy(data, context, level);
    }
    if !context.components() {
        return children(data, context, level);
    }
    typed_items(data, context, level)?;
    if id == "beehive" {
        position(data, "FlowerPos", "flower_pos")?;
    }
    if id == "end_gateway" {
        position(data, "ExitPortal", "exit_portal")?;
    }
    if id == "beehive"
        && let Some(value) = data.remove("Bees")
    {
        let mut value = value;
        for (index, bee) in list_mut(&mut value)?.iter_mut().enumerate() {
            let bee = map_mut(bee)?;
            move_field(bee, "EntityData", "entity_data")?;
            move_field(bee, "TicksInHive", "ticks_in_hive")?;
            move_field(bee, "MinOccupationTicks", "min_ticks_in_hive")?;
            if let Some(value) = bee.get_mut("entity_data") {
                entity(map_mut(value)?, context, level + 1)
                    .map_err(|e| format!("bees[{index}].entity_data.{e}"))?;
            }
        }
        data.insert("bees".into(), value);
    }
    if id == "banner"
        && let Some(value) = data.remove("Patterns")
    {
        data.insert("patterns".into(), patterns(value)?);
    }
    if id == "skull" {
        let owner = data.remove("SkullOwner");
        let extra = data.remove("ExtraType");
        if let Some(value) = owner.or(extra) {
            data.insert("profile".into(), profiles::forward(value)?);
        }
    }
    if let Some(value) = data.get_mut("CustomName") {
        super::text::convert(value, context, level + 1)?;
    }
    if id == "banner"
        && data
            .get("CustomName")
            .map(|v| items::standard_name("minecraft:white_banner", v))
            .transpose()?
            .unwrap_or(false)
    {
        let name = data.remove("CustomName").unwrap();
        let mut components = take_map(data, "components")?;
        components.insert("minecraft:item_name".into(), name);
        components.insert(
            "minecraft:hide_additional_tooltip".into(),
            V::Compound(Compound::new()),
        );
        data.insert("components".into(), V::Compound(components));
    }
    sign_text(data, context, level)?;
    Ok(())
}

fn typed_items(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    reject_commands(data, context)?;
    if inventory_owner(&text(data, "id")?) {
        item_list(data, "Items", context, level + 1, false)?;
    }
    for key in ["RecordItem", "Book", "item"] {
        let id = text(data, "id")?;
        if matches!(
            (id.as_str(), key),
            ("minecraft:jukebox", "RecordItem")
                | ("minecraft:lectern", "Book")
                | (
                    "minecraft:decorated_pot"
                        | "minecraft:brushable_block"
                        | "minecraft:suspicious_sand",
                    "item"
                )
        ) {
            item_field(data, key, context, level + 1)?;
        }
    }
    spawns(data, context, level)?;
    if text(data, "id")? == "minecraft:piston"
        && let Some(value) = data.get_mut("blockState")
    {
        blocks::nbt(map_mut(value)?, context)?;
    }
    Ok(())
}

fn sign_text(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    for side in ["front_text", "back_text"] {
        if matches!(
            text(data, "id")?.as_str(),
            "minecraft:sign" | "minecraft:hanging_sign"
        ) && let Some(value) = data.get_mut(side)
        {
            let data = map_mut(value)?;
            for key in ["messages", "filtered_messages"] {
                if let Some(value) = data.get_mut(key) {
                    for value in list_mut(value)? {
                        super::text::convert(value, context, level + 1)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn children(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    typed_items(data, context, level)?;
    if let Some(value) = data.get_mut("CustomName") {
        super::text::convert(value, context, level + 1)?;
    }
    sign_text(data, context, level)?;
    for key in ["Bees", "bees"] {
        if text(data, "id")? == "minecraft:beehive"
            && let Some(value) = data.get_mut(key)
        {
            let field = if key == "Bees" {
                "EntityData"
            } else {
                "entity_data"
            };
            for (index, value) in list_mut(value)?.iter_mut().enumerate() {
                if let Some(value) = map_mut(value)?.get_mut(field) {
                    entity(map_mut(value)?, context, level + 1)
                        .map_err(|e| format!("{key}[{index}].{field}.{e}"))?;
                }
            }
        }
    }
    Ok(())
}

fn legacy(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    children(data, context, level)?;
    let id = text(data, "id")?;
    if id == "minecraft:beehive" {
        super::entities::legacy_position(data, "flower_pos", "FlowerPos")?;
    }
    if id == "minecraft:end_gateway" {
        super::entities::legacy_position(data, "exit_portal", "ExitPortal")?;
    }
    if id == "minecraft:beehive"
        && let Some(mut value) = data.remove("bees")
    {
        for value in list_mut(&mut value)? {
            let bee = map_mut(value)?;
            for (new, old) in [
                ("entity_data", "EntityData"),
                ("ticks_in_hive", "TicksInHive"),
                ("min_ticks_in_hive", "MinOccupationTicks"),
            ] {
                move_field(bee, new, old)?;
            }
        }
        super::insert(data, "Bees", value)?;
    }
    if id == "minecraft:banner"
        && let Some(value) = data.remove("patterns")
    {
        super::insert(
            data,
            "Patterns",
            super::legacy_items::legacy_patterns(value)?,
        )?;
    }
    if id == "minecraft:skull"
        && let Some(value) = data.remove("profile")
    {
        super::insert(data, "SkullOwner", profiles::reverse(value)?)?;
    }
    if let Some(mut components) = data.remove("components") {
        let map = map_mut(&mut components)?;
        if let Some(value) = map.remove("minecraft:item_name") {
            if !items::standard_name("minecraft:white_banner", &value)? {
                return Err(
                    "components.item_name: nonstandard banner name cannot be represented".into(),
                );
            }
            super::insert(data, "CustomName", value)?;
        }
        map.remove("minecraft:hide_additional_tooltip");
        if !map.is_empty() {
            return Err("block entity components cannot be represented by legacy NBT".into());
        }
    }
    Ok(())
}

fn position(data: &mut Compound, from: &str, to: &str) -> Result<()> {
    if let Some(value) = data.remove(from) {
        let value = super::entities::position_value(crate::nbt::compound(&value)?)?;
        super::insert(data, to, value)?;
    }
    Ok(())
}

pub(super) fn spawns(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    if !matches!(
        text(data, "id")?.as_str(),
        "minecraft:mob_spawner" | "minecraft:spawner_minecart"
    ) {
        return Ok(());
    }
    if let Some(value) = data.get_mut("SpawnData") {
        let spawn = map_mut(value)?;
        if spawn.contains_key("id") {
            entity(spawn, context, level + 1)?;
        } else if let Some(value) = spawn.get_mut("entity") {
            let entity_data = map_mut(value)?;
            if !entity_data.is_empty() {
                entity(entity_data, context, level + 1)?;
            }
        } else if !spawn.is_empty() {
            return Err("SpawnData: cannot infer nested entity schema".into());
        }
    }
    if let Some(value) = data.get_mut("SpawnPotentials") {
        for (index, value) in list_mut(value)?.iter_mut().enumerate() {
            let entry = map_mut(value)?;
            if let Some(value) = entry.get_mut("Entity") {
                entity(map_mut(value)?, context, level + 1)
                    .map_err(|e| format!("SpawnPotentials[{index}].Entity.{e}"))?;
            } else if let Some(value) = entry.get_mut("data")
                && let Some(value) = map_mut(value)?.get_mut("entity")
            {
                entity(map_mut(value)?, context, level + 1)
                    .map_err(|e| format!("SpawnPotentials[{index}].data.entity.{e}"))?;
            }
        }
    }
    Ok(())
}

fn historical(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    if matches!(id, "furnace" | "blast_furnace" | "smoker") && context.crosses(4181) {
        if context.forward() {
            if let Some(value) = data.get("BurnTime") {
                super::insert(data, "lit_total_time", value.clone())?;
            }
        } else if let Some(value) = data.remove("lit_total_time") {
            let total = crate::nbt::number(&value)?;
            if total != 0 {
                context.loss("lit_total_time", "older furnace loading resets the total burn indicator; remaining fuel and cooking progress are retained");
            }
        }
        for (old, new) in [
            ("CookTime", "cooking_time_spent"),
            ("CookTimeTotal", "cooking_total_time"),
            ("BurnTime", "lit_time_remaining"),
        ] {
            let (from, to) = if context.forward() {
                (old, new)
            } else {
                (new, old)
            };
            if let Some(value) = data.remove(from) {
                let number = crate::nbt::number(&value)?;
                let number = i16::try_from(number)
                    .map_err(|_| format!("{from}: value is outside the short codec range"))?;
                super::insert(data, to, V::Short(number))?;
            }
        }
    }

    if matches!(id, "furnace" | "blast_furnace" | "smoker") && context.crosses(2501) {
        if context.forward() {
            let size = data
                .remove("RecipesUsedSize")
                .map(|v| crate::nbt::number(&v))
                .transpose()?
                .unwrap_or(0);
            if !(0..=65536).contains(&size) {
                return Err("RecipesUsedSize: invalid recipe count".into());
            }
            let mut recipes = Compound::new();
            for index in 0..size {
                let key = format!("RecipeLocation{index}");
                let recipe = data
                    .remove(&key)
                    .ok_or_else(|| format!("{key}: missing recipe"))?;
                let recipe = crate::catalog::namespace(crate::nbt::string(&recipe)?);
                let key = format!("RecipeAmount{index}");
                let amount = data
                    .remove(&key)
                    .ok_or_else(|| format!("{key}: missing amount"))?;
                let amount = crate::nbt::number(&amount)?;
                if amount < 0 {
                    return Err(format!("{key}: negative recipe use count"));
                }
                super::insert(&mut recipes, &recipe, V::Int(amount))?;
            }
            super::insert(data, "RecipesUsed", V::Compound(recipes))?;
        } else if let Some(value) = data.remove("RecipesUsed") {
            let recipes = crate::nbt::compound(&value)?;
            let mut entries: Vec<_> = recipes.iter().collect();
            entries.sort_by_key(|(key, _)| *key);
            super::insert(
                data,
                "RecipesUsedSize",
                V::Int(
                    entries
                        .len()
                        .try_into()
                        .map_err(|_| "RecipesUsed: too many recipes")?,
                ),
            )?;
            for (index, (recipe, amount)) in entries.into_iter().enumerate() {
                let amount = crate::nbt::number(amount)?;
                if amount < 0 {
                    return Err(format!("RecipesUsed.{recipe}: negative recipe use count"));
                }
                super::insert(
                    data,
                    &format!("RecipeLocation{index}"),
                    V::String(recipe.clone()),
                )?;
                super::insert(data, &format!("RecipeAmount{index}"), V::Int(amount))?;
            }
        }
    }
    if id == "jigsaw"
        && let Some(value) = data.get_mut("final_state")
    {
        let mut state = crate::nbt::string(value)?.to_owned();
        super::commands::block_argument(&mut state, context)?;
        *value = V::String(state);
    }
    if id == "jigsaw" && context.crosses(2518) {
        if context.forward() {
            move_field(data, "target_pool", "pool")?;
            if let Some(value) = data.remove("attachement_type") {
                super::insert(data, "name", value.clone())?;
                super::insert(data, "target", value)?;
            }
        } else {
            if data.contains_key("joint") {
                return Err(
                    "jigsaw.joint: inverse requires source block orientation context".into(),
                );
            }
            move_field(data, "pool", "target_pool")?;
            match (data.remove("name"), data.remove("target")) {
                (None, None) => {}
                (Some(name), Some(target)) if name == target => {
                    super::insert(data, "attachement_type", name)?
                }
                _ => return Err(
                    "jigsaw.name/target: distinct connector names cannot be represented before1.16"
                        .into(),
                ),
            }
        }
    }
    Ok(())
}

fn inventory_owner(id: &str) -> bool {
    matches!(
        id,
        "minecraft:chest"
            | "minecraft:trapped_chest"
            | "minecraft:barrel"
            | "minecraft:hopper"
            | "minecraft:dispenser"
            | "minecraft:dropper"
            | "minecraft:shulker_box"
            | "minecraft:furnace"
            | "minecraft:blast_furnace"
            | "minecraft:smoker"
            | "minecraft:brewing_stand"
            | "minecraft:campfire"
            | "minecraft:chiseled_bookshelf"
            | "minecraft:shelf"
            | "minecraft:crafter"
    )
}

fn modern_children(data: &mut Compound, id: &str, context: &Context, level: usize) -> Result<()> {
    if id == "trial_spawner" {
        for field in ["normal_config", "ominous_config"] {
            if let Some(value) = data.get_mut(field) {
                if let V::String(reference) = value {
                    let config = trial_config(reference)?;
                    if context.source < 4067 {
                        return Err(format!(
                            "{field}: registry configuration references require Java1.21.2+"
                        ));
                    }
                    if context.target.data_version < 4067 {
                        context.loss(field, "built-in configuration reference expanded to its pinned vanilla definition; registry reference identity is unavailable in the older schema");
                        *value = V::Compound(config);
                    } else {
                        continue;
                    }
                }
                let config = map_mut(value)?;
                if let Some(value) = config.get_mut("spawn_potentials") {
                    for (index, value) in list_mut(value)?.iter_mut().enumerate() {
                        let entry = map_mut(value)?;
                        if let Some(weight) = entry.get("weight")
                            && crate::nbt::number(weight)? < 0
                        {
                            return Err(format!(
                                "{field}.spawn_potentials[{index}].weight: negative weight"
                            ));
                        }
                        if let Some(value) = entry.get_mut("data") {
                            context
                                .scoped(&format!("{field}.spawn_potentials[{index}].data"), || {
                                    spawn_entity(map_mut(value)?, context, level + 1)
                                })
                                .map_err(|e| {
                                    format!("{field}.spawn_potentials[{index}].data.{e}")
                                })?;
                        }
                    }
                }
            }
        }
        if let Some(value) = data.get_mut("spawn_data") {
            context
                .scoped("spawn_data", || {
                    spawn_entity(map_mut(value)?, context, level + 1)
                })
                .map_err(|e| format!("spawn_data.{e}"))?;
        }
    }
    if id == "vault" {
        for (field, item) in [("config", "key_item"), ("shared_data", "display_item")] {
            if let Some(value) = data.get_mut(field) {
                context
                    .scoped(field, || {
                        item_field(map_mut(value)?, item, context, level + 1)
                    })
                    .map_err(|e| format!("{field}.{e}"))?;
            }
        }
        if let Some(value) = data.get_mut("server_data") {
            context
                .scoped("server_data", || {
                    item_list(map_mut(value)?, "items_to_eject", context, level + 1, false)
                })
                .map_err(|e| format!("server_data.{e}"))?;
        }
    }
    Ok(())
}

fn spawn_entity(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    if let Some(value) = data.get_mut("entity") {
        context.scoped("entity", || entity(map_mut(value)?, context, level + 1))?;
    }
    Ok(())
}

fn trial_config(reference: &str) -> Result<Compound> {
    #[derive(Deserialize)]
    struct Source {
        configs: std::collections::BTreeMap<String, String>,
    }
    static CONFIGS: OnceLock<
        std::result::Result<std::collections::BTreeMap<String, Compound>, String>,
    > = OnceLock::new();
    let configs = CONFIGS
        .get_or_init(|| {
            let source: Source =
                serde_json::from_str(include_str!("data/trial-configs-1.21.3.json"))
                    .map_err(|e| e.to_string())?;
            source
                .configs
                .into_iter()
                .map(|(id, snbt)| Ok((id, fastsnbt::from_str(&snbt).map_err(|e| e.to_string())?)))
                .collect::<Result<_>>()
        })
        .as_ref()
        .map_err(Clone::clone)?;
    configs
        .get(&crate::catalog::namespace(reference))
        .cloned()
        .ok_or_else(|| format!("unresolved external trial-spawner configuration {reference}"))
}
