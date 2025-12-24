"use strict";

import { JyutpingSearch } from "../pkg/index.js"

const query_string = window.location.search;
const url_params = new URLSearchParams(query_string);
var query = url_params.get('q');
var toki_sama;
var textfield = document.getElementById("entry");
var resultsfield = document.getElementById("results");
var explanation = document.getElementById("explanation");

var debug = url_params.get('debug') === '1';

fetch("test.jyp_dict")
    .then(response => {
        if (!response.ok) {
            throw new Error(`Failed to fetch dictionary blob: ${response.status} ${response.statusText}`);
        }
        return response.arrayBuffer();
  })
  .then(data => {
    console.log("Got dictionary blob {} bytes", data.byteLength);
    const data_array = new Uint8Array(data);
    toki_sama = new JyutpingSearch(data_array);
    console.log("Finished search init!");

    textfield.removeAttribute("disabled");
    textfield.setAttribute("placeholder", "teacher");
    textfield.focus();

    const input_function = prefix => {
        resultsfield.innerHTML = "";

        if (prefix.length > 0) {
            render(prefix, toki_sama.search(prefix));
            explanation.hidden = true;

            // Update URL query parameter
            const newUrl = new URL(window.location);
            newUrl.searchParams.set('q', prefix);
            window.history.replaceState({}, '', newUrl);
        }
        else {
            textfield.setAttribute("placeholder", "");
            explanation.hidden = false;

            // Remove query parameter when search is empty
            const newUrl = new URL(window.location);
            newUrl.searchParams.delete('q');
            window.history.replaceState({}, '', newUrl);
        }
    };

    textfield.addEventListener('input', (e) => input_function(e.target.value));

    if (query)
    {
        input_function(query);
        textfield.value = query;
    }
});

// Get colouring classes for different translation sources
function get_class_by_source(source) {
    if (source === "CEDict") {
        return "generated";
    }
    else if (source === "CCanto") {
        return "nimi-pu";
    }
    else {
        return "";
    }
}

// Highlight matching text based on matched spans
// matched_spans is an array of [field_index, start_pos, end_pos]
function highlightText(text, matched_spans) {
    // Find spans that match this field
    //const relevant_spans = matched_spans.filter(span => span[0] === field_index);
  const relevant_spans = matched_spans;

    if (relevant_spans.length === 0) {
        return escapeHtml(text);
    }

    // Sort spans by start position
    relevant_spans.sort((a, b) => a[0] - b[0]);

    // Build highlighted text
    let result = '';
    let last_pos = 0;

    for (let span of relevant_spans) {
        const start = span[0];
        const end = span[1];

        // Add text before the match
        if (start > last_pos) {
            result += escapeHtml(text.substring(last_pos, start));
        }

        // Add highlighted match
        result += '<mark class="hit-highlight">' + escapeHtml(text.substring(start, end)) + '</mark>';
        last_pos = end;
    }

    // Add remaining text
    if (last_pos < text.length) {
        result += escapeHtml(text.substring(last_pos));
    }

    return result;
}

// Escape HTML special characters
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function get_class_by_source_toki(source) {
    if (source === "Generated") {
        return "generated";
    }
    else if (source === "NimiPu") {
        return "nimi-pu";
    }
    else if (source === "Compounds") {
        return "compounds";
    }
    else {
        return "";
    }
}

// Render a search result
function render(prefix, results_string) {
    const results = JSON.parse(results_string)

    if (results.length == 0) {
        return;
    }

    // Card element for all results
    var card = document.createElement("ul");
    card.setAttribute("class", "card");

    for (let result of results) {
        let title = document.createElement("li");
        title.setAttribute("class", "card-item");

        let source = result.display_entry.entry_source;
        let source_class = get_class_by_source(source);

        let traditional_elem = document.createElement("span");
        traditional_elem.setAttribute("class", "item-english");

        let title_traditional = document.createElement("h2");
        title_traditional.setAttribute("class", "title");
        let traditional = result.display_entry.characters;
        if (result.match_obj.match_type === "Traditional") {
            traditional = highlightText(traditional, result.match_obj.matched_spans);
        }
        title_traditional.innerHTML = traditional;
        traditional_elem.appendChild(title_traditional);

        let jyutping_elem = document.createElement("span");
        jyutping_elem.setAttribute("class", "item-toki-pona");

        {
            let title_jyutping = document.createElement("h3");
            title_jyutping.setAttribute("class", "title");
            title_jyutping.setAttribute("title", result.display_entry.entry_source);

            let jyutping = result.display_entry.jyutping;
            if (result.match_obj.match_type === "Jyutping") {
              jyutping = highlightText(jyutping, result.match_obj.matched_spans)
            }
            title_jyutping.innerHTML = jyutping;
            jyutping_elem.appendChild(title_jyutping);
        }

        title.appendChild(jyutping_elem);
        title.appendChild(traditional_elem);

        card.appendChild(title);

        for (let i = 0; i < result.display_entry.english_definitions.length; i++) {
            let english = result.display_entry.english_definitions[i];
            let similar_elem = document.createElement("li");
            similar_elem.setAttribute("class", "card-item");

            let english_elem = document.createElement("span");
            english_elem.setAttribute("class", "item-english indent");

            // TODO highlight
            english_elem.innerHTML = english;
            //english_elem.innerHTML = highlightText(english, result.match_obj.matched_spans);

            similar_elem.appendChild(english_elem);

            card.appendChild(similar_elem);
        }

        let source_elem = document.createElement("p");
        source_elem.setAttribute("class", "item-english " + source_class);
        if (source === "CEDict")
        {
            source_elem.innerText = "(Sourced from CEDict)";
        }
        else if (source == "CCanto")
        {
            source_elem.innerText = "(Sourced from CC-Canto)";
        }

        card.appendChild(source_elem);

        if (debug) {
            let debug_elem = document.createElement("div");
            debug_elem.setAttribute("class", "debug-info");

            let json_elem = document.createElement("pre");
            json_elem.innerText = JSON.stringify(result, null, 2);
            debug_elem.appendChild(json_elem);

            card.appendChild(debug_elem);
        }
    }

    resultsfield.appendChild(card);
}

function highlight_completion(prefix, full) {
    let res = prefix;
    const completion =full.substring(prefix.length);
    res += "<b>";
    res += completion;
    res += "</b>";

    return res;
}
