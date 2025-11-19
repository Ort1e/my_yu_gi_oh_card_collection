/// <reference path="./types.js" />


/** @type {BackendCall} */
const backend = window.backendCall;


// -------------------- YDKE Copy --------------------

const copyBtn = document.getElementById("copy-ydke-btn");
const copyPopup = document.getElementById("copy-popup");

// Toggle popup on button click
copyBtn.addEventListener("click", (e) => {
    e.stopPropagation(); // prevent triggering document click
    copyPopup.classList.toggle("hidden");
    const rect = copyBtn.getBoundingClientRect();
    copyPopup.style.top = rect.bottom + "px";
    copyPopup.style.left = rect.left + "px";
});

// Handle option click
copyPopup.querySelectorAll("button").forEach(btn => {
    btn.addEventListener("click", () => {
        const variant = btn.dataset.variant;
        const text = ydkeUrls[variant];
        navigator.clipboard.writeText(text).then(() => {
            copyBtn.textContent = "Copied " + btn.textContent + "!";
            setTimeout(() => (copyBtn.textContent = "Copy YDKE"), 2000);
            copyPopup.classList.add("hidden");
        }).catch(err => alert("Failed to copy: " + err));
    });
});

// Hide popup if clicking outside
document.addEventListener("click", () => {
    copyPopup.classList.add("hidden");
});

// -------------------- YDKE Import --------------------
document.getElementById("import-ydke-btn").addEventListener("click", async () => {
    const ydkeUrl = prompt("Paste your YDKE string:");
    if (!ydkeUrl) return;

    backend.importDeckFromYdke(ydkeUrl)
    .then((response) => {
        if (response.error) {
            alert("Error: " + response.error);
            return;
        }
        alert("Deck imported successfully!");
        window.location.href = response.deck_url; // redirect to new deck view
    });
});
