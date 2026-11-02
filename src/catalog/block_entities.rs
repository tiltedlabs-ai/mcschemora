pub(crate) fn block_entity_id(id: &str) -> Option<&str> {
    let n = id.strip_prefix("minecraft:").unwrap_or(id);
    Some(match n {
        "moving_piston" => "piston",
        "soul_campfire" => "campfire",
        "chain_command_block" | "repeating_command_block" => "command_block",
        n if n.ends_with("_hanging_sign") => "hanging_sign",
        n if n.ends_with("_sign") || n == "sign" => "sign",
        n if n.ends_with("_bed") => "bed",
        n if n.ends_with("_banner") => "banner",
        n if n.ends_with("shulker_box") => "shulker_box",
        n if n.ends_with("_shelf") => "shelf",
        n if n.ends_with("copper_chest") => "chest",
        "skeleton_skull"
        | "skeleton_wall_skull"
        | "wither_skeleton_skull"
        | "wither_skeleton_wall_skull"
        | "player_head"
        | "player_wall_head"
        | "zombie_head"
        | "zombie_wall_head"
        | "creeper_head"
        | "creeper_wall_head"
        | "dragon_head"
        | "dragon_wall_head"
        | "piglin_head"
        | "piglin_wall_head" => "skull",
        "chest"
        | "trapped_chest"
        | "furnace"
        | "blast_furnace"
        | "smoker"
        | "hopper"
        | "dispenser"
        | "dropper"
        | "barrel"
        | "beacon"
        | "spawner"
        | "lectern"
        | "brewing_stand"
        | "crafter"
        | "campfire"
        | "command_block"
        | "comparator"
        | "daylight_detector"
        | "enchanting_table"
        | "end_portal"
        | "end_gateway"
        | "ender_chest"
        | "jigsaw"
        | "jukebox"
        | "structure_block"
        | "conduit"
        | "bell"
        | "beehive"
        | "sculk_sensor"
        | "calibrated_sculk_sensor"
        | "sculk_catalyst"
        | "sculk_shrieker"
        | "chiseled_bookshelf"
        | "decorated_pot"
        | "trial_spawner"
        | "creaking_heart"
        | "copper_golem_statue"
        | "vault" => n,
        "bee_nest" => "beehive",
        "suspicious_sand" | "suspicious_gravel" => "brushable_block",
        _ => return None,
    })
}

pub(crate) fn inventory_size(id: &str) -> Option<i32> {
    match id.strip_prefix("minecraft:").unwrap_or(id) {
        "chest" | "trapped_chest" | "barrel" => Some(27),
        "hopper" | "brewing_stand" => Some(5),
        "furnace" | "blast_furnace" | "smoker" => Some(3),
        "dispenser" | "dropper" | "crafter" => Some(9),
        "campfire" | "soul_campfire" => Some(4),
        "chiseled_bookshelf" => Some(6),
        n if n.ends_with("shulker_box") => Some(27),
        "shelf" => Some(3),
        _ => None,
    }
}
