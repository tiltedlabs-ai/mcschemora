#![cfg(target_arch = "wasm32")]

use schemora::{
    catalog, formats,
    model::{Block, Document},
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

fn error(value: impl ToString) -> JsValue {
    js_sys::Error::new(&value.to_string()).into()
}
fn value(value: serde_json::Value) -> Result<JsValue, JsValue> {
    js_sys::JSON::parse(&value.to_string())
}

#[wasm_bindgen(js_name = MinecraftData)]
pub struct WasmData {
    data: Arc<catalog::MinecraftData>,
}

#[wasm_bindgen(js_class = MinecraftData)]
impl WasmData {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<WasmData, JsValue> {
        Ok(Self {
            data: Arc::new(catalog::MinecraftData::new(None, false).map_err(error)?),
        })
    }

    pub async fn load(&self, version: &str) -> Result<String, JsValue> {
        Ok(self
            .data
            .load(version)
            .await
            .map_err(error)?
            .version
            .clone())
    }

    pub async fn versions(&self) -> Result<Vec<String>, JsValue> {
        self.data.initialize().await.map_err(error)?;
        self.data.versions().map_err(error)
    }

    pub async fn dataset(&self, version: &str, kind: &str) -> Result<JsValue, JsValue> {
        self.data.load(version).await.map_err(error)?;
        value(self.data.dataset(version, kind).map_err(error)?)
    }

    pub async fn create(&self, version: &str) -> Result<WasmDocument, JsValue> {
        self.data.load(version).await.map_err(error)?;
        Ok(WasmDocument {
            document: Document::new("java", version, self.data.clone()).map_err(error)?,
        })
    }

    #[wasm_bindgen(js_name = fromBytes)]
    pub async fn from_bytes(
        &self,
        bytes: &[u8],
        format: &str,
        options: JsValue,
    ) -> Result<WasmDocument, JsValue> {
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
        Ok(WasmDocument {
            document: formats::decode(bytes, format, self.data.clone(), &options)
                .await
                .map_err(error)?,
        })
    }
}

#[wasm_bindgen(js_name = Schematic)]
pub struct WasmDocument {
    document: Document,
}

#[wasm_bindgen(js_class = Schematic)]
impl WasmDocument {
    #[wasm_bindgen(getter)]
    pub fn version(&self) -> String {
        self.document.version.clone()
    }

    pub fn regions(&self) -> Result<JsValue, JsValue> {
        value(self.document.regions.iter().map(|(name, r)| serde_json::json!({"name":name,"origin":r.origin,"start":r.bounds.start,"size":r.bounds.size})).collect())
    }

    #[wasm_bindgen(js_name = getBlock)]
    pub fn get_block(&self, region: &str, x: i32, y: i32, z: i32) -> Result<String, JsValue> {
        Ok(self
            .document
            .region(region)
            .map_err(error)?
            .get([x, y, z])
            .text())
    }

    #[wasm_bindgen(js_name = setBlock)]
    pub fn set_block(
        &mut self,
        region: &str,
        x: i32,
        y: i32,
        z: i32,
        state: &str,
    ) -> Result<(), JsValue> {
        self.document
            .set_blocks(region, [([x, y, z], Block::parse(state).map_err(error)?)])
            .map_err(error)
    }

    pub fn validate(&self) -> Result<JsValue, JsValue> {
        let report = self.document.validate();
        value(
            serde_json::json!({"errors":report.errors,"warnings":report.warnings,"unknown":report.unknown}),
        )
    }

    #[wasm_bindgen(js_name = importDiagnostics, getter)]
    pub fn import_diagnostics(&self) -> Vec<String> {
        self.document.import_diagnostics.clone()
    }

    #[wasm_bindgen(js_name = toBytes)]
    pub fn to_bytes(
        &self,
        format: &str,
        allow_loss: bool,
        flatten: bool,
    ) -> Result<Vec<u8>, JsValue> {
        formats::encode(&self.document, format, allow_loss, flatten).map_err(error)
    }
}
