use crate::{Result, catalog::source};
use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

#[derive(Debug)]
pub(crate) struct Cache {
    offline: bool,
    bytes: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl Cache {
    pub fn new(cache_dir: Option<PathBuf>, offline: bool) -> Result<Self> {
        if cache_dir.is_some() {
            return Err("Browser catalog storage does not accept filesystem paths".into());
        }
        Ok(Self {
            offline,
            bytes: Mutex::new(BTreeMap::new()),
        })
    }

    pub async fn read(&self, relative: &str) -> Result<Vec<u8>> {
        let url = source::url(relative)?;
        if let Some(bytes) = self
            .bytes
            .lock()
            .map_err(|_| "Catalog cache lock poisoned")?
            .get(relative)
        {
            return Ok(bytes.clone());
        }
        if self.offline {
            return Err(format!("Offline catalog cache miss: {relative}"));
        }
        let global = js_sys::global();
        let fetch = js_sys::Reflect::get(&global, &JsValue::from_str("fetch"))
            .map_err(js_error)?
            .dyn_into::<js_sys::Function>()
            .map_err(js_error)?;
        let promise = fetch
            .call1(&global, &JsValue::from_str(&url))
            .map_err(js_error)?
            .dyn_into::<js_sys::Promise>()
            .map_err(js_error)?;
        let response = JsFuture::from(promise)
            .await
            .map_err(js_error)?
            .dyn_into::<web_sys::Response>()
            .map_err(js_error)?;
        if !response.ok() {
            return Err(format!("Download {url}: HTTP {}", response.status()));
        }
        if response
            .headers()
            .get("content-length")
            .map_err(js_error)?
            .and_then(|v| v.parse::<u64>().ok())
            .is_some_and(|v| v > source::JSON_LIMIT)
        {
            return Err(format!("Catalog dataset exceeds 32 MiB: {relative}"));
        }
        let bytes = JsFuture::from(response.array_buffer().map_err(js_error)?)
            .await
            .map_err(js_error)?;
        let bytes = js_sys::Uint8Array::new(&bytes).to_vec();
        source::json(relative, &bytes)?;
        self.bytes
            .lock()
            .map_err(|_| "Catalog cache lock poisoned")?
            .insert(relative.into(), bytes.clone());
        Ok(bytes)
    }
}

fn js_error(value: JsValue) -> String {
    format!("Browser catalog fetch failed: {value:?}")
}
