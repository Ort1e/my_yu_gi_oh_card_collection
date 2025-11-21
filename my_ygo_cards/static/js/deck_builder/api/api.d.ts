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
export declare enum ZoneEnum {
  Main = "main",
  Extra = "extra",
  Side = "side",
}
/**
 * * `Banned` - Banned
 * * `Limited` - Limited
 * * `Semi-Limited` - Semi-Limited
 */
export declare enum StatusEnum {
  Banned = "Banned",
  Limited = "Limited",
  SemiLimited = "Semi-Limited",
}
/**
 * * `true` - true
 * * `false` - false
 */
export declare enum SoldEnum {
  True = "true",
  False = "false",
}
/**
 * * `true` - true
 * * `false` - false
 */
export declare enum ProxyEnum {
  True = "true",
  False = "false",
}
export type NullEnum = null;
/**
 * * `MT` - Mint
 * * `NM` - Near Mint
 * * `EX` - Excellent
 * * `G` - Good
 * * `P` - Poor
 */
export declare enum LastKnownStatusEnum {
  MT = "MT",
  NM = "NM",
  EX = "EX",
  G = "G",
  P = "P",
}
export declare enum BlankEnum {
  Value = "",
}
export interface AdvancedBanList {
  id: number;
  /**
   * The date when this ban list became effective
   * @format date
   */
  date: string;
  entries: BanListEntry[];
}
export interface BanListEntry {
  id: number;
  card_data: CardData;
  status: StatusEnum;
}
export interface BanListEntryForCardData {
  /** @format date */
  ban_list_date: string;
  /**
   * * `Banned` - Banned
   * * `Limited` - Limited
   * * `Semi-Limited` - Semi-Limited
   */
  status: StatusEnum;
  ban_list_id: number;
}
export interface Card {
  id: number;
  /** @maxLength 255 */
  name: string;
  /** @maxLength 255 */
  en_name?: string | null;
  /** @maxLength 20 */
  code?: string | null;
  last_known_status?: LastKnownStatusEnum | BlankEnum | NullEnum | null;
  card_data: CardData;
  is_proxy: boolean;
  /** @format double */
  unite_price: number | null;
}
export interface CardCategory {
  id: number;
  /** @maxLength 255 */
  name: string;
  /** @default false */
  assigned?: boolean;
  is_conditional: boolean;
}
export interface CardCategoryAssignment {
  category_id: number;
  assigned: boolean;
}
export interface CardData {
  /** @maxLength 255 */
  en_name: string;
  ygopro_id?: number | null;
  /** @maxLength 100 */
  card_type?: string | null;
  json_data: any;
  /** Return the URL of the card image. */
  image_url: string;
  ban_statuses: BanListEntryForCardData[];
}
export interface CardFilter {
  status?: string;
  /**
   * * `true` - true
   * * `false` - false
   */
  proxy?: ProxyEnum;
  name?: string;
  card_type?: string;
  code?: string;
  /**
   * * `true` - true
   * * `false` - false
   */
  sold?: SoldEnum;
}
export interface DeckVersion {
  id: number;
  name: string;
  /** @maxLength 100 */
  version_name: string;
  main_deck: Card[];
  extra_deck: Card[];
  side_deck: Card[];
  ydke_with_proxies: string;
  ydke_without_proxies: string;
  ydke_only_proxies: string;
  ban_list: AdvancedBanList;
}
export interface DeckVersionCloneInput {
  /**
   * Name of the new cloned deck version
   * @maxLength 255
   */
  name: string;
}
export interface DeckVersionMonteCarloInput {
  /**
   * Number of Monte Carlo simulations to run
   * @min 1
   * @default 1000
   */
  num_simulations?: number;
  /**
   * Number of cards to draw per simulation
   * @min 1
   * @default 5
   */
  num_cards?: number;
}
export interface DeckVersionProxyCreateInput {
  /**
   * Name of the proxy card
   * @maxLength 255
   */
  name: string;
  /**
   * Deck zone to add the proxy card
   *
   * * `main` - main
   * * `extra` - extra
   * * `side` - side
   * @default "main"
   */
  zone?: ZoneEnum;
}
export interface DeckYdkeImport {
  ydke_url: string;
  /** @default "New Deck" */
  name?: string;
}
export interface PatchedDeckVersionUpdate {
  /** @maxLength 100 */
  version_name?: string;
  main_deck?: number[];
  extra_deck?: number[];
  side_deck?: number[];
  ban_list_id?: number | null;
}
export type QueryParamsType = Record<string | number, any>;
export type ResponseFormat = keyof Omit<Body, "body" | "bodyUsed">;
export interface FullRequestParams extends Omit<RequestInit, "body"> {
  /** set parameter to `true` for call `securityWorker` for this request */
  secure?: boolean;
  /** request path */
  path: string;
  /** content type of request body */
  type?: ContentType;
  /** query params */
  query?: QueryParamsType;
  /** format of response (i.e. response.json() -> format: "json") */
  format?: ResponseFormat;
  /** request body */
  body?: unknown;
  /** base url */
  baseUrl?: string;
  /** request cancellation token */
  cancelToken?: CancelToken;
}
export type RequestParams = Omit<
  FullRequestParams,
  "body" | "method" | "query" | "path"
>;
export interface ApiConfig<SecurityDataType = unknown> {
  baseUrl?: string;
  baseApiParams?: Omit<RequestParams, "baseUrl" | "cancelToken" | "signal">;
  securityWorker?: (
    securityData: SecurityDataType | null,
  ) => Promise<RequestParams | void> | RequestParams | void;
  customFetch?: typeof fetch;
}
export interface HttpResponse<D extends unknown, E extends unknown = unknown>
  extends Response {
  data: D;
  error: E;
}
type CancelToken = Symbol | string | number;
export declare enum ContentType {
  Json = "application/json",
  JsonApi = "application/vnd.api+json",
  FormData = "multipart/form-data",
  UrlEncoded = "application/x-www-form-urlencoded",
  Text = "text/plain",
}
export declare class HttpClient<SecurityDataType = unknown> {
  baseUrl: string;
  private securityData;
  private securityWorker?;
  private abortControllers;
  private customFetch;
  private baseApiParams;
  constructor(apiConfig?: ApiConfig<SecurityDataType>);
  setSecurityData: (data: SecurityDataType | null) => void;
  protected encodeQueryParam(key: string, value: any): string;
  protected addQueryParam(query: QueryParamsType, key: string): string;
  protected addArrayQueryParam(query: QueryParamsType, key: string): any;
  protected toQueryString(rawQuery?: QueryParamsType): string;
  protected addQueryParams(rawQuery?: QueryParamsType): string;
  private contentFormatters;
  protected mergeRequestParams(
    params1: RequestParams,
    params2?: RequestParams,
  ): RequestParams;
  protected createAbortSignal: (
    cancelToken: CancelToken,
  ) => AbortSignal | undefined;
  abortRequest: (cancelToken: CancelToken) => void;
  request: <T = any, E = any>({
    body,
    secure,
    path,
    type,
    query,
    format,
    baseUrl,
    cancelToken,
    ...params
  }: FullRequestParams) => Promise<HttpResponse<T, E>>;
}
/**
 * @title No title
 * @version 0.0.0
 */
export declare class Api<
  SecurityDataType extends unknown,
> extends HttpClient<SecurityDataType> {
  myYgoCards: {
    /**
     * @description GET, PATCH, DELETE deck version
     *
     * @tags deck_versions
     * @name DeckVersionsRetrieve
     * @request GET:/my_ygo_cards/api/deck_versions/{deck_version_id}/
     * @secure
     */
    deckVersionsRetrieve: (
      deckVersionId: number,
      params?: RequestParams,
    ) => Promise<HttpResponse<DeckVersion, any>>;
    /**
     * @description GET, PATCH, DELETE deck version
     *
     * @tags deck_versions
     * @name DeckVersionsPartialUpdate
     * @request PATCH:/my_ygo_cards/api/deck_versions/{deck_version_id}/
     * @secure
     */
    deckVersionsPartialUpdate: (
      deckVersionId: number,
      data: PatchedDeckVersionUpdate,
      params?: RequestParams,
    ) => Promise<HttpResponse<DeckVersion, any>>;
    /**
     * @description GET, PATCH, DELETE deck version
     *
     * @tags deck_versions
     * @name DeckVersionsDestroy
     * @request DELETE:/my_ygo_cards/api/deck_versions/{deck_version_id}/
     * @secure
     */
    deckVersionsDestroy: (
      deckVersionId: number,
      params?: RequestParams,
    ) => Promise<HttpResponse<void, any>>;
    /**
     * @description GET  -> return all categories for a card in a deck, marking assigned ones POST -> assign/unassign a card to a category
     *
     * @tags deck_versions
     * @name DeckVersionsAssignCategoriesRetrieve
     * @request GET:/my_ygo_cards/api/deck_versions/{deck_version_id}/assign_categories/{card_id}/
     * @secure
     */
    deckVersionsAssignCategoriesRetrieve: (
      cardId: number,
      deckVersionId: number,
      params?: RequestParams,
    ) => Promise<
      HttpResponse<
        {
          id?: number;
          name?: string;
          assigned?: boolean;
        }[],
        any
      >
    >;
    /**
     * @description JSON payload: { category_id, assigned: true/false }
     *
     * @tags deck_versions
     * @name DeckVersionsAssignCategoriesCreate
     * @request POST:/my_ygo_cards/api/deck_versions/{deck_version_id}/assign_categories/{card_id}/
     * @secure
     */
    deckVersionsAssignCategoriesCreate: (
      cardId: number,
      deckVersionId: number,
      data: CardCategoryAssignment,
      params?: RequestParams,
    ) => Promise<
      HttpResponse<
        {
          status?: string;
        },
        any
      >
    >;
    /**
     * @description POST /api/deck_versions/{deck_version_id}/card_lists/ Filters cards using filter_cards_queryset. Optional query params: - deck_id: exclude cards already in this deck - name, card_type, proxy, sold, code, status - limit: limit number of results
     *
     * @tags deck_versions
     * @name DeckVersionsCardListsCreate
     * @request POST:/my_ygo_cards/api/deck_versions/{deck_version_id}/card_lists/
     * @secure
     */
    deckVersionsCardListsCreate: (
      deckVersionId: number,
      data: CardFilter,
      params?: RequestParams,
    ) => Promise<HttpResponse<Card[], any>>;
    /**
     * @description GET  -> List all categories for a deck POST -> Create a new category
     *
     * @tags deck_versions
     * @name DeckVersionsCategoriesList
     * @request GET:/my_ygo_cards/api/deck_versions/{deck_version_id}/categories/
     * @secure
     */
    deckVersionsCategoriesList: (
      deckVersionId: number,
      params?: RequestParams,
    ) => Promise<HttpResponse<CardCategory[], any>>;
    /**
     * @description GET  -> List all categories for a deck POST -> Create a new category
     *
     * @tags deck_versions
     * @name DeckVersionsCategoriesCreate
     * @request POST:/my_ygo_cards/api/deck_versions/{deck_version_id}/categories/
     * @secure
     */
    deckVersionsCategoriesCreate: (
      deckVersionId: number,
      data: Record<string, any>,
      params?: RequestParams,
    ) => Promise<HttpResponse<CardCategory, any>>;
    /**
     * @description DELETE -> remove category + its assignments
     *
     * @tags deck_versions
     * @name DeckVersionsCategoriesDeleteDestroy
     * @request DELETE:/my_ygo_cards/api/deck_versions/{deck_version_id}/categories/delete/{category_id}/
     * @secure
     */
    deckVersionsCategoriesDeleteDestroy: (
      categoryId: number,
      deckVersionId: number,
      params?: RequestParams,
    ) => Promise<HttpResponse<void, any>>;
    /**
     * @description POST /api/deck_versions/{deck_version_id}/clone/
     *
     * @tags deck_versions
     * @name DeckVersionsCloneCreate
     * @request POST:/my_ygo_cards/api/deck_versions/{deck_version_id}/clone/
     * @secure
     */
    deckVersionsCloneCreate: (
      deckVersionId: number,
      data: DeckVersionCloneInput,
      params?: RequestParams,
    ) => Promise<
      HttpResponse<
        {
          deck_id?: number;
          deck_url?: string;
        },
        any
      >
    >;
    /**
     * @description POST /api/deck_versions/{deck_version_id}/monte_carlos/ Runs Monte Carlo simulations on the deck version.
     *
     * @tags deck_versions
     * @name DeckVersionsMonteCarlosCreate
     * @request POST:/my_ygo_cards/api/deck_versions/{deck_version_id}/monte_carlos/
     * @secure
     */
    deckVersionsMonteCarlosCreate: (
      deckVersionId: number,
      data: DeckVersionMonteCarloInput,
      params?: RequestParams,
    ) => Promise<
      HttpResponse<
        {
          status?: string;
          results?: object;
        },
        any
      >
    >;
    /**
     * @description POST /api/deck_versions/{deck_version_id}/proxy/
     *
     * @tags deck_versions
     * @name DeckVersionsProxyCreate
     * @request POST:/my_ygo_cards/api/deck_versions/{deck_version_id}/proxy/
     * @secure
     */
    deckVersionsProxyCreate: (
      deckVersionId: number,
      data: DeckVersionProxyCreateInput,
      params?: RequestParams,
    ) => Promise<HttpResponse<Card, any>>;
    /**
     * @description DELETE /api/deck_versions/{deck_version_id}/proxy/{card_id}/
     *
     * @tags deck_versions
     * @name DeckVersionsProxyDestroy
     * @request DELETE:/my_ygo_cards/api/deck_versions/{deck_version_id}/proxy/{card_id}/
     * @secure
     */
    deckVersionsProxyDestroy: (
      cardId: number,
      deckVersionId: number,
      params?: RequestParams,
    ) => Promise<HttpResponse<void, any>>;
    /**
     * @description POST /api/decks/{deck_id}/import_ydke/ Creates a new DeckVersion from a YDKE URL.
     *
     * @tags decks
     * @name DecksImportYdkeCreate
     * @request POST:/my_ygo_cards/api/decks/{deck_id}/import_ydke/
     * @secure
     */
    decksImportYdkeCreate: (
      deckId: number,
      data: DeckYdkeImport,
      params?: RequestParams,
    ) => Promise<
      HttpResponse<
        {
          status?: string;
          deck_id?: number;
          deck_url?: string;
        },
        any
      >
    >;
    /**
     * @description OpenApi3 schema for this API. Format can be selected via content negotiation. - YAML: application/vnd.oai.openapi - JSON: application/vnd.oai.openapi+json
     *
     * @tags schema
     * @name SchemaRetrieve
     * @request GET:/my_ygo_cards/api/schema/
     * @secure
     */
    schemaRetrieve: (
      query?: {
        format?: "json" | "yaml";
        lang?:
          | "af"
          | "ar"
          | "ar-dz"
          | "ast"
          | "az"
          | "be"
          | "bg"
          | "bn"
          | "br"
          | "bs"
          | "ca"
          | "ckb"
          | "cs"
          | "cy"
          | "da"
          | "de"
          | "dsb"
          | "el"
          | "en"
          | "en-au"
          | "en-gb"
          | "eo"
          | "es"
          | "es-ar"
          | "es-co"
          | "es-mx"
          | "es-ni"
          | "es-ve"
          | "et"
          | "eu"
          | "fa"
          | "fi"
          | "fr"
          | "fy"
          | "ga"
          | "gd"
          | "gl"
          | "he"
          | "hi"
          | "hr"
          | "hsb"
          | "hu"
          | "hy"
          | "ia"
          | "id"
          | "ig"
          | "io"
          | "is"
          | "it"
          | "ja"
          | "ka"
          | "kab"
          | "kk"
          | "km"
          | "kn"
          | "ko"
          | "ky"
          | "lb"
          | "lt"
          | "lv"
          | "mk"
          | "ml"
          | "mn"
          | "mr"
          | "ms"
          | "my"
          | "nb"
          | "ne"
          | "nl"
          | "nn"
          | "os"
          | "pa"
          | "pl"
          | "pt"
          | "pt-br"
          | "ro"
          | "ru"
          | "sk"
          | "sl"
          | "sq"
          | "sr"
          | "sr-latn"
          | "sv"
          | "sw"
          | "ta"
          | "te"
          | "tg"
          | "th"
          | "tk"
          | "tr"
          | "tt"
          | "udm"
          | "uk"
          | "ur"
          | "uz"
          | "vi"
          | "zh-hans"
          | "zh-hant";
      },
      params?: RequestParams,
    ) => Promise<HttpResponse<Record<string, any>, any>>;
  };
}
export {};
