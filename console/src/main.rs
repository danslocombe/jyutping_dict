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
    let no_query = args.iter().any(|x| x.eq_ignore_ascii_case("no_query"));
    
    // Check for single query parameter --query "search term"
    let single_query = args.iter()
        .position(|x| x.eq_ignore_ascii_case("--query"))
        .and_then(|i| args.get(i + 1))
        .map(|s| s.clone());

    // Check for --batch mode (reads queries from stdin, one per line)
    let batch = args.iter().any(|x| x.eq_ignore_ascii_case("--batch"));

    // Check for limit parameter --limit N
    let limit = args.iter()
        .position(|x| x.eq_ignore_ascii_case("--limit"))
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(5); // Default to 5 results

    let (data_path, name, print_debug) = if test_set {
        ("../test", "test", true)
    }
    else {
        ("../full", "full", false)
    };


    let index_path = format!("{}/{}.jyp_dict", data_path, name);

    if (build)
    {
        println!("Building...");
        let mut builder = dictlib::builder::Builder::default();
        let mut trad_to_frequency = dictlib::builder::TraditionalToFrequencies::parse(&format!("{}/frequencies.txt", data_path));

        // Use trad→simp mappings from both dictionary files to fill in frequency
        // data for traditional characters whose simplified form is in the freq file
        trad_to_frequency.add_simplified_fallbacks(&format!("{}/cedict_ts.u8", data_path));
        trad_to_frequency.add_simplified_fallbacks(&format!("{}/cccanto-webdist.txt", data_path));

        // Override costs for Cantonese-specific characters with no Mandarin equivalent
        trad_to_frequency.add_cantonese_overrides();

        // Cedict is
        // Traditional / Pinyin / English Definition.
        builder.parse_cedict(&format!("{}/cedict_ts.u8", data_path), &trad_to_frequency);

        let trad_to_jyutping = dictlib::builder::TraditionalToJyutping::parse(&format!("{}/cccedict-canto-readings-150923.txt", data_path));
        builder.annotate(&trad_to_jyutping);

        builder.parse_ccanto(&format!("{}/cccanto-webdist.txt", data_path), &trad_to_frequency);

        if print_debug {
            println!("Data\n{:#?}", builder);
        }

        builder.apply_additional_heuristics();

        builder.mark_attested_words(&format!("{}/wordshk-headwords.txt", data_path));

        let word_frequencies = dictlib::builder::WordFrequencies::parse(&format!("{}/lihkg-freq.tsv", data_path));
        builder.mark_word_frequencies(&word_frequencies);

        let built_dictionary = CompiledDictionary::from_builder(builder);

        let dump_entries = false;
        if (dump_entries)
        {
            built_dictionary.dump_entries("entries_dump.txt");
        }

        println!("Writing to {}", &index_path);
        let mut data_writer = data_writer::DataWriter::new(&index_path);
        built_dictionary.serialize(&mut data_writer).unwrap();
        println!("Writing done!");
        return;
    }

    let compiled_dictionary = {
        println!("Reading from {}", &index_path);
        let mut f = std::fs::File::open(index_path).unwrap();
        let mut buffer = Vec::new();
        f.read_to_end(&mut buffer).unwrap();

        let mut data_reader = data_reader::DataReader::new(&buffer[..]);
        CompiledDictionary::deserialize(&mut data_reader)
    };

    if (print_debug) {
        println!("Compiled Dictionary\n{:#?}", compiled_dictionary);
    }

    let mut buffer = String::new();

    if (no_query)
    {
        println!("'no_query' specified - exiting");
        return;
    }

    // Handle single query mode
    if let Some(query) = single_query {
        println!("=====================");
        println!("Query: {}", query);
        println!("\n");

        let stopwatch = Box::new(NativeStopwatch::new());
        let result = compiled_dictionary.search(&query.trim(), limit, stopwatch);

        for m in result.matches
        {
            let display = compiled_dictionary.get_diplay_entry(m.match_obj.entry_id);
            println!("(Match {:?})\n{:#?}", m, display);
        }
        return;
    }

    // Handle batch mode: read queries from stdin, one per line
    if batch {
        let stdin = std::io::stdin();
        let reader = std::io::BufReader::new(stdin.lock());
        use std::io::BufRead;
        for line in reader.lines() {
            let query = match line {
                Ok(q) => q,
                Err(_) => break,
            };
            let query = query.trim().to_string();
            if query.is_empty() {
                continue;
            }

            println!("=====================");
            println!("Query: {}", query);
            println!("\n");

            let stopwatch = Box::new(NativeStopwatch::new());
            let result = compiled_dictionary.search(&query, limit, stopwatch);

            for m in result.matches {
                let display = compiled_dictionary.get_diplay_entry(m.match_obj.entry_id);
                println!("(Match {:?})\n{:#?}", m, display);
            }
            println!("===QUERY_END===");
        }
        return;
    }

    // Interactive mode
    loop {
        buffer.clear();

        println!("=====================");
        print!("Query: ");
        std::io::stdout().flush().unwrap();
        std::io::stdin().read_line(&mut buffer).unwrap();
        println!("\n\n");

        let stopwatch = Box::new(NativeStopwatch::new());
        let result = compiled_dictionary.search(&buffer.trim(), limit, stopwatch);

        for m in result.matches
        {
            let display = compiled_dictionary.get_diplay_entry(m.match_obj.entry_id);
            println!("(Match {:?})\n{:#?}", m, display);
        }
    }
}
