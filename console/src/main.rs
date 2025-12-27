#![allow(dead_code)]
#![allow(unused_parens)]

use std::io::Write;
use std::io::Read;

use dictlib::compiled_dictionary::CompiledDictionary;
use dictlib::*;

fn main() {
    let args : Vec<String> = std::env::args().collect();

    let build = args.iter().any(|x| x.eq_ignore_ascii_case("build"));
    let test_set = args.iter().any(|x| x.eq_ignore_ascii_case("test_set"));

    let (data_path, print_debug) = if test_set {
        ("../test", true)
    }
    else {
        ("../full", false)
    };


    let write_path = format!("{}/test.jyp_dict", data_path);

    if (build)
    {
        println!("Building...");
        let mut builder = dictlib::builder::Builder::default();
        let trad_to_frequency = dictlib::builder::TraditionalToFrequencies::parse(&format!("{}/frequencies.txt", data_path));

        // Cedict is
        // Traditional / Pinyin / English Definition.
        builder.parse_cedict(&format!("{}/cedict_ts.u8", data_path), &trad_to_frequency);

        let trad_to_jyutping = dictlib::builder::TraditionalToJyutping::parse(&format!("{}/cccedict-canto-readings-150923.txt", data_path));
        builder.annotate(&trad_to_jyutping);

        builder.parse_ccanto(&format!("{}/cccanto-webdist.txt", data_path));

        if print_debug {
            println!("Data\n{:#?}", builder);
        }

        let write_path = format!("{}/test.jyp_dict", data_path);

        let built_dictionary = CompiledDictionary::from_builder(builder);

        let dump_entries = false;
        if (dump_entries)
        {
            built_dictionary.dump_entries("entries_dump.txt");
        }

        println!("Writing to {}", &write_path);
        let mut data_writer = data_writer::DataWriter::new(&write_path);
        built_dictionary.serialize(&mut data_writer).unwrap();
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

        println!("=====================");
        print!("Query: ");
        std::io::stdout().flush().unwrap();
        std::io::stdin().read_line(&mut buffer).unwrap();
        println!("\n\n");

        let stopwatch = Box::new(NativeStopwatch::new());
        let result = compiled_dictionary.search(&buffer.trim(), stopwatch);

        for m in result.matches
        {
            let display = compiled_dictionary.get_diplay_entry(m.match_obj.entry_id);
            println!("(Match {:?})\n{:#?}", m, display);
        }
    }
}
