use dictlib::{compiled_dictionary::{CompiledDictionary, DisplayDictionaryEntry}, data_reader::DataReader, DebugLogger};
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
        for (match_info, entry) in results
        {
            let display_entry = DisplayDictionaryEntry::from_entry(entry, &self.dict);
            display_results.push(DisplayResult
            {
                cost: match_info.total(),
                match_cost: match_info.match_cost,
                static_cost: match_info.static_cost,
                
                display_entry,
            })
        }

        serde_json::to_string(&display_results).unwrap()
        //let results = self.toki_sama.lookup(prefix, &self.pu);
        //serde_json::to_string(&results).unwrap()
        //String::default()
    }
}

#[derive(Serialize)]
struct DisplayResult
{
    pub cost: u32,
    pub match_cost: u32,
    pub static_cost: u32,

    pub display_entry: DisplayDictionaryEntry,
}