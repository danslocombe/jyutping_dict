"use strict";

import { JyutpingSearch } from "../pkg/index.js"

const query_string = window.location.search;
const url_params = new URLSearchParams(query_string);
var query = url_params.get('q');
var jyutping_search;
var textfield = document.getElementById("entry");
var resultsfield = document.getElementById("results");
var explanation = document.getElementById("explanation");

var debug = url_params.get('debug') === '1';

fetch("test.jyp_dict", { cache: 'force-cache' })
    .then(response => {
        if (!response.ok) {
            throw new Error(`Failed to fetch dictionary blob: ${response.status} ${response.statusText}`);
        }
        return response.arrayBuffer();
  })
  .then(data => {
    console.log("Got dictionary blob {} bytes", data.byteLength);
    const data_array = new Uint8Array(data);
    jyutping_search = new JyutpingSearch(data_array);
    console.log("Finished search init!");

    textfield.removeAttribute("disabled");
    textfield.setAttribute("placeholder", "teacher");
    textfield.focus();

    const input_function = prefix => {
        resultsfield.innerHTML = "";

        if (prefix.length > 0) {
            render(prefix, jyutping_search.search(prefix));
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

// Render a search result
function render(prefix, results_string) {
    const search_result = JSON.parse(results_string)
    const results = search_result.results;
    console.log(search_result.timings)

    if (results.length == 0) {
        return;
    }

    // Card element for all results
    var card = document.createElement("ul");
    card.setAttribute("class", "card");

    for (let result of results) {
        let title = document.createElement("li");
        title.setAttribute("class", "card-item");

        let source = result.rendered_entry.entry_source;
        let source_class = get_class_by_source(source);

        let traditional_elem = document.createElement("span");
        traditional_elem.setAttribute("class", "item-english");

        let title_traditional = document.createElement("h2");
        title_traditional.setAttribute("class", "title");
        // Use pre-highlighted characters (already contains HTML markup)
        title_traditional.innerHTML = result.rendered_entry.characters;
        traditional_elem.appendChild(title_traditional);

        let jyutping_elem = document.createElement("span");
        jyutping_elem.setAttribute("class", "item-jyutping");

        {
            let title_jyutping = document.createElement("h3");
            title_jyutping.setAttribute("class", "title");
            title_jyutping.setAttribute("title", result.rendered_entry.entry_source);

            // Use pre-highlighted jyutping (already contains HTML markup)
            title_jyutping.innerHTML = result.rendered_entry.jyutping;
            jyutping_elem.appendChild(title_jyutping);
        }

        title.appendChild(jyutping_elem);
        title.appendChild(traditional_elem);

        card.appendChild(title);

        for (let i = 0; i < result.rendered_entry.english_definitions.length; i++) {
            let english = result.rendered_entry.english_definitions[i];
            let similar_elem = document.createElement("li");
            similar_elem.setAttribute("class", "card-item");

            let english_elem = document.createElement("span");
            english_elem.setAttribute("class", "item-english indent");

            // Use pre-highlighted english (already contains HTML markup)
            english_elem.innerHTML = english;

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
