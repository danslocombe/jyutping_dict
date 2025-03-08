#![allow(dead_code)]
#![allow(unused_parens)]

use std::{collections::BTreeMap, io::Read};

use dictlib::compiled_dictionary::CompiledDictionary;
use dictlib::compiled_dictionary::DisplayDictionaryEntry;

use dictlib::*;

fn main() {
    let mut defs = TraditionalToDefinitions::default();

    let test_set = false;

    let (data_path, print_debug) = if test_set {
        ("../test", true)
    }
    else {
        ("../full", false)
    };

    // Cedict is
    // Traditional / Pinyin / English Definition.
    defs.parse_cedict(&format!("{}/cedict_ts.u8", data_path));

    if (print_debug) {
        println!("Defs0\n{:#?}", defs);
    }

    let mut trad_to_jyutping = TraditionalToJyutping::parse(&format!("{}/cccedict-canto-readings-150923.txt", data_path));
    let mut trad_to_frequency = TraditionalToFrequencies::parse(&format!("{}/frequencies.txt", data_path));

    defs.parse_ccanto(&mut trad_to_jyutping, &mut trad_to_frequency, &format!("{}/cccanto-webdist.txt", data_path));

    if (print_debug) {
        println!("Defs1\n{:#?}", defs);
    }

    let dict = Dictionary {
        trad_to_def : defs,
        trad_to_jyutping,
        trad_to_frequency,
    };

    if print_debug {
        println!("Data\n{:#?}", dict);
    }

    //let char = "人";

    //let frequency_data = trad_to_frequency.inner.get(char).unwrap();
    //let jyutping = trad_to_jyutping.inner.get(char).unwrap();
    //let def = trad_to_def.inner.get(char).unwrap();

    //println!("{} - {} {:?} - {:?}", char, jyutping, def, frequency_data);

    //let hacky_results = dict.hacky_search("fu2");
    //println!("fu2 results: \n{:#?}", hacky_results);
    //return;

    let write_path = format!("{}/test.jyp_dict", data_path);
    
    {
        let compiled_dictionary = CompiledDictionary::from_dictionary(dict);

        println!("Writing to {}", &write_path);
        let mut data_writer = data_writer::DataWriter::new(&write_path);
        compiled_dictionary.serialize(&mut data_writer).unwrap();
        println!("Writing done!");
    }

    let compiled_dictionary = {
        println!("Reading from {}", &write_path);
        let mut f = std::fs::File::open(write_path).unwrap();
        let mut buffer = Vec::new();
        f.read_to_end(&mut buffer).unwrap();

        let mut data_reader = data_reader::DataReader::new(&buffer[..]);
        CompiledDictionary::deserialize(&mut data_reader)
    };

    if (print_debug) {
        println!("Compiled Dictionary\n{:#?}", compiled_dictionary);
    }

    let mut buffer = String::new();

    loop {
        buffer.clear();

        println!("Query: ");
        std::io::stdin().read_line(&mut buffer).unwrap();

        let matches = compiled_dictionary.search(&buffer.trim());

        for (match_cost_info, m) in matches
        {
            let display = DisplayDictionaryEntry::from_entry(m, &compiled_dictionary);
            println!("(Match Cost {:?}) - {:#?}", match_cost_info, display);
        }
    }

    //let mut buffer = String::new();

    //loop {
    //    buffer.clear();

    //    println!("Query: ");
    //    std::io::stdin().read_line(&mut buffer).unwrap();

    //    let results = dict.hacky_search(&buffer);
    //    println!("{:#?}", results);
    //}

}