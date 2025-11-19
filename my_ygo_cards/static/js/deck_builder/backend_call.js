
/// <reference path="./types.js" />


class BackendCall {
    /**
     * @param {ApiUrls} baseUrl 
     * @param {string} csrfToken
     */
    constructor(baseUrl, csrfToken) {
        this.csrfToken = csrfToken;

        this.apiUrls = baseUrl;
    }

    // ------------------- Generic request helper -------------------
    /**
     * Smart fetch wrapper
     * @param {((...args: any[]) => string)} urlFn - function returning the URL; can accept arguments
     * @param {object} options - fetch options (method, body)
     * @param {boolean} parseJson - whether to parse JSON response (default true)
     * @param  {any[]} urlArgs - arguments to pass to urlFn
     * @param {AbortSignal|null} controllerSignal - optional AbortSignal to cancel the request
     */
    async _request(urlFn, options = {}, parseJson = true, urlArgs = [], controllerSignal = null) {
        const url = urlFn(...urlArgs);
        const method = options.method || "GET";

        const headers = {
            "Content-Type": "application/json",
            ...options.headers
        };

        // Include CSRF token automatically for unsafe methods
        if (["POST", "PATCH", "DELETE"].includes(method.toUpperCase())) {
            headers["X-CSRFToken"] = this.csrfToken;
        }

        // Stringify body if it's an object
        let body = options.body;
        if (body && typeof body === "object") {
            body = JSON.stringify(body);
        }

        try {
            const response = await fetch(url, { ...options, headers, body }, controllerSignal ? { signal: controllerSignal } : {});

            if (!response.ok) {
                let errorMsg;
                try {
                    const errorData = await response.json();
                    errorMsg = JSON.stringify(errorData);
                } catch {
                    errorMsg = response.status;
                }
                throw new Error(`Request failed: ${errorMsg}`);
            }

            return parseJson ? await response.json() : response;
        } catch (error) {
            console.error(error);
            throw error;
        }
    }

    // ------------------- Deck API methods -------------------
    getDeckVersion() {
        return this._request(this.apiUrls.deckVersionDetail);
    }

    updateDeckVersion(updateData) {
        return this._request(this.apiUrls.deckVersionDetail, {
            method: "PATCH",
            body: updateData
        });
    }

    deleteDeckVersion() {
        return this._request(this.apiUrls.deckVersionDetail, { method: "DELETE" }, false)
            .then(() => ({ message: "Deck version deleted successfully" }));
    }

    /**
     * @param {object} body
     * @param {AbortSignal} controllerSignal
     */
    getDeckCardLists(body, controllerSignal) {
        return this._request(this.apiUrls.deckVersionCardLists, {
            method: "POST",
            body: body
        }, true, [], controllerSignal);
    }

    /**
     * @param {string} newName
     */
    cloneDeckVersion(newName) {
        return this._request(this.apiUrls.deckVersionClone, {
            method: "POST",
            body: { name: newName }
        });
    }

    /**
     * @param {string} ydkeUrl
     */
    importDeckFromYdke(ydkeUrl) {
        return this._request(this.apiUrls.deckImportYdke, {
            method: "POST",
            body: { ydke_url: ydkeUrl, name: "Imported from YDKE" }
        });
    }

    // ------------------- Proxy Card API methods -------------------

    /**
     * @param {number} cardId
     */
    deleteProxyCard(cardId) {
        return this._request(this.apiUrls.deckVersionProxyDelete, {
            method: "DELETE",
        }, false, [cardId]);
    }

    /**
     * @param {string} name
     * @param {string} zone
     */
    createProxyCard(name, zone) {
        return this._request(this.apiUrls.deckVersionProxyCreate, {
            method: "POST",
            body: { 
                "name": name,
                "zone": zone
            }
        }, false);
    }

    // ------------------- Category API methods -------------------

    /**
     * @param {number} cardId
     * @param {number} category
     * @param {boolean} is_assigned
     */
    assignCategories(cardId, category, is_assigned) {
        return this._request(this.apiUrls.deckVersionAssignCategories, {
            method: "POST",
            body: { 
                category_id: category,
                assigned: is_assigned
            }
        }, true, [cardId]);
    }

    /**
     * @param {number} cardId
     */
    getAssignedCategories(cardId) {
        return this._request(this.apiUrls.deckVersionAssignCategories, {}, true, [cardId]);
    }

    getCategories() {
        return this._request(this.apiUrls.deckVersionCategories);
    }

    /**
     * @param {number} categoryId
     */
    deleteCategory(categoryId) {
        return this._request(this.apiUrls.deckVersionCategoryDelete, {
            method: "DELETE"
        }, false, [categoryId]);
    }

    createCategory(name) {
        return this._request(this.apiUrls.deckVersionCategories, {
            method: "POST",
            body: { 
                "name": name
            }
        });
    }

    // ------------------- Other API methods -------------------
    /**
     * 
     * @param {number} nb_cards_by_sim 
     * @param {number} nb_sims 
     * @returns 
     */
    getMonteCarloSimulationResult(nb_cards_by_sim, nb_sims) {
        return this._request(this.apiUrls.monteCarloSimulationUrl,
            {
                method: "POST",
                body: {
                    num_cards: nb_cards_by_sim,
                    num_simulations: nb_sims
                }
            }
        );
    }
}
