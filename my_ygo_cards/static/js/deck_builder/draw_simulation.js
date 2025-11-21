/**
 * @fileoverview Deck Builder - Draw Simulation
 * 
 * Need the card.js loaded as well for card rendering and overview.
 * 
 */

/// <reference path="./types.js" />

import {zones} from "./deck_management.js";
import {showCardOverview, sortCards, clearCategoriesCache} from "./card.js";
import { Api } from './api/api.js';

/** @type {Api} */
const backend = window.backendCall;

/** @type {ApiUrls} */
const api = window.apiUrls;

/** @type {string} */
const baseImgUrl = window.imgBaseUrl;

/** @type {number} */
const deckVersionId = window.deckVersionId;

// -------------------- test draw --------------------
// Shuffle function (Fisher-Yates algorithm)
function shuffle(array) {
    let a = [...array];
    for (let i = a.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [a[i], a[j]] = [a[j], a[i]];
    }
    return a;
}

const testDrawBtn   = document.getElementById("test-draw-btn");
const testDrawCount = document.getElementById("test-draw-count");
const testDrawResult = document.getElementById("test-draw-result");

testDrawBtn.addEventListener("click", () => {
    const mainDeck = zones.main;
    const cards = [...mainDeck.querySelectorAll(".card")]; // all cards in main

    const drawCount = Math.min(parseInt(testDrawCount.value, 10) || 1, cards.length);

    if (cards.length === 0) return;

    // Clear previous draw
    testDrawResult.innerHTML = "";

    // Shuffle copy
    const shuffled = shuffle(cards);

    // Take first N cards and clone them
    const drawn = shuffled.slice(0, drawCount).map(card => card.cloneNode(true));

    // Important: re-bind click overview + draggable properties to cloned cards
    drawn.forEach(card => {
        card.draggable = false; // avoid accidental dragging into deck
        card.addEventListener("click", () => showCardOverview(card));
        testDrawResult.appendChild(card);
    });

    sortCards(testDrawResult);
});


const statsContent = document.getElementById("stats-content");
let deckCategoriesCache = []; // cache per deck
// monteCarlo simulation elements
const monteCarloBtn = document.getElementById("run-simulation-btn");
const nbSimulationsInput = document.getElementById("simulation-iterations");
let monteCarloResultsCache = null; // cache results


function renderDeckCategories() {
    let statsHtml = "";
    let additionalHeaders = "";

    if (monteCarloResultsCache === null) {
        statsHtml = `
            ${
                deckCategoriesCache.map(cat => {
                return `
                    <tr>
                        <td>${cat.name}</td>
                        <td>No info</td>
                        <td>No info</td>
                        <td><button class="remove-cat-btn" data-id="${cat.id}">✕</button></td>
                    </tr>
                `;
                }).join("")
            }
        `;
    } else {
        // Map category ID to occurrences
        const categoryResultsMap = {};
        monteCarloResultsCache.category_stats.forEach(catResult => {
            categoryResultsMap[catResult.category_id] = catResult.occurences;
        });

        // prepare details
        const maxCardsInHand = document.getElementById("test-draw-count").value;
        let resultsDetailsMap = {};

        for (let i = 0; i <= maxCardsInHand; i++) {
            additionalHeaders += `<th>${i}/hand</th>`;
        }

        monteCarloResultsCache.category_details_stats.forEach(detail => {
            resultsDetailsMap[detail.category_id] = detail.nb_per_main_occurences;
        });

        statsHtml = `
            ${
                deckCategoriesCache.map(cat => {
                    const occurrences = categoryResultsMap[cat.id] || 0;
                    const percentage = ((occurrences * 1.0 / monteCarloResultsCache.total_simulations)).toFixed(2);

                    let detailsHtml = "";
                    for (let i = 0; i <= maxCardsInHand; i++) {
                        const nbPerMain = resultsDetailsMap[cat.id] ? (resultsDetailsMap[cat.id][i] || 0) : 0;
                        const percPerMain = ((nbPerMain * 1.0 / monteCarloResultsCache.total_simulations)*100).toFixed(2);
                        detailsHtml += `<td>${nbPerMain} (${percPerMain}%)</td>`;
                    }

                    return `
                        <tr>
                            <td>${cat.name}</td>
                            <td>${occurrences}</td>
                            <td>${percentage}</td>
                            <td><button class="remove-cat-btn" data-id="${cat.id}">✕</button></td>
                            ${detailsHtml}
                        </tr>
                    `;
            }).join("")}
        `;

        const chartContainer = document.getElementById("categoryChart");
        if (chartContainer && monteCarloResultsCache !== null) {
            const ctx = chartContainer.getContext("2d");
            const maxCardsInHand = parseInt(document.getElementById("test-draw-count").value, 10);

            const datasets = deckCategoriesCache.map((cat, index) => {
                const details = monteCarloResultsCache.category_details_stats.find(
                    d => d.category_id === cat.id
                );
                const nbPerMainOccurences = details ? details.nb_per_main_occurences : {};

                // Prepare data points
                const dataPoints = [];
                for (let i = 0; i <= maxCardsInHand; i++) {
                    const nb = nbPerMainOccurences[i] || 0;
                    const perc = ((nb / monteCarloResultsCache.total_simulations) * 100).toFixed(2);
                    dataPoints.push(perc);
                }

                // Random color for each category line
                const color = `hsl(${(index * 60) % 360}, 70%, 50%)`;

                return {
                    label: cat.name,
                    data: dataPoints,
                    borderColor: color,
                    backgroundColor: color + "33",
                    fill: false,
                    tension: 0.2
                };
            });

            const labels = Array.from({ length: maxCardsInHand + 1 }, (_, i) => i.toString());

            // Destroy previous chart if exists (avoid duplicate overlay)
            if (window.categoryChartInstance) {
                window.categoryChartInstance.destroy();
            }

            window.categoryChartInstance = new Chart(ctx, {
                type: "line",
                data: {
                    labels: labels,
                    datasets: datasets
                },
                options: {
                    responsive: true,
                    interaction: { mode: "index", intersect: false },
                    plugins: {
                        title: {
                            display: true,
                            text: "Category Percentages by Number of Cards in Hand"
                        },
                        tooltip: {
                            callbacks: {
                                label: (context) => `${context.dataset.label}: ${context.parsed.y}%`
                            }
                        }
                    },
                    scales: {
                        x: {
                            title: { display: true, text: "Number of Cards in Hand" }
                        },
                        y: {
                            title: { display: true, text: "Percentage (%)" },
                            min: 0,
                            max: 100
                        }
                    }
                }
            });
        }
    }


    statsContent.innerHTML = `
        <table>
            <tr>
                <th>Category</th>
                <th>Nb</th>
                <th>Avg/main</th>
                <th>Remove</th>
                ${additionalHeaders}
            </tr>
            ${
                statsHtml
            }
        </table>
        <input type="text" id="new-category-name" placeholder="New category name">
        <button id="add-category-btn">Add Category</button>
    `;

    // DELETE category
    document.querySelectorAll(".remove-cat-btn").forEach(btn => {
        btn.addEventListener("click", async () => {
            const catId = btn.dataset.id;

            await backend.myYgoCards.deckVersionsCategoriesDeleteDestroy(catId, deckVersionId);

            deckCategoriesCache =
                deckCategoriesCache.filter(c => c.id != catId);
            clearCategoriesCache(); // clear card category assignment cache

            renderDeckCategories();
        });
    });

    // CREATE category
    document.getElementById("add-category-btn").addEventListener("click", async () => {
        const name = document.getElementById("new-category-name").value.trim();
        if (!name) return;

        const res = (await backend.myYgoCards.deckVersionsCategoriesCreate(deckVersionId, {name : name})).data;
        deckCategoriesCache.push(res);
        clearCategoriesCache(); // clear card category assignment cache
        renderDeckCategories();
    });
}

// LAUNCH simulation
monteCarloBtn.addEventListener("click", async () => {
    const nbSimulations = parseInt(nbSimulationsInput.value, 10) || 1000;
    const nbCards = document.getElementById("test-draw-count");

    monteCarloBtn.disabled = true;
    monteCarloBtn.textContent = "Running...";
    monteCarloResultsCache = null;

    const response = await backend.myYgoCards.deckVersionsMonteCarlosCreate(deckVersionId, {num_cards : nbCards.value, num_simulations : nbSimulations});
    monteCarloResultsCache = response.data.results;

    monteCarloBtn.disabled = false;
    monteCarloBtn.textContent = "Run Simulation";
    // Render results
    renderDeckCategories();
});

async function loadDeckCategories() {
    if (deckCategoriesCache.length === 0) {
        deckCategoriesCache = (await backend.myYgoCards.deckVersionsCategoriesList(deckVersionId)).data;
    }
    renderDeckCategories();
}

loadDeckCategories();


