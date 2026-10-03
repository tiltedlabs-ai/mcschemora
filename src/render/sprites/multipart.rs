use super::Crop;
use crate::{model::Block, render::View};

pub(super) struct Choice {
    pub name: String,
    pub covered: &'static [&'static str],
    pub turns: u8,
    pub mirror: bool,
    pub crop: Crop,
    pub view_specific: bool,
}

pub(super) fn resolve(block: &Block, display: &str, view: View) -> Option<Choice> {
    let prop = |key: &str| block.properties.get(key).map(String::as_str);
    let name = display.to_lowercase().replace([' ', '+'], "-");
    if block.id.ends_with("_bed") {
        let part = match prop("part")? {
            part @ ("head" | "foot") => part,
            _ => return None,
        };
        let facing = prop("facing")?;
        let top = matches!(view, View::Top | View::Bottom);
        let side = matches!(facing, "east" | "west");
        let prefix = if top {
            "top-"
        } else if side {
            "side-"
        } else {
            ""
        };
        let turns = if top {
            match facing {
                "south" => 1,
                "west" => 2,
                "north" => 3,
                _ => 0,
            }
        } else {
            0
        };
        Some(Choice {
            name: format!("BlockSprite:{name}-{prefix}{part}"),
            covered: &["part", "facing"],
            turns,
            mirror: !top && facing == "west",
            crop: Crop::Whole,
            view_specific: true,
        })
    } else if block.id.ends_with("_door") {
        let (half, crop) = match prop("half")? {
            "upper" => ("top", Crop::Upper),
            "lower" => ("bottom", Crop::Lower),
            _ => return None,
        };
        Some(Choice {
            name: format!("BlockSprite:{name}-{half}"),
            covered: &["half"],
            turns: 0,
            mirror: false,
            crop,
            view_specific: false,
        })
    } else {
        None
    }
}
