use dictlib::{DebugLogger, compiled_dictionary::{CompiledDictionary, DisplayDictionaryEntry, Match, MatchType}, data_reader::DataReader};
use serde::Serialize;
use wasm_bindgen::prelude::*;

macro_rules! log {
    ( $( $t:tt )* ) => {
        web_sys::console::log_1(&format!( $( $t )* ).into());
    }
}

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
        let results = self.dict.search(prefix);

        let mut display_results = Vec::new();
        for m in results
        {
            let display_entry = self.dict.get_display_entry(m.entry_id);
            display_results.push(DisplayResult
            {
                match_obj: m,
                display_entry,
                query: prefix.to_string(),
            })
        }

        serde_json::to_string(&display_results).unwrap()
    }
}

#[derive(Serialize)]
struct DisplayResult
{
    pub match_obj: Match,
    pub display_entry: DisplayDictionaryEntry,
    pub query: String,
}
