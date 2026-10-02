//! Game-rule validation with separate errors, warnings, and unknown results.

mod block_entities;
mod portals;
mod rules;
mod scene;
mod shapes;

pub(crate) use block_entities::block_entity_id;
pub(crate) use shapes::Shapes;

use crate::model::{Block, Position, Schematic, direction};
use scene::{Cell, Scene};
use shapes::Support;
use std::collections::HashSet;

/// Read-only game-rule validation results; no world ticks are simulated.
#[derive(Debug, Default)]
pub struct Report {
    /// Structural violations or invalid block and attached-data states.
    pub errors: Vec<String>,
    /// Unstable states and advisory document notices.
    pub warnings: Vec<String>,
    /// Checks requiring unavailable game data or surrounding world blocks.
    pub unknown: Vec<String>,
}

type Point = [i64; 3];
const UP: Point = [0, 1, 0];
const DOWN: Point = [0, -1, 0];
const SIDES: [(&str, Point); 4] = [
    ("north", [0, 0, -1]),
    ("east", [1, 0, 0]),
    ("south", [0, 0, 1]),
    ("west", [-1, 0, 0]),
];

fn add(p: Point, d: Point) -> Point {
    std::array::from_fn(|i| p[i] + d[i])
}
fn neg(d: Point) -> Point {
    d.map(|n| -n)
}
fn world(origin: Position, p: Position) -> Point {
    std::array::from_fn(|i| i64::from(origin[i]) + i64::from(p[i]))
}
fn name(b: &Block) -> &str {
    b.name.strip_prefix("minecraft:").unwrap_or(&b.name)
}
fn prop<'a>(b: &'a Block, key: &str) -> &'a str {
    b.properties.get(key).map(String::as_str).unwrap_or("")
}
fn facing(b: &Block) -> Point {
    direction(prop(b, "facing"))
        .unwrap_or([0, 0, -1])
        .map(i64::from)
}
fn clockwise(d: Point) -> Point {
    [-d[2], 0, d[0]]
}

#[derive(Clone, Copy)]
enum Severity {
    Error,
    Warning,
    Unknown,
}

struct Check<'a, 'b> {
    scene: &'a Scene<'b>,
    cell: Cell<'b>,
    report: &'a mut Report,
    missing: Vec<&'static str>,
}

impl Check<'_, '_> {
    fn emit(&mut self, rule: &str, message: &str, severity: Severity) {
        let text = format!(
            "{} {:?}: {rule}: {message}",
            self.cell.region, self.cell.local
        );
        match severity {
            Severity::Error => self.report.errors.push(text),
            Severity::Warning => self.report.warnings.push(text),
            Severity::Unknown => self.report.unknown.push(text),
        }
    }
    fn require(&mut self, rule: &'static str, result: Option<bool>, message: &str) {
        match result {
            Some(true) => (),
            Some(false) => self.emit(rule, message, Severity::Error),
            None if !self.missing.contains(&rule) => {
                self.missing.push(rule);
                self.emit(
                    rule,
                    "needs surrounding blocks or support data",
                    Severity::Unknown,
                )
            }
            None => (),
        }
    }
    fn at(&self, d: Point) -> Option<&Block> {
        self.scene.get(add(self.cell.point, d))
    }
    fn needs_support(&mut self, offset: Point, face: Point, kind: Support) {
        let result = self.scene.support(add(self.cell.point, offset), face, kind);
        self.require("support", result, "required supporting face is missing");
    }
}

/// Checks a document without modifying it.
///
/// Java catalogs and surrounding blocks determine which rules can be checked. Other editions
/// and unavailable support data produce unknown results.
pub fn validate(doc: &Schematic) -> Report {
    let mut report = Report {
        warnings: doc
            .notices
            .iter()
            .chain(&doc.import_diagnostics)
            .cloned()
            .collect(),
        ..Report::default()
    };
    if doc.edition != "java" {
        report
            .unknown
            .push("edition: game-rule validation currently requires Java Edition".into());
        return report;
    }
    let registry = match doc.registry() {
        Ok(registry) => registry,
        Err(e) => {
            report.unknown.push(e);
            return report;
        }
    };
    scene::check_region_bounds(doc, &mut report);
    if !report.errors.is_empty() {
        return report;
    }
    if registry.validation_shapes.get().is_none() {
        let shapes = doc.data.collision_shapes(&doc.version).and_then(|value| {
            serde_json::from_value(value).map_err(|e| format!("Invalid collision shapes: {e}"))
        });
        match shapes {
            Ok(shapes) => {
                let _ = registry.validation_shapes.set(shapes);
            }
            Err(e) => report.unknown.push(format!("support.catalog: {e}")),
        }
    }
    let scene = scene::build_scene(doc, registry, registry.validation_shapes.get(), &mut report);
    let mut portals = HashSet::new();
    for &cell in &scene.cells {
        let state = &scene.states[cell.state];
        if !state.valid {
            continue;
        }
        let b = &state.block;
        let mut check = Check {
            scene: &scene,
            cell,
            report: &mut report,
            missing: Vec::new(),
        };
        for rule in &state.rules {
            rule(&mut check, b);
        }
        if name(b) == "moving_piston"
            && !doc.regions[cell.region]
                .block_entities
                .contains_key(&cell.local)
        {
            check.emit(
                "piston.data",
                "moving piston requires its block entity data",
                Severity::Error,
            );
        }
        if matches!(name(b), "nether_portal" | "end_portal") && !portals.contains(&cell.point) {
            portals::check(&mut check, b, &mut portals);
        }
    }
    for (region_name, region) in &doc.regions {
        for (&local, data) in &region.block_entities {
            block_entities::check(
                &scene,
                world(region.origin, local),
                data,
                region_name,
                local,
                doc,
                &mut report,
            );
        }
    }
    report
}
