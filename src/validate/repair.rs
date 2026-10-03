use super::{Check, Report, name, prepare, rules, scene};
use crate::{
    Result,
    model::{Block, Position, Schematic},
};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug)]
pub struct RepairChange {
    pub region: String,
    pub position: Position,
    pub before: Block,
    pub after: Block,
}

#[derive(Debug, Default)]
pub struct RepairReport {
    pub changes: Vec<RepairChange>,
    pub skipped: Vec<String>,
}

fn repair_rule(block: &Block) -> Option<(&'static str, rules::Rule)> {
    match name(block) {
        "redstone_wire" => Some(("redstone", rules::check_redstone)),
        n if n.ends_with("_stairs") => Some(("stairs", rules::check_stairs)),
        n if n.ends_with("_fence") => Some(("fences", rules::check_connections)),
        n if n == "iron_bars" || n.ends_with("glass_pane") => {
            Some(("panes", rules::check_connections))
        }
        n if n.ends_with("_wall") => Some(("walls", rules::check_connections)),
        _ => None,
    }
}

pub fn repair(schematic: &mut Schematic, selected: Option<&[String]>) -> Result<RepairReport> {
    if let Some(selected) = selected {
        for rule in selected {
            if !["redstone", "stairs", "fences", "panes", "walls"].contains(&rule.as_str()) {
                return Err(format!("Unknown repair rule {rule:?}"));
            }
        }
        if selected.is_empty() {
            return Ok(RepairReport::default());
        }
    }
    if schematic
        .regions
        .values()
        .any(|region| region.blocks.keys().any(|p| !region.bounds.contains(*p)))
    {
        return Err("Repair requires blocks inside their region bounds".into());
    }
    let mut diagnostics = Report::default();
    let Some(mut scene) = prepare(schematic, &mut diagnostics) else {
        if !diagnostics.errors.is_empty() {
            return Err(diagnostics.errors.join("\n"));
        }
        return Ok(RepairReport {
            skipped: diagnostics.unknown,
            ..RepairReport::default()
        });
    };
    let registry = schematic.registry()?;
    let shapes = registry.validation_shapes.get();
    let mut cells: Vec<_> = scene
        .cells
        .iter()
        .enumerate()
        .filter_map(|(i, cell)| {
            repair_rule(&scene.states[cell.state].block)
                .filter(|(name, _)| {
                    selected.is_none_or(|selected| selected.iter().any(|v| v == name))
                })
                .map(|(_, rule)| (i, rule))
        })
        .collect();
    cells.sort_by_key(|&(i, _)| {
        let cell = scene.cells[i];
        (
            !name(&scene.states[cell.state].block).ends_with("_stairs"),
            -cell.point[1],
            cell.point[0],
            cell.point[2],
        )
    });
    let mut states: HashMap<_, _> = scene
        .states
        .iter()
        .enumerate()
        .map(|(i, state)| (state.block.clone(), i))
        .collect();
    let mut edits = BTreeMap::new();
    let invalid = diagnostics.errors;
    let mut skipped = Vec::new();
    let mut settled = false;
    for _ in 0..=cells.len() {
        let mut changed = false;
        skipped.clone_from(&invalid);
        for &(index, rule) in &cells {
            let cell = scene.cells[index];
            let block = &scene.states[cell.state].block;
            let mut diagnostics = Report::default();
            let mut check = Check {
                scene: &scene,
                cell,
                report: &mut diagnostics,
                missing: Vec::new(),
                repaired: Some(block.clone()),
            };
            rule(&mut check, block);
            let after = check.repaired.unwrap();
            if !diagnostics.unknown.is_empty() {
                skipped.extend(diagnostics.unknown);
                continue;
            }
            if &after == block {
                continue;
            }
            let after = registry.resolve(&after)?;
            let state = *states.entry(after.clone()).or_insert_with(|| {
                let index = scene.states.len();
                scene
                    .states
                    .push(scene::State::new(after.clone(), true, shapes, registry));
                index
            });
            scene.replace(index, state);
            edits.insert((cell.region.to_owned(), cell.local), after);
            changed = true;
        }
        if !changed {
            settled = true;
            break;
        }
    }
    if !settled {
        return Err("Connection repairs did not converge; schematic was not changed".into());
    }
    drop(scene);
    let mut report = RepairReport {
        changes: Vec::with_capacity(edits.len()),
        skipped,
    };
    for ((region, position), after) in edits {
        let source = schematic.regions.get_mut(&region).unwrap();
        let before = source.get(position);
        if before == after {
            continue;
        }
        source.blocks.set(position, &after);
        report.changes.push(RepairChange {
            region,
            position,
            before,
            after,
        });
    }
    Ok(report)
}
