/// <reference path="./types.js" />
import { Api } from './api/api.js';

/** @type {Api} */
const backend = window.backendCall;

/** @type {ApiUrls} */
const api = window.apiUrls;

/** @type {number} */
const deckVersionId = window.deckVersionId;

export const zones = {
    main: document.getElementById("main-deck"),
    extra: document.getElementById("extra-deck"),
    side: document.getElementById("side-deck"),
    reserve: document.getElementById("reserve"),
};

// -------------------- Deck Actions --------------------


document.getElementById("save-btn").addEventListener("click", () => {
    const mainIds = [...zones.main.querySelectorAll(".card")].map(c => c.dataset.id);
    const extraIds = [...zones.extra.querySelectorAll(".card")].map(c => c.dataset.id);
    const sideIds = [...zones.side.querySelectorAll(".card")].map(c => c.dataset.id);
    const banListId = parseInt(document.getElementById("ban-list-switcher").value);


    backend.myYgoCards.deckVersionsPartialUpdate(
        deckVersionId,
        {
            main_deck: mainIds,
            extra_deck: extraIds,
            side_deck: sideIds,
            ban_list_id: banListId,
        }
    ).then(data => {
        alert("Deck saved!");

        if (data.ydke_url) {
            ydkeUrls = data.ydke_url; // ✅ update global ydke string
        }
    });
});

document.getElementById("rename-btn").addEventListener("click", () => {
    let oldName = document.getElementById("deck-name").textContent;
    oldName = oldName.split(" - ")[1]; // remove deck prefix if any
    const newName = prompt("Enter new version name:", oldName);
    if (!newName) return;

    backend.myYgoCards.deckVersionsPartialUpdate(deckVersionId, {
        version_name: newName,
    }).then(data => {
        alert("Version renamed to " + data.version_name);

        // Update page title dynamically
        document.getElementById("deck-name").textContent = data.name;

        // Update dropdown selected option too
        const deckSwitcher = document.getElementById("deck-switcher");
        if (deckSwitcher) {
            const selectedOption = deckSwitcher.querySelector("option:checked");
            if (selectedOption) {
                selectedOption.textContent = data.version_name;
            }
        }
    });
});



document.getElementById("new-btn").addEventListener("click", () => {
    const name = prompt("Enter name for new deck:", "New Deck");
    if (!name) return;

    backend.myYgoCards.deckVersionsCloneCreate(deckVersionId, {name : name})
    .then(data => {
        window.location.href = data.deck_url; // redirect to new deck view
    });
});

document.getElementById("delete-btn").addEventListener("click", () => {
    if (!confirm("Are you sure you want to delete this deck?")) return;

    backend.myYgoCards.deckVersionsDestroy(deckVersionId)
    .then(() => {
        alert("Deck deleted");
        window.location.href = api.deckListUrl();
    });
});

document.getElementById("deck-switcher").addEventListener("change", e => {
    const deckId = e.target.value;
    window.location.href = api.deckBuilderUrl(deckId);
});