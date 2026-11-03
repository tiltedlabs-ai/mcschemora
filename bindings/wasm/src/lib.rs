#![cfg(target_arch = "wasm32")]

//! Browser catalog loading, block edits, validation, and schematic byte I/O.
//!
//! Catalog operations return promises. Failures reject promises or throw JavaScript Errors.

use mcschemora::{
    catalog, formats,
    model::{Block, Schematic},
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

fn error(value: impl ToString) -> JsValue {
    js_sys::Error::new(&value.to_string()).into()
}
fn value(value: serde_json::Value) -> Result<JsValue, JsValue> {
    js_sys::JSON::parse(&value.to_string())
}

/// JavaScript MinecraftData: shared Minecraft Java catalogs fetched and cached in memory.
#[wasm_bindgen(js_name = MinecraftData)]
pub struct WasmData {
    data: Arc<catalog::MinecraftData>,
}

#[wasm_bindgen(js_class = MinecraftData)]
impl WasmData {
    /// Creates a catalog provider.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<WasmData, JsValue> {
        Ok(Self {
            data: Arc::new(catalog::MinecraftData::new(None, false).map_err(error)?),
        })
    }

    /// Resolves a Java version, loads its catalog, and fulfills with the resolved version.
    pub async fn load(&self, version: &str) -> Result<String, JsValue> {
        Ok(self
            .data
            .load(version)
            .await
            .map_err(error)?
            .version
            .clone())
    }

    /// Fetches metadata and fulfills with supported Java version strings.
    pub async fn versions(&self) -> Result<Vec<String>, JsValue> {
        self.data.initialize().await.map_err(error)?;
        self.data.versions().map_err(error)
    }

    /// Loads and caches a JSON dataset: blocks, items, entities, blockCollisionShapes,
    /// biomes, recipes, recipeDefinitions, or itemTags. A shared tracker stores official
    /// server-JAR recipes with version-specific item IDs. Optional datasets load on demand.
    pub async fn dataset(&self, version: &str, kind: &str) -> Result<JsValue, JsValue> {
        value(self.data.dataset(version, kind).await.map_err(error)?)
    }

    /// Loads a catalog and fulfills with an empty Java Schematic containing a main region.
    pub async fn create(&self, version: &str) -> Result<WasmSchematic, JsValue> {
        self.data.load(version).await.map_err(error)?;
        Ok(WasmSchematic {
            schematic: Schematic::new("java", version, self.data.clone()).map_err(error)?,
        })
    }

    /// JavaScript fromBytes: decodes a Uint8Array and fulfills with a Schematic.
    ///
    /// format selects the codec. options may contain blueprint-only version, origin, and palette;
    /// version must be explicit for blueprints.
    #[wasm_bindgen(js_name = fromBytes)]
    pub async fn from_bytes(
        &self,
        bytes: &[u8],
        format: &str,
        options: JsValue,
    ) -> Result<WasmSchematic, JsValue> {
        let options: formats::ImportOptions = if options.is_null() || options.is_undefined() {
            formats::ImportOptions::default()
        } else {
            serde_json::from_str(
                &js_sys::JSON::stringify(&options)?
                    .as_string()
                    .ok_or_else(|| error("Invalid import options"))?,
            )
            .map_err(error)?
        };
        Ok(WasmSchematic {
            schematic: formats::decode(bytes, format, self.data.clone(), &options)
                .await
                .map_err(error)?,
        })
    }
}

/// JavaScript Schematic: a versioned schematic with named regions.
#[wasm_bindgen(js_name = Schematic)]
pub struct WasmSchematic {
    schematic: Schematic,
}

#[wasm_bindgen(js_class = Schematic)]
impl WasmSchematic {
    /// The schematic Minecraft version string.
    #[wasm_bindgen(getter)]
    pub fn version(&self) -> String {
        self.schematic.version.clone()
    }

    /// Returns region descriptions with name, schematic-global origin, local start, and size.
    pub fn regions(&self) -> Result<JsValue, JsValue> {
        value(self.schematic.regions.iter().map(|(name, r)| serde_json::json!({"name":name,"origin":r.origin,"start":r.bounds.start,"size":r.bounds.size})).collect())
    }

    #[wasm_bindgen(js_name = scheduledTicks)]
    pub fn scheduled_ticks(&self, region: &str) -> Result<JsValue, JsValue> {
        let ticks = self
            .schematic
            .region(region)
            .map_err(error)?
            .scheduled_ticks()
            .map_err(error)?;
        let records = js_sys::Array::new();
        for tick in ticks {
            let record = value(serde_json::to_value(&tick).map_err(error)?)?;
            js_sys::Reflect::set(
                &record,
                &JsValue::from_str("order"),
                &js_sys::BigInt::from(tick.order).into(),
            )?;
            records.push(&record);
        }
        Ok(records.into())
    }

    #[wasm_bindgen(js_name = toGlobal)]
    pub fn to_global(&self, region: &str, x: i32, y: i32, z: i32) -> Result<Vec<i32>, JsValue> {
        self.schematic
            .region(region)
            .map_err(error)?
            .to_global([x, y, z])
            .map(|position| position.to_vec())
            .map_err(error)
    }

    #[wasm_bindgen(js_name = toLocal)]
    pub fn to_local(&self, region: &str, x: i32, y: i32, z: i32) -> Result<Vec<i32>, JsValue> {
        self.schematic
            .region(region)
            .map_err(error)?
            .to_local([x, y, z])
            .map(|position| position.to_vec())
            .map_err(error)
    }

    /// JavaScript getBlock: returns a full state string at local integer coordinates.
    ///
    /// Absent cells read as air; an unknown region throws an Error.
    #[wasm_bindgen(js_name = getBlock)]
    pub fn get_block(&self, region: &str, x: i32, y: i32, z: i32) -> Result<String, JsValue> {
        Ok(self
            .schematic
            .region(region)
            .map_err(error)?
            .get([x, y, z])
            .text())
    }

    /// JavaScript setBlock: validates and writes a state string at local coordinates.
    #[wasm_bindgen(js_name = setBlock)]
    pub fn set_block(
        &mut self,
        region: &str,
        x: i32,
        y: i32,
        z: i32,
        state: &str,
    ) -> Result<(), JsValue> {
        self.schematic
            .set_blocks(region, [([x, y, z], Block::parse(state).map_err(error)?)])
            .map_err(error)
    }

    /// Returns errors, warnings, and unknown arrays without changing the schematic.
    pub fn validate(&self) -> Result<JsValue, JsValue> {
        let report = self.schematic.validate();
        value(
            serde_json::json!({"errors":report.errors,"warnings":report.warnings,"unknown":report.unknown}),
        )
    }

    /// Returns items and fractional raw_materials. Blocks and items without a recipe
    /// expansion count as one primitive unit of themselves.
    /// Water and lava count only source blocks (level=0); flowing and falling fluids are excluded.
    /// Costs are cached by catalog version. Shaped recipes precede other catalog recipes;
    /// the first acyclic expansion to declared primitives wins. Paired structures count once,
    /// orphan halves once, and double slabs/stacked items use their item quantities.
    /// Regions are summed, including overlaps. Inventories, entities, fuel, tool wear,
    /// fluid acquisition, and component-dependent recipes are excluded. No blocks are edited.
    /// Filled-container primitives count gross inputs; returned containers are not credited.
    #[wasm_bindgen(js_name = materialCost)]
    pub async fn material_cost(&self) -> Result<JsValue, JsValue> {
        let report = self.schematic.material_cost().await.map_err(error)?;
        value(serde_json::to_value(report).map_err(error)?)
    }

    /// Applies inline options and an optional bundled profile name or JSON profile object.
    ///
    /// Edition must match; block IDs are checked against the schematic's version.
    /// Non-air replacements retain supported properties, discard unsupported ones, and use
    /// target defaults for new properties. Shared values must be valid. Both blocks require
    /// the same block-entity type. Compatible attached data is preserved. Invalid replacements are
    /// skipped silently. Options contain keep (block IDs), replace (source-to-target object),
    /// and fallback (block ID). Explicit keep wins over replace, then profile rules.
    /// Invalid overrides leave their sources unchanged. Override targets remain canonical.
    /// A fallback that is itself replaced is skipped. Malformed profiles or options throw an Error.
    pub fn normalize(
        &mut self,
        profile: Option<JsValue>,
        options: Option<JsValue>,
    ) -> Result<JsValue, JsValue> {
        let profile = profile
            .map(|profile| {
                let json = match profile.as_string() {
                    Some(name) => name,
                    None => js_sys::JSON::stringify(&profile)?
                        .as_string()
                        .ok_or_else(|| error("Invalid normalization profile"))?,
                };
                mcschemora::normalize::Profile::parse(&json).map_err(error)
            })
            .transpose()?;
        let options = options
            .map(|options| {
                serde_json::from_str::<mcschemora::normalize::Options>(
                    &js_sys::JSON::stringify(&options)?
                        .as_string()
                        .ok_or_else(|| error("Invalid normalization options"))?,
                )
                .map_err(error)
            })
            .transpose()?
            .unwrap_or_default();
        let report = self
            .schematic
            .normalize(profile.as_ref(), &options)
            .map_err(error)?;
        value(serde_json::to_value(report).map_err(error)?)
    }

    pub fn repair(&mut self, rules: Option<Vec<String>>) -> Result<JsValue, JsValue> {
        let report = self.schematic.repair(rules.as_deref()).map_err(error)?;
        let changes: Vec<_> = report
            .changes
            .into_iter()
            .map(|change| {
                serde_json::json!({
                    "region": change.region, "position": change.position,
                    "before": change.before.text(), "after": change.after.text()
                })
            })
            .collect();
        value(
            serde_json::json!({"changed": changes.len(), "changes": changes, "skipped": report.skipped}),
        )
    }

    /// JavaScript importDiagnostics: notices about import assumptions and omissions.
    #[wasm_bindgen(js_name = importDiagnostics, getter)]
    pub fn import_diagnostics(&self) -> Vec<String> {
        self.schematic.import_diagnostics.clone()
    }

    /// JavaScript toBytes: encodes the schematic as a Uint8Array.
    ///
    /// allow_loss accepts reported omissions; flatten merges regions when required.
    /// Blocking errors and unaccepted losses throw an Error.
    #[wasm_bindgen(js_name = toBytes)]
    pub async fn to_bytes(
        &self,
        format: &str,
        allow_loss: bool,
        flatten: bool,
        version: Option<String>,
    ) -> Result<Vec<u8>, JsValue> {
        formats::encode(
            &self.schematic,
            format,
            version.as_deref(),
            allow_loss,
            flatten,
        )
        .await
        .map_err(error)
    }
    #[wasm_bindgen(js_name = checkExport)]
    pub async fn check_export(
        &self,
        format: &str,
        flatten: bool,
        version: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let report = formats::check_export(&self.schematic, format, version.as_deref(), flatten)
            .await
            .map_err(error)?;
        value(serde_json::json!({"errors": report.errors, "losses": report.losses}))
    }
}
