use crate::{
    Result,
    model::{Block, Region, ScheduledTick},
    nbt::{self, Tag},
};
use std::collections::BTreeMap;

impl Region {
    pub fn scheduled_ticks(&self) -> Result<Vec<ScheduledTick>> {
        let mut result = Vec::new();
        for (field, fluid) in [("Block", false), ("Fluid", true)] {
            let key = format!("Pending{field}Ticks");
            let Some(value) = self.retained.spatial.get(&key) else {
                continue;
            };
            for (index, value) in nbt::list(value)?.iter().enumerate() {
                let tick = nbt::compound(value)?;
                let order = match tick.get("SubTick") {
                    None => 0,
                    Some(Tag::Long(value)) => *value,
                    Some(value) => i64::from(nbt::number(value)?),
                };
                let priority = tick
                    .get("Priority")
                    .map(nbt::number)
                    .transpose()?
                    .unwrap_or(0);
                if !(-3..=3).contains(&priority) {
                    return Err(format!(
                        "{key}[{index}]: tick priority must be -3 through 3"
                    ));
                }
                result.push(ScheduledTick {
                    position: nbt::xyz(value)?,
                    identifier: Block::new(nbt::string(nbt::get(tick, field)?)?, BTreeMap::new())?
                        .id,
                    fluid,
                    delay: nbt::number(nbt::get(tick, "Time")?)?,
                    priority,
                    order,
                });
            }
        }
        result.sort_by_key(|tick| tick.order);
        Ok(result)
    }
}
