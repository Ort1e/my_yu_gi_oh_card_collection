/// <reference path="./types.js" />

import {zones} from "./deck_management.js";

/** @type {BackendCall} */
const backend = window.backendCall;

/** @type {ApiUrls} */
const api = window.apiUrls;

/** @type {string} */
const baseImgUrl = window.imgBaseUrl;


// -------------------- Global Variables --------------------
let banListId = null;

// -------------------- Utils --------------------

export function renderCard(card) {
    const data = card.card_data || {}; // shortcut to nested card_data
  
    const div = document.createElement("div");
    div.className = "card";
    div.draggable = true;
  
    // Dataset fields mapped to new serializer fields
    div.dataset.id = card.id;
    div.dataset.name = (card.name || "").toLowerCase();
    div.dataset.en_name = (card.en_name || data.en_name || "").toLowerCase();
    div.dataset.code = card.code || "";
    div.dataset.type = (data.card_type || "").toLowerCase();
    div.dataset.is_proxy = card.is_proxy;
    div.dataset.price = card.unite_price || "N/A";
    div.dataset.url = api.cardBaseUrl(card.id);
  
    // Card image
    const img = document.createElement("img");
    const imgUrl = baseImgUrl + data.image_url;
    img.src = imgUrl;
    img.alt = data.en_name || card.en_name || card.name || "Card";

    // add ban list data
    if (banListId) {
      const banStatus = data.ban_statuses.filter(bs => bs.ban_list_id === banListId)
      console.log(banStatus)
      if (banStatus.length > 0) {
        div.dataset.ban_status = banStatus[0].status; // e.g., "forbidden", "limited", "semi-limited", "unlimited"
      }
    }

    if (!div.dataset.ban_status) {
      div.dataset.ban_status = "unlimited"; // default if not specified
    }

    div.appendChild(img);

  
    // ---- Bind interactions ----
  
    // Drag
    div.addEventListener("dragstart", e => {
      e.dataTransfer.setData("cardId", div.dataset.id);
    });
  
    // Click → overview
    div.addEventListener("click", e => {
      e.preventDefault();
      showCardOverview(div);
    });
  
    return div;
}

export function sortCards(zone) {
    const cards = [...zone.querySelectorAll(".card")];
  
    cards.sort((a, b) => {
      // Compare by type first
      const typeComparison = a.dataset.type.localeCompare(b.dataset.type);
      if (typeComparison !== 0) {
        return typeComparison;
      }
      // If types are the same, compare by en_name
      return a.dataset.en_name.localeCompare(b.dataset.en_name);
    });
  
    cards.forEach(card => zone.appendChild(card));
}

function updateCardCounts() {
  document.getElementById("count-main").textContent = zones.main.querySelectorAll(".card").length;
  document.getElementById("count-extra").textContent = zones.extra.querySelectorAll(".card").length;
  document.getElementById("count-side").textContent = zones.side.querySelectorAll(".card").length;
}

 // -------------------- get cards in decks --------------------
function get_cards_in_deck() {
  backend.getDeckVersion().then(data => {
      const mainDeckArea = zones.main;
      const extraDeckArea = zones.extra;
      const sideDeckArea = zones.side;

      mainDeckArea.innerHTML = "";
      extraDeckArea.innerHTML = "";
      sideDeckArea.innerHTML = "";

      if (data.ban_list) {
        banListId = data.ban_list.id;
      }

      data.main_deck.forEach(card => mainDeckArea.appendChild(renderCard(card)));
      data.extra_deck.forEach(card => extraDeckArea.appendChild(renderCard(card)));
      data.side_deck.forEach(card => sideDeckArea.appendChild(renderCard(card)));

      

      sortCards(mainDeckArea);
      sortCards(extraDeckArea);
      sortCards(sideDeckArea);
      updateCardCounts();
  });
}

get_cards_in_deck();
// -------------------- Proxy Management --------------------

async function deleteProxyCard(card, cardId) {
  try {
    const data = await backend.deleteProxyCard(cardId);

    
    card.remove();
    updateCardCounts();
  } catch (err) {
    console.error(err);
    alert("Error deleting proxy card");
  }
}

// Drag & Drop
function enableDragDrop(zone) {
  zone.addEventListener("dragover", e => {
    e.preventDefault();
    zone.classList.add("drag-over");
  });

  zone.addEventListener("dragleave", () => zone.classList.remove("drag-over"));

  zone.addEventListener("drop", async e => {
    e.preventDefault();
    zone.classList.remove("drag-over");

    const cardId = e.dataTransfer.getData("cardId");
    const card = document.querySelector(`.card[data-id='${cardId}']`);
    if (!card) return;

    const isProxy = card.dataset.is_proxy === "true";

    if (isProxy) {
      await deleteProxyCard(card, cardId);
      return;
    }

    zone.appendChild(card);
    sortCards(zone);
    updateCardCounts();
  });
}

// Delete proxy if dropped outside any zone
document.addEventListener("drop", async e => {
  const cardId = e.dataTransfer.getData("cardId");
  if (!cardId) return;

  const card = document.querySelector(`.card[data-id='${cardId}']`);
  if (!card) return;

  const isProxy = card.dataset.is_proxy === "true";
  const droppedInsideZone = Object.values(zones).some(zone => zone.contains(e.target));

  if (isProxy && (!droppedInsideZone || droppedInsideZone === zones.reserve)) {
    await deleteProxyCard(card, cardId);
  }
  if (!isProxy && droppedInsideZone === zones.reserve) {
    card.remove();
    updateCardCounts();
  }
});

function enableProxyCreation(zone, zoneName) {
  zone.addEventListener("contextmenu", async e => {
    e.preventDefault();
    const name = prompt("Enter proxy card name:");
    if (!name) return;

    await backend.createProxyCard(name, zoneName);

    get_cards_in_deck();
  });
}

enableProxyCreation(zones.main, "main");
enableProxyCreation(zones.extra, "extra");
enableProxyCreation(zones.side, "side");

enableDragDrop(zones.main);
enableDragDrop(zones.extra);
enableDragDrop(zones.side);
enableDragDrop(zones.reserve);

document.addEventListener("dragover", e => e.preventDefault());

// -------------------- Card Overview --------------------
let categoriesCache = {};

export function clearCategoriesCache() {
  categoriesCache = {};
}

export async function showCardOverview(card) {
  const { name, en_name, code, type, is_proxy, price, url, id, ban_status } = card.dataset;
  const imgSrc = card.querySelector("img")?.src;
  const cardOverview = document.getElementById("card-overview");

  cardOverview.innerHTML = `
    <img src="${imgSrc}" alt="${en_name}">
    <div class="overview-text">
      <h4><a href="${url}" target="_blank" rel="noopener">${en_name}</a></h4>
      <p>Name: ${name}</p>
      <p>Type: ${type}</p>
      <p>Code: ${code}</p>
      <p>Price: ${price}€</p>
      <p>Ban Status: ${ban_status}</p>
      <p>Proxy: ${is_proxy === "true" ? "Yes" : "No"}</p>
      <div id="category-selector"><strong>Categories:</strong><br/>Loading...</div>
    </div>
  `;

  const selectorDiv = document.getElementById("category-selector");

  try {
    // ✅ Use cache if present
    if (!categoriesCache[id]) {
      const categories = await backend.getAssignedCategories(id);
      categoriesCache[id] = categories; // store in cache
    }

    // ✅ Use cached data when rendering
    selectorDiv.innerHTML = categoriesCache[id].map(cat => `
      <label>
        <input type="checkbox" data-category-id="${cat.id}" ${cat.assigned ? "checked" : ""}>
        <span>${cat.name}</span>
      </label><br/>
    `).join("");

    selectorDiv.querySelectorAll("input[type=checkbox]").forEach(input => {
      input.addEventListener("change", async (e) => {
        const categoryId = e.target.dataset.categoryId;
        const assigned = e.target.checked;
        await backend.assignCategories(id, categoryId, assigned);

        // ✅ Update cache immediately so UI stays in sync without re-fetch
        const cat = categoriesCache[id].find(c => c.id == categoryId);
        if (cat) cat.assigned = assigned;
      });
    });

  } catch (err) {
    selectorDiv.innerHTML = "Error loading categories.";
    console.error(err);
  }
}


// -------------------- Server-Side Filtering (Sorting Local) --------------------
let currentRequestController = null;

function refreshReserve(filters = {}) {
  const reserve = document.getElementById("reserve");
  if ((!filters.name || filters.name.length == 0) && !filters.code && !filters.card_type) {
      reserve.innerHTML = "Please enter at least one filter to search cards.";
      return;
  }

  if (currentRequestController) currentRequestController.abort();

  currentRequestController = new AbortController();

  backend.getDeckCardLists(
    filters,
    currentRequestController.signal
  ).then(data => {
    
    reserve.innerHTML = "";

    data.forEach(card => reserve.appendChild(renderCard(card)));

    sortCards(reserve);
  });
}
// Gather filters from the form
function getFilters() {
  return {
    name: document.getElementById("filter-name").value.toLowerCase(),
    code: document.getElementById("filter-code").value,
    card_type: document.getElementById("filter-type").value
  };
}

let debounceTimer;
function scheduleRefresh() {
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => refreshReserve(getFilters()), 250);
}

document.querySelectorAll("#filter-name, #filter-code")
      .forEach(el => el.addEventListener("input", scheduleRefresh));

document.querySelectorAll("#filter-type")
      .forEach(el => el.addEventListener("change", scheduleRefresh));