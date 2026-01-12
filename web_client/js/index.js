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

// Track current search state
var currentQuery = "";
var currentMaxResults = 12;

const current_index_filename = "full.jyp_dict";

loadDictionary(current_index_filename)
  .then(data => {
    console.log("Got dictionary blob", data.byteLength, "bytes");
    const data_array = new Uint8Array(data);
    jyutping_search = new JyutpingSearch(data_array);
    console.log("Finished search init!");

    textfield.removeAttribute("disabled");
    textfield.setAttribute("placeholder", "lou5 si1, teacher, 老師, ...");
    textfield.focus();

    const input_function = prefix => {
        resultsfield.innerHTML = "";

        if (prefix.length > 0) {
            // Reset max_results if query changed
            if (currentQuery !== prefix) {
                currentQuery = prefix;
                currentMaxResults = 12;
            }
            
            const results_string = jyutping_search.search(prefix, currentMaxResults);
            render(results_string);
            explanation.hidden = true;

            // Update URL query parameter
            const newUrl = new URL(window.location);
            newUrl.searchParams.set('q', prefix);
            window.history.replaceState({}, '', newUrl);
        }
        else {
            textfield.setAttribute("placeholder", "");
            explanation.hidden = false;
            currentQuery = "";
            currentMaxResults = 12;

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
        return "ce-dict";
    }
    else if (source === "CCanto") {
        return "cc-canto";
    }
    else {
        return "";
    }
}

// Render a search result
function render(results_string) {
    const search_result = JSON.parse(results_string)
    const results = search_result.results;

    if (results.length == 0) {
        return;
    }

    // Card element for all results
    var card = document.createElement("ul");
    card.setAttribute("class", "card");

    if (debug) {
        let debug_elem = document.createElement("div");
        debug_elem.setAttribute("class", "debug-info");

        let json_elem = document.createElement("pre");
        json_elem.innerText = JSON.stringify(search_result.timings, null, 2);
        debug_elem.appendChild(json_elem);

        card.appendChild(debug_elem);

        const hr_elem = document.createElement("hr");
        card.appendChild(hr_elem);
    }

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
        title_traditional.innerHTML = makeCharactersClickable(result.rendered_entry.characters);
        traditional_elem.appendChild(title_traditional);

        let jyutping_elem = document.createElement("span");
        jyutping_elem.setAttribute("class", "item-jyutping");

        {
            let title_jyutping = document.createElement("h3");
            title_jyutping.setAttribute("class", "title");
            title_jyutping.setAttribute("title", result.rendered_entry.entry_source);

            // Use pre-highlighted jyutping (already contains HTML markup)
            title_jyutping.innerHTML = makeJyutpingClickable(result.rendered_entry.jyutping);
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
    
    // Add "Load More" button only if we got the max number of results (meaning there might be more)
    if (results.length === currentMaxResults) {
        var loadMoreBtn = document.createElement("button");
        loadMoreBtn.setAttribute("class", "load-more-btn");
        loadMoreBtn.innerText = "More";
        loadMoreBtn.onclick = () => {
            currentMaxResults *= 2;
            const input_function_for_load_more = () => {
                resultsfield.innerHTML = "";
                const results_string = jyutping_search.search(currentQuery, currentMaxResults);
                render(results_string);
            };
            input_function_for_load_more();
        };
        resultsfield.appendChild(loadMoreBtn);
    }
}

// Helper function to make jyutping terms clickable
function makeJyutpingClickable(jyutpingHtml) {
    const container = document.createElement('div');
    container.innerHTML = jyutpingHtml;
    
    const result = document.createElement('div');
    wrapJyutpingSyllables(container, result);
    
    return result.innerHTML;
}

// Helper function to make traditional characters clickable
function makeCharactersClickable(charactersHtml) {
    const container = document.createElement('div');
    container.innerHTML = charactersHtml;
    
    const result = document.createElement('div');
    wrapCharacters(container, result);
    
    return result.innerHTML;
}

// Wrap jyutping syllables in links, preserving markup like <mark> tags
function wrapJyutpingSyllables(sourceNode, targetNode) {
    let currentLink = null;
    let currentText = '';
    
    function flushLink() {
        if (currentLink && currentText.trim().length > 0) {
            currentLink.href = `?q=${encodeURIComponent(currentText.trim())}`;
            targetNode.appendChild(currentLink);
            currentLink = null;
            currentText = '';
        }
    }
    
    function processNode(node) {
        if (node.nodeType === Node.TEXT_NODE) {
            const text = node.textContent;
            const parts = text.split(/(\s+)/);
            
            for (let part of parts) {
                if (part.trim().length > 0) {
                    // Start a new link if needed
                    if (!currentLink) {
                        currentLink = document.createElement('a');
                        currentLink.className = 'jyutping-link';
                    }
                    currentLink.appendChild(document.createTextNode(part));
                    currentText += part;
                } else if (part.length > 0) {
                    // Whitespace - flush current link and add whitespace
                    flushLink();
                    targetNode.appendChild(document.createTextNode(part));
                }
            }
        } else if (node.nodeType === Node.ELEMENT_NODE) {
            // Clone element and add to current link (or create new link if needed)
            if (!currentLink) {
                currentLink = document.createElement('a');
                currentLink.className = 'jyutping-link';
            }
            
            const clonedElement = document.createElement(node.tagName);
            for (let attr of node.attributes) {
                clonedElement.setAttribute(attr.name, attr.value);
            }
            
            // Process children into the cloned element
            for (let child of node.childNodes) {
                processNodeIntoElement(child, clonedElement);
            }
            
            currentLink.appendChild(clonedElement);
        }
    }
    
    function processNodeIntoElement(node, targetElement) {
        if (node.nodeType === Node.TEXT_NODE) {
            targetElement.appendChild(document.createTextNode(node.textContent));
            currentText += node.textContent;
        } else if (node.nodeType === Node.ELEMENT_NODE) {
            const clonedElement = document.createElement(node.tagName);
            for (let attr of node.attributes) {
                clonedElement.setAttribute(attr.name, attr.value);
            }
            for (let child of node.childNodes) {
                processNodeIntoElement(child, clonedElement);
            }
            targetElement.appendChild(clonedElement);
        }
    }
    
    for (let child of sourceNode.childNodes) {
        processNode(child);
    }
    
    flushLink();
}

// Wrap each character in a link, preserving markup like <mark> tags  
function wrapCharacters(sourceNode, targetNode) {
    function processNode(node) {
        if (node.nodeType === Node.TEXT_NODE) {
            const text = node.textContent;
            for (let char of text) {
                const link = document.createElement('a');
                link.href = `?q=${encodeURIComponent(char)}`;
                link.className = 'character-link';
                link.textContent = char;
                targetNode.appendChild(link);
            }
        } else if (node.nodeType === Node.ELEMENT_NODE) {
            // For character wrapping, we need to wrap content within marks
            const clonedElement = document.createElement(node.tagName);
            for (let attr of node.attributes) {
                clonedElement.setAttribute(attr.name, attr.value);
            }
            
            // Process children into links within the element
            for (let child of node.childNodes) {
                if (child.nodeType === Node.TEXT_NODE) {
                    for (let char of child.textContent) {
                        const link = document.createElement('a');
                        link.href = `?q=${encodeURIComponent(char)}`;
                        link.className = 'character-link';
                        link.textContent = char;
                        clonedElement.appendChild(link);
                    }
                } else {
                    processNodeIntoElement(child, clonedElement);
                }
            }
            
            targetNode.appendChild(clonedElement);
        }
    }
    
    function processNodeIntoElement(node, targetElement) {
        if (node.nodeType === Node.TEXT_NODE) {
            for (let char of node.textContent) {
                const link = document.createElement('a');
                link.href = `?q=${encodeURIComponent(char)}`;
                link.className = 'character-link';
                link.textContent = char;
                targetElement.appendChild(link);
            }
        } else if (node.nodeType === Node.ELEMENT_NODE) {
            const clonedElement = document.createElement(node.tagName);
            for (let attr of node.attributes) {
                clonedElement.setAttribute(attr.name, attr.value);
            }
            for (let child of node.childNodes) {
                processNodeIntoElement(child, clonedElement);
            }
            targetElement.appendChild(clonedElement);
        }
    }
    
    for (let child of sourceNode.childNodes) {
        processNode(child);
    }
}

async function loadDictionary(filename) {
    try {
        const db = await openDB();
        const cached = await getCachedDict(db, filename);
        
        if (cached) {
            console.log("Using cached dictionary from IndexedDB");
            return cached;
        }
    } catch (err) {
        console.warn("IndexedDB access failed, falling back to network:", err);
    }
    
    console.log("Fetching dictionary from network");
    const response = await fetch(filename, { cache: 'force-cache' });
    if (!response.ok) {
        throw new Error(`Failed to fetch dictionary blob: ${response.status} ${response.statusText}`);
    }
    const data = await response.arrayBuffer();
    
    try {
        const db = await openDB();
        await cacheDict(db, filename, data);
        console.log("Dictionary cached in IndexedDB");
    } catch (err) {
        console.warn("Failed to cache dictionary in IndexedDB:", err);
    }
    
    return data;
}

const DB_STORE_NAME = 'jyut';

function openDB() {
    return new Promise((resolve, reject) => {
        const DB_NAME = 'jyutping_dict_cache';
        const DB_VERSION = 1;
        const request = indexedDB.open(DB_NAME, DB_VERSION);
        
        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve(request.result);
        
        request.onupgradeneeded = (event) => {
            const db = event.target.result;
            if (!db.objectStoreNames.contains(DB_STORE_NAME)) {
                db.createObjectStore(DB_STORE_NAME);
            }
        };
    });
}

function getCachedDict(db, filename) {
    return new Promise((resolve, reject) => {
        const transaction = db.transaction([DB_STORE_NAME], 'readonly');
        const store = transaction.objectStore(DB_STORE_NAME);
        const request = store.get(filename);
        
        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve(request.result);
    });
}

function cacheDict(db, filename, data) {
    return new Promise((resolve, reject) => {
        const transaction = db.transaction([DB_STORE_NAME], 'readwrite');
        const store = transaction.objectStore(DB_STORE_NAME);
        const request = store.put(data, filename);
        
        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve();
    });
}