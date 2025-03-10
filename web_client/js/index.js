"use strict";

import { JyutpingSearch } from "../pkg/index.js"

const query_string = window.location.search;
const url_params = new URLSearchParams(query_string);
var query = url_params.get('q');
var toki_sama;
var textfield = document.getElementById("entry");
var resultsfield = document.getElementById("results");
var explanation = document.getElementById("explanation");

Promise.all(
    [
        fetch("test.jyp_dict").then(x => x.arrayBuffer()),
    ]
)
.then(([data]) => {
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
        }
        else {
            textfield.setAttribute("placeholder", "");
            explanation.hidden = false;
        }
    };

    textfield.addEventListener('input', (e) => input_function(e.target.value));

    var query = url_params.get('q');
    if (query)
    {
        input_function(query);
        textfield.value = query;
    }
})

// Get colouring classes for different translation sources
function get_class_by_source(source) {
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

        let traditional_elem = document.createElement("span");
        traditional_elem.setAttribute("class", "item-english");

        let title_traditional = document.createElement("h3");
        title_traditional.setAttribute("class", "title");
        title_traditional.innerHTML = result.display_entry.characters
        traditional_elem.appendChild(title_traditional);

        let jyutping_elem = document.createElement("span");
        jyutping_elem.setAttribute("class", "item-toki-pona");

        let title_jyutping = document.createElement("h3");

        title_jyutping.setAttribute("class", "title");
        title_jyutping.setAttribute("title", result.source);
        title_jyutping.innerHTML = result.display_entry.jyutping;
        jyutping_elem.appendChild(title_jyutping);

        title.appendChild(jyutping_elem);
        title.appendChild(traditional_elem);

        card.appendChild(title);

        for (let english of result.display_entry.english_definitions) {
            let similar_elem = document.createElement("li");
            similar_elem.setAttribute("class", "card-item");

            let english_elem = document.createElement("span");
            english_elem.setAttribute("class", "item-english");
            english_elem.innerHTML = english;

            //let toki_elem = document.createElement("span");
            //toki_elem.setAttribute("class", "item-toki-pona " + get_class_by_source(similar.source));
            //toki_elem.setAttribute("title", similar.source);
            //toki_elem.innerHTML = similar.toki_pona_string;

            similar_elem.appendChild(english_elem);
            //similar_elem.appendChild(toki_elem);

            card.appendChild(similar_elem);
        }
    }

    /*
    // Create key
    {
        let key_elem = document.createElement("li");
        key_elem.setAttribute("class", "card-item");

        let english_elem = document.createElement("span");
        english_elem.setAttribute("class", "item-english");
        english_elem.innerHTML = "English";

        let toki_elem = document.createElement("span");
        toki_elem.setAttribute("class", "item-toki-pona");
        toki_elem.innerHTML = "toki pona";

        key_elem.appendChild(english_elem);
        key_elem.appendChild(toki_elem);

        card.appendChild(key_elem);
    }
        */

    // Start rendering results
    /*
    for (let result of results) {
        let title = document.createElement("li");
        title.setAttribute("class", "card-item");

        let english_elem = document.createElement("span");
        english_elem.setAttribute("class", "item-english");

        let title_english = document.createElement("h3");
        title_english.setAttribute("class", "title");
        title_english.innerHTML = highlight_completion(prefix, result.english_search);
        english_elem.appendChild(title_english);

        let toki_elem = document.createElement("span");
        toki_elem.setAttribute("class", "item-toki-pona");

        let title_toki = document.createElement("h3");

        title_toki.setAttribute("class", "title " + get_class_by_source(result.source));
        title_toki.setAttribute("title", result.source);
        title_toki.innerHTML = result.original_translation_string;
        toki_elem.appendChild(title_toki);

        title.appendChild(english_elem);
        title.appendChild(toki_elem);

        card.appendChild(title);

        for (let similar of result.similar) {
            let similar_elem = document.createElement("li");
            similar_elem.setAttribute("class", "card-item");

            let english_elem = document.createElement("span");
            english_elem.setAttribute("class", "item-english");
            english_elem.innerHTML = similar.english;

            let toki_elem = document.createElement("span");
            toki_elem.setAttribute("class", "item-toki-pona " + get_class_by_source(similar.source));
            toki_elem.setAttribute("title", similar.source);
            toki_elem.innerHTML = similar.toki_pona_string;

            similar_elem.appendChild(english_elem);
            similar_elem.appendChild(toki_elem);

            card.appendChild(similar_elem);
        }
    }

    resultsfield.appendChild(card);
    */

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