/* eslint-disable */
/* tslint:disable */
// @ts-nocheck
/*
 * ---------------------------------------------------------------
 * ## THIS FILE WAS GENERATED VIA SWAGGER-TYPESCRIPT-API        ##
 * ##                                                           ##
 * ## AUTHOR: acacode                                           ##
 * ## SOURCE: https://github.com/acacode/swagger-typescript-api ##
 * ---------------------------------------------------------------
 */

/**
 * * `main` - main
 * * `extra` - extra
 * * `side` - side
 */
export var ZoneEnum;
(function (ZoneEnum) {
  ZoneEnum["Main"] = "main";
  ZoneEnum["Extra"] = "extra";
  ZoneEnum["Side"] = "side";
})(ZoneEnum || (ZoneEnum = {}));
/**
 * * `Banned` - Banned
 * * `Limited` - Limited
 * * `Semi-Limited` - Semi-Limited
 */
export var StatusEnum;
(function (StatusEnum) {
  StatusEnum["Banned"] = "Banned";
  StatusEnum["Limited"] = "Limited";
  StatusEnum["SemiLimited"] = "Semi-Limited";
})(StatusEnum || (StatusEnum = {}));
/**
 * * `true` - true
 * * `false` - false
 */
export var SoldEnum;
(function (SoldEnum) {
  SoldEnum["True"] = "true";
  SoldEnum["False"] = "false";
})(SoldEnum || (SoldEnum = {}));
/**
 * * `true` - true
 * * `false` - false
 */
export var ProxyEnum;
(function (ProxyEnum) {
  ProxyEnum["True"] = "true";
  ProxyEnum["False"] = "false";
})(ProxyEnum || (ProxyEnum = {}));
/**
 * * `MT` - Mint
 * * `NM` - Near Mint
 * * `EX` - Excellent
 * * `G` - Good
 * * `P` - Poor
 */
export var LastKnownStatusEnum;
(function (LastKnownStatusEnum) {
  LastKnownStatusEnum["MT"] = "MT";
  LastKnownStatusEnum["NM"] = "NM";
  LastKnownStatusEnum["EX"] = "EX";
  LastKnownStatusEnum["G"] = "G";
  LastKnownStatusEnum["P"] = "P";
})(LastKnownStatusEnum || (LastKnownStatusEnum = {}));
export var BlankEnum;
(function (BlankEnum) {
  BlankEnum["Value"] = "";
})(BlankEnum || (BlankEnum = {}));
export var ContentType;
(function (ContentType) {
  ContentType["Json"] = "application/json";
  ContentType["JsonApi"] = "application/vnd.api+json";
  ContentType["FormData"] = "multipart/form-data";
  ContentType["UrlEncoded"] = "application/x-www-form-urlencoded";
  ContentType["Text"] = "text/plain";
})(ContentType || (ContentType = {}));
export class HttpClient {
  baseUrl = "";
  securityData = null;
  securityWorker;
  abortControllers = new Map();
  customFetch = (...fetchParams) => fetch(...fetchParams);
  baseApiParams = {
    credentials: "same-origin",
    headers: {},
    redirect: "follow",
    referrerPolicy: "no-referrer",
  };
  constructor(apiConfig = {}) {
    Object.assign(this, apiConfig);
  }
  setSecurityData = (data) => {
    this.securityData = data;
  };
  encodeQueryParam(key, value) {
    const encodedKey = encodeURIComponent(key);
    return `${encodedKey}=${encodeURIComponent(typeof value === "number" ? value : `${value}`)}`;
  }
  addQueryParam(query, key) {
    return this.encodeQueryParam(key, query[key]);
  }
  addArrayQueryParam(query, key) {
    const value = query[key];
    return value.map((v) => this.encodeQueryParam(key, v)).join("&");
  }
  toQueryString(rawQuery) {
    const query = rawQuery || {};
    const keys = Object.keys(query).filter(
      (key) => "undefined" !== typeof query[key],
    );
    return keys
      .map((key) =>
        Array.isArray(query[key])
          ? this.addArrayQueryParam(query, key)
          : this.addQueryParam(query, key),
      )
      .join("&");
  }
  addQueryParams(rawQuery) {
    const queryString = this.toQueryString(rawQuery);
    return queryString ? `?${queryString}` : "";
  }
  contentFormatters = {
    [ContentType.Json]: (input) =>
      input !== null && (typeof input === "object" || typeof input === "string")
        ? JSON.stringify(input)
        : input,
    [ContentType.JsonApi]: (input) =>
      input !== null && (typeof input === "object" || typeof input === "string")
        ? JSON.stringify(input)
        : input,
    [ContentType.Text]: (input) =>
      input !== null && typeof input !== "string"
        ? JSON.stringify(input)
        : input,
    [ContentType.FormData]: (input) => {
      if (input instanceof FormData) {
        return input;
      }
      return Object.keys(input || {}).reduce((formData, key) => {
        const property = input[key];
        formData.append(
          key,
          property instanceof Blob
            ? property
            : typeof property === "object" && property !== null
              ? JSON.stringify(property)
              : `${property}`,
        );
        return formData;
      }, new FormData());
    },
    [ContentType.UrlEncoded]: (input) => this.toQueryString(input),
  };
  mergeRequestParams(params1, params2) {
    return {
      ...this.baseApiParams,
      ...params1,
      ...(params2 || {}),
      headers: {
        ...(this.baseApiParams.headers || {}),
        ...(params1.headers || {}),
        ...((params2 && params2.headers) || {}),
      },
    };
  }
  createAbortSignal = (cancelToken) => {
    if (this.abortControllers.has(cancelToken)) {
      const abortController = this.abortControllers.get(cancelToken);
      if (abortController) {
        return abortController.signal;
      }
      return void 0;
    }
    const abortController = new AbortController();
    this.abortControllers.set(cancelToken, abortController);
    return abortController.signal;
  };
  abortRequest = (cancelToken) => {
    const abortController = this.abortControllers.get(cancelToken);
    if (abortController) {
      abortController.abort();
      this.abortControllers.delete(cancelToken);
    }
  };
  request = async ({
    body,
    secure,
    path,
    type,
    query,
    format,
    baseUrl,
    cancelToken,
    ...params
  }) => {
    const secureParams =
      ((typeof secure === "boolean" ? secure : this.baseApiParams.secure) &&
        this.securityWorker &&
        (await this.securityWorker(this.securityData))) ||
      {};
    const requestParams = this.mergeRequestParams(params, secureParams);
    const queryString = query && this.toQueryString(query);
    const payloadFormatter = this.contentFormatters[type || ContentType.Json];
    const responseFormat = format || requestParams.format;
    return this.customFetch(
      `${baseUrl || this.baseUrl || ""}${path}${queryString ? `?${queryString}` : ""}`,
      {
        ...requestParams,
        headers: {
          ...(requestParams.headers || {}),
          ...(type && type !== ContentType.FormData
            ? { "Content-Type": type }
            : {}),
        },
        signal:
          (cancelToken
            ? this.createAbortSignal(cancelToken)
            : requestParams.signal) || null,
        body:
          typeof body === "undefined" || body === null
            ? null
            : payloadFormatter(body),
      },
    ).then(async (response) => {
      const r = response;
      r.data = null;
      r.error = null;
      const responseToParse = responseFormat ? response.clone() : response;
      const data = !responseFormat
        ? r
        : await responseToParse[responseFormat]()
            .then((data) => {
              if (r.ok) {
                r.data = data;
              } else {
                r.error = data;
              }
              return r;
            })
            .catch((e) => {
              r.error = e;
              return r;
            });
      if (cancelToken) {
        this.abortControllers.delete(cancelToken);
      }
      if (!response.ok) throw data;
      return data;
    });
  };
}
/**
 * @title No title
 * @version 0.0.0
 */
export class Api extends HttpClient {
  myYgoCards = {
    /**
     * @description GET, PATCH, DELETE deck version
     *
     * @tags deck_versions
     * @name DeckVersionsRetrieve
     * @request GET:/my_ygo_cards/api/deck_versions/{deck_version_id}/
     * @secure
     */
    deckVersionsRetrieve: (deckVersionId, params = {}) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/`,
        method: "GET",
        secure: true,
        format: "json",
        ...params,
      }),
    /**
     * @description GET, PATCH, DELETE deck version
     *
     * @tags deck_versions
     * @name DeckVersionsPartialUpdate
     * @request PATCH:/my_ygo_cards/api/deck_versions/{deck_version_id}/
     * @secure
     */
    deckVersionsPartialUpdate: (deckVersionId, data, params = {}) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/`,
        method: "PATCH",
        body: data,
        secure: true,
        type: ContentType.Json,
        format: "json",
        ...params,
      }),
    /**
     * @description GET, PATCH, DELETE deck version
     *
     * @tags deck_versions
     * @name DeckVersionsDestroy
     * @request DELETE:/my_ygo_cards/api/deck_versions/{deck_version_id}/
     * @secure
     */
    deckVersionsDestroy: (deckVersionId, params = {}) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/`,
        method: "DELETE",
        secure: true,
        ...params,
      }),
    /**
     * @description GET  -> return all categories for a card in a deck, marking assigned ones POST -> assign/unassign a card to a category
     *
     * @tags deck_versions
     * @name DeckVersionsAssignCategoriesRetrieve
     * @request GET:/my_ygo_cards/api/deck_versions/{deck_version_id}/assign_categories/{card_id}/
     * @secure
     */
    deckVersionsAssignCategoriesRetrieve: (
      cardId,
      deckVersionId,
      params = {},
    ) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/assign_categories/${cardId}/`,
        method: "GET",
        secure: true,
        format: "json",
        ...params,
      }),
    /**
     * @description JSON payload: { category_id, assigned: true/false }
     *
     * @tags deck_versions
     * @name DeckVersionsAssignCategoriesCreate
     * @request POST:/my_ygo_cards/api/deck_versions/{deck_version_id}/assign_categories/{card_id}/
     * @secure
     */
    deckVersionsAssignCategoriesCreate: (
      cardId,
      deckVersionId,
      data,
      params = {},
    ) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/assign_categories/${cardId}/`,
        method: "POST",
        body: data,
        secure: true,
        type: ContentType.Json,
        format: "json",
        ...params,
      }),
    /**
     * @description POST /api/deck_versions/{deck_version_id}/card_lists/ Filters cards using filter_cards_queryset. Optional query params: - deck_id: exclude cards already in this deck - name, card_type, proxy, sold, code, status - limit: limit number of results
     *
     * @tags deck_versions
     * @name DeckVersionsCardListsCreate
     * @request POST:/my_ygo_cards/api/deck_versions/{deck_version_id}/card_lists/
     * @secure
     */
    deckVersionsCardListsCreate: (deckVersionId, data, params = {}) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/card_lists/`,
        method: "POST",
        body: data,
        secure: true,
        type: ContentType.Json,
        format: "json",
        ...params,
      }),
    /**
     * @description GET  -> List all categories for a deck POST -> Create a new category
     *
     * @tags deck_versions
     * @name DeckVersionsCategoriesList
     * @request GET:/my_ygo_cards/api/deck_versions/{deck_version_id}/categories/
     * @secure
     */
    deckVersionsCategoriesList: (deckVersionId, params = {}) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/categories/`,
        method: "GET",
        secure: true,
        format: "json",
        ...params,
      }),
    /**
     * @description GET  -> List all categories for a deck POST -> Create a new category
     *
     * @tags deck_versions
     * @name DeckVersionsCategoriesCreate
     * @request POST:/my_ygo_cards/api/deck_versions/{deck_version_id}/categories/
     * @secure
     */
    deckVersionsCategoriesCreate: (deckVersionId, data, params = {}) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/categories/`,
        method: "POST",
        body: data,
        secure: true,
        type: ContentType.Json,
        format: "json",
        ...params,
      }),
    /**
     * @description DELETE -> remove category + its assignments
     *
     * @tags deck_versions
     * @name DeckVersionsCategoriesDeleteDestroy
     * @request DELETE:/my_ygo_cards/api/deck_versions/{deck_version_id}/categories/delete/{category_id}/
     * @secure
     */
    deckVersionsCategoriesDeleteDestroy: (
      categoryId,
      deckVersionId,
      params = {},
    ) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/categories/delete/${categoryId}/`,
        method: "DELETE",
        secure: true,
        ...params,
      }),
    /**
     * @description POST /api/deck_versions/{deck_version_id}/clone/
     *
     * @tags deck_versions
     * @name DeckVersionsCloneCreate
     * @request POST:/my_ygo_cards/api/deck_versions/{deck_version_id}/clone/
     * @secure
     */
    deckVersionsCloneCreate: (deckVersionId, data, params = {}) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/clone/`,
        method: "POST",
        body: data,
        secure: true,
        type: ContentType.Json,
        format: "json",
        ...params,
      }),
    /**
     * @description POST /api/deck_versions/{deck_version_id}/monte_carlos/ Runs Monte Carlo simulations on the deck version.
     *
     * @tags deck_versions
     * @name DeckVersionsMonteCarlosCreate
     * @request POST:/my_ygo_cards/api/deck_versions/{deck_version_id}/monte_carlos/
     * @secure
     */
    deckVersionsMonteCarlosCreate: (deckVersionId, data, params = {}) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/monte_carlos/`,
        method: "POST",
        body: data,
        secure: true,
        type: ContentType.Json,
        format: "json",
        ...params,
      }),
    /**
     * @description POST /api/deck_versions/{deck_version_id}/proxy/
     *
     * @tags deck_versions
     * @name DeckVersionsProxyCreate
     * @request POST:/my_ygo_cards/api/deck_versions/{deck_version_id}/proxy/
     * @secure
     */
    deckVersionsProxyCreate: (deckVersionId, data, params = {}) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/proxy/`,
        method: "POST",
        body: data,
        secure: true,
        type: ContentType.Json,
        format: "json",
        ...params,
      }),
    /**
     * @description DELETE /api/deck_versions/{deck_version_id}/proxy/{card_id}/
     *
     * @tags deck_versions
     * @name DeckVersionsProxyDestroy
     * @request DELETE:/my_ygo_cards/api/deck_versions/{deck_version_id}/proxy/{card_id}/
     * @secure
     */
    deckVersionsProxyDestroy: (cardId, deckVersionId, params = {}) =>
      this.request({
        path: `/my_ygo_cards/api/deck_versions/${deckVersionId}/proxy/${cardId}/`,
        method: "DELETE",
        secure: true,
        ...params,
      }),
    /**
     * @description POST /api/decks/{deck_id}/import_ydke/ Creates a new DeckVersion from a YDKE URL.
     *
     * @tags decks
     * @name DecksImportYdkeCreate
     * @request POST:/my_ygo_cards/api/decks/{deck_id}/import_ydke/
     * @secure
     */
    decksImportYdkeCreate: (deckId, data, params = {}) =>
      this.request({
        path: `/my_ygo_cards/api/decks/${deckId}/import_ydke/`,
        method: "POST",
        body: data,
        secure: true,
        type: ContentType.Json,
        format: "json",
        ...params,
      }),
    /**
     * @description OpenApi3 schema for this API. Format can be selected via content negotiation. - YAML: application/vnd.oai.openapi - JSON: application/vnd.oai.openapi+json
     *
     * @tags schema
     * @name SchemaRetrieve
     * @request GET:/my_ygo_cards/api/schema/
     * @secure
     */
    schemaRetrieve: (query, params = {}) =>
      this.request({
        path: `/my_ygo_cards/api/schema/`,
        method: "GET",
        query: query,
        secure: true,
        format: "json",
        ...params,
      }),
  };
}
