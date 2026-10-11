/** Ordered parameter pairs: repeated keys remain repeated. */
export type QueryEntry = readonly [string, string | number | boolean];

export interface SearchRequestOptions extends Omit<RequestInit, "method" | "body"> {
  /** Deployment-specific conservative threshold for the complete encoded URL. */
  getUrlBytes?: number;
  /** Match the server's effective query limits. POST uses the same budgets. */
  maxQueryBytes?: number;
  maxRawItems?: number;
  baseUrl?: string;
  fetcher?: typeof fetch;
}

export type QueryValue = string | number | boolean;

/** Server metadata used to keep a builder aligned with selectable/filterable fields. */
export interface QueryFieldMetadata {
  selectable?: boolean;
  filterable?: boolean;
  sortable?: boolean;
  operators?: readonly string[];
}

export interface QueryMetadata<F extends string = string> {
  fields: Record<F, QueryFieldMetadata>;
}

/**
 * Typed, metadata-driven query builder. It only emits the existing
 * RawParams protocol, so callers can use it with GET or POST transport.
 */
export class QueryBuilder<F extends string = string> {
  private readonly entries: QueryEntry[] = [];
  private readonly metadata?: QueryMetadata<F>;

  constructor(metadata?: QueryMetadata<F>) {
    this.metadata = metadata;
  }

  where(field: F, value: QueryValue, operator = "eq"): this {
    this.assertField(field, "filterable");
    this.assertOperator(field, operator);
    this.entries.push([field, value]);
    if (operator !== "eq") this.entries.push([`${field}-op`, operator]);
    return this;
  }

  whereIn(field: F, values: readonly QueryValue[]): this {
    if (values.length === 0) throw new RangeError("whereIn requires at least one value");
    this.assertField(field, "filterable");
    this.assertOperator(field, "il");
    for (const value of values) this.entries.push([field, value]);
    this.entries.push([`${field}-op`, "il"]);
    return this;
  }

  onlySelect(...fields: readonly F[]): this {
    if (fields.length === 0) throw new RangeError("onlySelect requires at least one field");
    for (const field of fields) this.assertField(field, "selectable");
    this.setControl("onlySelect", fields.join(","));
    return this;
  }

  selectExclude(...fields: readonly F[]): this {
    if (fields.length === 0) throw new RangeError("selectExclude requires at least one field");
    for (const field of fields) this.assertField(field, "selectable");
    this.setControl("selectExclude", fields.join(","));
    return this;
  }

  orderBy(...orders: readonly `${F}:${"asc" | "desc"}`[]): this {
    if (orders.length === 0) throw new RangeError("orderBy requires at least one field");
    for (const order of orders) {
      const field = order.slice(0, order.lastIndexOf(":")) as F;
      this.assertField(field, "sortable");
    }
    this.setControl("orderBy", orders.join(","));
    return this;
  }

  page(page: number): this {
    this.setPositiveInteger("page", page);
    this.setControl("page", page);
    return this;
  }

  offset(offset: number): this {
    this.setNonNegativeInteger("offset", offset);
    this.setControl("offset", offset);
    return this;
  }

  size(size: number): this {
    this.setPositiveInteger("size", size);
    this.setControl("size", size);
    return this;
  }

  cursor(cursor: string): this {
    this.setControl("cursor", cursor);
    return this;
  }

  /** Returns a defensive copy suitable for `encodeQuery` or `search`. */
  toEntries(): QueryEntry[] {
    return this.entries.slice();
  }

  toQueryString(): string {
    return encodeQuery(this.entries);
  }

  request(endpoint: string | URL, options: SearchRequestOptions = {}): Promise<Response> {
    return search(endpoint, this.entries, options);
  }

  private setControl(key: string, value: QueryValue): void {
    for (let index = this.entries.length - 1; index >= 0; index--) {
      if (this.entries[index][0] === key) this.entries.splice(index, 1);
    }
    this.entries.push([key, value]);
  }

  private assertField(field: F, capability: keyof QueryFieldMetadata): void {
    const metadata = this.metadata?.fields[field];
    if (this.metadata && !metadata) throw new RangeError(`Unknown field: ${String(field)}`);
    if (metadata && metadata[capability] === false) {
      throw new RangeError(`Field is not ${String(capability)}: ${String(field)}`);
    }
  }

  private assertOperator(field: F, operator: string): void {
    const operators = this.metadata?.fields[field]?.operators;
    if (operators && !operators.includes(operator)) {
      throw new RangeError(`Operator is not allowed for field: ${String(field)}`);
    }
  }

  private setPositiveInteger(name: string, value: number): void {
    if (!Number.isSafeInteger(value) || value <= 0) throw new RangeError(`${name} must be positive`);
  }

  private setNonNegativeInteger(name: string, value: number): void {
    if (!Number.isSafeInteger(value) || value < 0) throw new RangeError(`${name} must be non-negative`);
  }
}

export function createQueryBuilder<F extends string>(metadata: QueryMetadata<F>): QueryBuilder<F> {
  return new QueryBuilder(metadata);
}

const encoder = new TextEncoder();

/** Stable form encoding, identical to Rust RawParams::to_query_string. */
export function encodeQuery(entries: Iterable<QueryEntry>): string {
  const pairs: string[] = [];
  const component = (value: string) => encodeURIComponent(value).replace(/[!'()*]/g,
    (character) => `%${character.charCodeAt(0).toString(16).toUpperCase()}`);
  for (const [key, value] of entries) {
    if (!key || (typeof value === "number" && !Number.isFinite(value))) {
      throw new TypeError("Invalid query parameter");
    }
    pairs.push(`${component(key)}=${component(String(value))}`);
  }
  return pairs.join("&");
}

/** Select transport before sending; errors never trigger a second request. */
export async function search(
  endpoint: string | URL,
  entries: Iterable<QueryEntry>,
  options: SearchRequestOptions = {},
): Promise<Response> {
  const {
    getUrlBytes = 2000,
    maxQueryBytes = 64 * 1024,
    maxRawItems = 128,
    baseUrl = typeof location === "undefined" ? undefined : location.href,
    fetcher = fetch,
    ...init
  } = options;
  for (const limit of [getUrlBytes, maxQueryBytes, maxRawItems]) {
    if (!Number.isSafeInteger(limit) || limit <= 0) {
      throw new RangeError("Query limits must be positive safe integers");
    }
  }
  const url = new URL(endpoint, baseUrl);
  if (url.search || url.hash) {
    throw new TypeError("Search endpoint must not contain query parameters or a fragment");
  }
  const pairs: QueryEntry[] = [];
  for (const pair of entries) {
    if (pairs.length >= maxRawItems) throw new RangeError("Query item budget exceeded");
    pairs.push(pair);
  }
  const encoded = encodeQuery(pairs);
  if (encoder.encode(encoded).length > maxQueryBytes) {
    throw new RangeError("Query byte budget exceeded; reduce the query");
  }
  const getUrl = new URL(url);
  getUrl.search = encoded;
  const headers = new Headers(init.headers);
  if (encoder.encode(getUrl.href).length <= getUrlBytes) {
    return fetcher(getUrl, { ...init, headers, method: "GET" });
  }
  headers.set("content-type", "application/x-www-form-urlencoded;charset=UTF-8");
  return fetcher(url, { ...init, headers, method: "POST", body: encoded });
}

/** Submit a streaming download without buffering the file in JS memory. */
export function downloadForm(
  endpoint: string | URL,
  entries: Iterable<QueryEntry>,
  target = "_blank",
): void {
  const url = new URL(endpoint, location.href);
  if (url.search || url.hash) throw new TypeError("Download endpoint must not contain a query or fragment");
  const form = document.createElement("form");
  form.method = "POST";
  form.action = url.href;
  form.target = target;
  form.hidden = true;
  form.rel = "noopener";
  form.acceptCharset = "UTF-8";
  for (const [key, value] of entries) {
    const input = document.createElement("input");
    input.type = "hidden";
    input.name = key;
    input.value = String(value);
    form.append(input);
  }
  document.body.append(form);
  try { form.submit(); } finally { form.remove(); }
}
