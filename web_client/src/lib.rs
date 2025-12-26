use dictlib::{DebugLogger, Stopwatch, compiled_dictionary::{CompiledDictionary, Match, MatchWithHitInfo, Timings}, data_reader::DataReader, rendered_result::RenderedResult};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::wasm_instant::WasmInstant;

macro_rules! log {
    ( $( $t:tt )* ) => {
        web_sys::console::log_1(&format!( $( $t )* ).into());
    }
}

mod wasm_instant;

struct ConsoleLogger
{
}

impl DebugLogger for ConsoleLogger
{
    fn log(&self, logline: &str) {
        web_sys::console::log_1(&logline.to_owned().into());
    }

    fn log_error(&self, logline: &str) {
        web_sys::console::error_1(&logline.to_owned().into());
    }
}

#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
pub struct JyutpingSearch
{
    dict: CompiledDictionary,
}

#[wasm_bindgen]
impl JyutpingSearch {
    #[wasm_bindgen(constructor)]
    pub fn new(compiled_data : Vec<u8>) -> Self {
        console_error_panic_hook::set_once();

        log!("Hello, received {} bytes", compiled_data.len());
        dictlib::set_debug_logger(Box::new(ConsoleLogger{}));
        let mut data_reader = DataReader::new(&compiled_data);
        let dict = CompiledDictionary::deserialize(&mut data_reader);
        Self {
            dict,
        }
    }

    pub fn search(&self, prefix : &str) -> String {
        let stopwatch = Box::new(WasmStopwatch::new());
        let results = self.dict.search(prefix, stopwatch);

        let mut display_results = Vec::new();
        for m in results.matches
        {
            let rendered = RenderedResult::from_match(&m, &self.dict);
            display_results.push(DisplayResult
            {
                match_obj: m,
                rendered_entry: rendered,
                query: prefix.to_string(),
            })
        }

        let dr = DisplaySearchResult {
            results: display_results,
            timings: results.timings,
        };

        serde_json::to_string(&dr).unwrap()
    }
}

#[derive(Serialize)]
struct DisplayResult
{
    pub match_obj: MatchWithHitInfo,
    pub rendered_entry: RenderedResult,
    pub query: String,
}

#[derive(Serialize)]
struct DisplaySearchResult
{
    results: Vec<DisplayResult>,
    timings: Timings,
}

pub struct WasmStopwatch {
    start: WasmInstant,
}

impl WasmStopwatch {
    pub fn new() -> Self {
        WasmStopwatch { start: WasmInstant::now() }
    }
}

impl Stopwatch for WasmStopwatch {
    fn elapsed_ms(&self) -> i32 {
        let now = WasmInstant::now();
        now.duration_since(self.start).as_millis() as i32
    }
}
