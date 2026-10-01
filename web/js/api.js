import { t } from "./i18n.js";

const DEFAULT_TIMEOUT_MS = 15_000;

// Where the API lives:
//  1. <meta name="kadi-api-base" content="…"> wins when set;
//  2. otherwise the same origin (the Rust server serves web/ itself with
//     KADI_ATLAS__WEB_DIR, or a reverse proxy puts both behind one host);
//  3. a page opened from file://, or from a separate local static server that
//     has no API behind it, talks to the dev API on 127.0.0.1:8080 (which then
//     needs KADI_ATLAS__CORS_ALLOW_ORIGIN).
const DEV_API = "http://127.0.0.1:8080/api/v1";
const CONFIGURED = document
  .querySelector('meta[name="kadi-api-base"]')
  ?.getAttribute("content")
  ?.trim()
  .replace(/\/+$/, "");
const LOCAL_PAGE = ["localhost", "127.0.0.1", "[::1]"].includes(window.location.hostname);

let apiBase =
  CONFIGURED ||
  (window.location.protocol === "file:" ? DEV_API : `${window.location.origin}/api/v1`);

export function apiBaseUrl() {
  return apiBase;
}


export class ApiError extends Error {
  constructor(message, { status = 0, code = "request_failed", details = null, url = "" } = {}) {
    super(message);

    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.details = details;
    this.url = url;
  }
}

export function isAbort(error) {
  return error instanceof ApiError && error.code === "aborted";
}


/* Backend speaks API. Frontend speaks feelings. This translates. */

async function request(path, { query = null, signal = null, timeout = DEFAULT_TIMEOUT_MS } = {}) {
  const url = buildUrl(path, query);
  const controller = new AbortController();
  const timeoutId = window.setTimeout(() => controller.abort("timeout"), timeout);
  const abortHandler = () => controller.abort(signal?.reason);

  if (signal) {
    if (signal.aborted) controller.abort(signal.reason);
    else signal.addEventListener("abort", abortHandler, { once: true });
  }

  try {
    const response = await fetch(url, {
      headers: { Accept: "application/json" },
      cache: "no-store",
      signal: controller.signal
    });

    if (!response.ok) throw await responseError(response, url);
    if (response.status === 204) return null;

    const contentType = response.headers.get("content-type") ?? "";

    if (!contentType.includes("application/json")) {
      throw new ApiError(t("errors.invalidResponse"), {
        status: response.status,
        code: "invalid_response",
        url
      });
    }

    return await response.json();
  } catch (error) {
    if (error instanceof ApiError) throw error;

    if (controller.signal.aborted) {
      const timedOut = controller.signal.reason === "timeout";

      throw new ApiError(timedOut ? t("errors.timeout") : t("errors.aborted"), {
        code: timedOut ? "timeout" : "aborted",
        url
      });
    }

    throw new ApiError(t("errors.network"), { code: "network_error", details: error, url });
  } finally {
    window.clearTimeout(timeoutId);
    signal?.removeEventListener("abort", abortHandler);
  }
}


export async function getMeta(options = {}) {
  let data;

  try {
    data = await request("/meta", options);
  } catch (error) {
    // a local static server without the API: try the dev API once
    const noApiHere = error.status === 404 || error.code === "invalid_response";
    if (CONFIGURED || !LOCAL_PAGE || apiBase === DEV_API || !noApiHere) throw error;

    apiBase = DEV_API;
    data = await request("/meta", options);
  }

  return {
    records: data?.counts?.appointments ?? 0,
    sources: data?.counts?.sources ?? 0,
    persons: data?.counts?.persons ?? 0,
    places: data?.counts?.places ?? 0,
    places_with_coordinates: data?.counts?.places_with_coordinates ?? 0,
    min_year: data?.year_numeric_range?.min ?? null,
    max_year: data?.year_numeric_range?.max ?? null,
    calendars: data?.calendars ?? [],
    degrees: data?.degrees ?? [],
    position_types: data?.position_types ?? []
  };
}

export async function searchPersons(query, options = {}) {
  const q = String(query ?? "").trim();
  if (!q) return [];

  const data = await request("/persons", { ...options, query: { q, limit: 20 } });
  return items(data).map(normalizePersonSummary);
}

export async function getPerson(id, options = {}) {
  return normalizePerson(await request(`/persons/${encodeId(id)}`, options));
}

export async function searchPlaces(query, options = {}) {
  const q = String(query ?? "").trim();
  if (!q) return [];

  const data = await request("/places", { ...options, query: { q, limit: 20 } });
  return items(data).map(normalizePlace);
}

export async function getPlace(id, options = {}) {
  return normalizePlace(await request(`/places/${encodeId(id)}`, options));
}

export async function getAppointments(filters = {}, options = {}) {
  const data = await request("/appointments", {
    ...options,
    query: {
      person: filters.person_id,
      place: filters.place_id,
      origin_place: filters.origin_place_id,
      destination_place: filters.destination_place_id,
      source_record_id: filters.source_record_id,
      year_from: filters.year_from,
      year_to: filters.year_to,
      degree: filters.degree,
      position_type: filters.position_type,
      limit: clampLimit(filters.limit, 200),
      offset: filters.offset ?? 0
    }
  });

  return page(data, normalizeAppointment);
}

export async function getFlows(filters = {}, options = {}) {
  const data = await request("/flows", {
    ...options,
    query: {
      person: filters.person_id,
      place: filters.place_id,
      year_from: filters.year_from,
      year_to: filters.year_to,
      degree: filters.degree,
      position_type: filters.position_type,
      require_coordinates: true,
      limit: clampLimit(filters.limit, 200)
    }
  });

  return page(data, normalizeFlow);
}

export async function getPlaceActivity(filters = {}, options = {}) {
  const data = await request("/place-activity", {
    ...options,
    query: {
      person: filters.person_id,
      place: filters.place_id,
      year_from: filters.year_from,
      year_to: filters.year_to,
      degree: filters.degree,
      position_type: filters.position_type,
      limit: clampLimit(filters.limit, 5000)
    }
  });

  return page(data, (place) => ({
    id: place.id,
    name: place.canonical_name ?? "",
    latitude: numberOrNull(place.latitude),
    longitude: numberOrNull(place.longitude),
    arrivals: Number(place.arrivals) || 0,
    departures: Number(place.departures) || 0
  }));
}

export async function searchSources({ q = "", limit = 30, offset = 0 } = {}, options = {}) {
  const data = await request("/sources", {
    ...options,
    query: { q: String(q).trim() || undefined, limit, offset }
  });

  return page(data, (source) => ({
    id: source.id,
    doc_id: source.doc_id ?? null,
    varak_no: source.varak_no ?? null,
    certificate: source.certificate ?? null,
    snippet: source.snippet ?? "",
    rank: source.rank ?? null
  }));
}

export async function getSource(id, options = {}) {
  const source = await request(`/sources/${encodeId(id)}`, options);

  return {
    id: source.id,
    doc_id: source.doc_id ?? null,
    varak_no: source.varak_no ?? null,
    certificate: source.certificate ?? null,
    text: source.source_text ?? "",
    raw: source.raw && typeof source.raw === "object" ? source.raw : {},
    appointment_ids: source.appointment_ids ?? []
  };
}


/* ---------- API adapters ---------- */

function items(data) {
  if (Array.isArray(data)) return data;
  return Array.isArray(data?.items) ? data.items : [];
}

function page(data, map) {
  const list = items(data).map(map);
  return { items: list, total: Number(data?.total ?? list.length) };
}

function normalizePersonSummary(person) {
  return {
    ...person,
    name: person.canonical_name ?? person.name ?? "",
    appointment_count: Number(person.appointment_count ?? 0)
  };
}

function normalizePerson(person) {
  const journey = person?.journey ?? person?.appointments ?? [];

  return {
    ...person,
    id: person?.id,
    name: person?.canonical_name ?? person?.name ?? "",
    wikidata_qid: person?.wikidata_qid ?? null,
    resolution_status: person?.resolution_status ?? null,
    appointments: Array.isArray(journey) ? journey.map(normalizeAppointment) : []
  };
}

function normalizePlace(place) {
  if (!place) return place;

  const inbound = place.stats?.inbound_appointments ?? place.inflow_count ?? 0;
  const outbound = place.stats?.outbound_appointments ?? place.outflow_count ?? 0;

  return {
    ...place,
    id: place.id,
    name: place.canonical_name ?? place.name ?? "",
    latitude: numberOrNull(place.latitude),
    longitude: numberOrNull(place.longitude),
    inbound: Number(inbound) || 0,
    outbound: Number(outbound) || 0,
    appointment_count: (Number(inbound) || 0) + (Number(outbound) || 0),
    distinct_persons: Number(place.stats?.distinct_persons ?? 0),
    historical_names: place.names ?? place.historical_names ?? [],
    related_persons: (place.related_persons ?? []).map((person) => ({
      id: person.id,
      name: person.canonical_name ?? "",
      resolution_status: person.resolution_status ?? null,
      arrived_here: Number(person.arrived_here) || 0,
      left_here: Number(person.left_here) || 0
    }))
  };
}

// One shape for appointments from /appointments and journey steps from
// /persons/{id}. `year_label` is what we show (the original string);
// `year_numeric` is only for filtering and ordering.
function normalizeAppointment(item) {
  const origin = typeof item?.origin === "object" ? item.origin : null;
  const destination = typeof item?.destination === "object" ? item.destination : null;
  const yearNumeric = numberOrNull(item?.year_numeric);

  return {
    ...item,
    id: item?.id ?? item?.appointment_id,
    source_record_id: item?.source_record_id ?? null,
    source_doc_id: item?.source_doc_id ?? null,
    year_numeric: yearNumeric,
    year_label: textOrNull(item?.year_original) ?? (yearNumeric != null ? String(yearNumeric) : null),
    degree: textOrNull(item?.degree),
    position_type: textOrNull(item?.position_type),
    salary: textOrNull(item?.salary),
    old_salary: textOrNull(item?.old_salary),
    origin_place_id: origin?.id ?? null,
    destination_place_id: destination?.id ?? null,
    origin: origin?.canonical_name || textOrNull(item?.raw_old_place),
    destination: destination?.canonical_name || textOrNull(item?.raw_new_place),
    origin_latitude: numberOrNull(origin?.latitude),
    origin_longitude: numberOrNull(origin?.longitude),
    destination_latitude: numberOrNull(destination?.latitude),
    destination_longitude: numberOrNull(destination?.longitude),
    old_person: item?.old_person
      ? { id: item.old_person.id, name: item.old_person.canonical_name ?? "" }
      : null,
    new_person: item?.new_person
      ? { id: item.new_person.id, name: item.new_person.canonical_name ?? "" }
      : null
  };
}

function normalizeFlow(flow) {
  return {
    origin_id: flow?.origin?.id ?? null,
    origin: flow?.origin?.canonical_name ?? "",
    origin_latitude: numberOrNull(flow?.origin?.latitude),
    origin_longitude: numberOrNull(flow?.origin?.longitude),
    destination_id: flow?.destination?.id ?? null,
    destination: flow?.destination?.canonical_name ?? "",
    destination_latitude: numberOrNull(flow?.destination?.latitude),
    destination_longitude: numberOrNull(flow?.destination?.longitude),
    count: Number(flow?.count ?? 0)
  };
}

function numberOrNull(value) {
  if (value === null || value === undefined || value === "") return null;
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function textOrNull(value) {
  if (value === null || value === undefined) return null;
  const text = String(value).trim();
  return text ? text : null;
}

function clampLimit(value, max) {
  const number = Math.trunc(Number(value ?? max));
  return Number.isFinite(number) ? Math.min(Math.max(number, 1), max) : max;
}


/* ---------- HTTP ---------- */

function buildUrl(path, query) {
  const url = new URL(`${apiBase}${path.startsWith("/") ? path : `/${path}`}`, window.location.href);

  for (const [key, value] of Object.entries(query ?? {})) {
    appendQuery(url.searchParams, key, value);
  }

  return url.toString();
}

function appendQuery(params, key, value) {
  if (value === undefined || value === null || value === "") return;

  if (Array.isArray(value)) {
    for (const item of value) appendQuery(params, key, item);
    return;
  }

  if (typeof value === "number" && !Number.isFinite(value)) return;

  params.append(key, String(value));
}

async function responseError(response, url) {
  let payload = null;

  try {
    if ((response.headers.get("content-type") ?? "").includes("application/json")) {
      payload = await response.json();
    }
  } catch {
    // The backend chose violence.
  }

  return new ApiError(defaultErrorMessage(response.status), {
    status: response.status,
    code: payload?.error?.code ?? payload?.code ?? `http_${response.status}`,
    details: payload,
    url
  });
}

// Server messages are English developer text; users get a localized one.
function defaultErrorMessage(status) {
  switch (status) {
    case 400: return t("errors.invalidQuery");
    case 404: return t("errors.notFound");
    case 408: return t("errors.timeout");
    case 429: return t("errors.tooMany");
    case 500: return t("errors.internal");
    case 502:
    case 503:
    case 504: return t("errors.unavailable");
    default: return t("errors.requestFailed", { status });
  }
}

function encodeId(id) {
  const value = String(id ?? "").trim();
  if (!value) throw new TypeError("Resource id is required.");
  return encodeURIComponent(value);
}
