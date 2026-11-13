import {
  countPlacesWithPrefix,
  getAppointmentIds,
  getAppointments,
  getFlows,
  getMeta,
  getPerson,
  getPlace,
  getPlaceActivity,
  getSession,
  getSource,
  getStats,
  getSummary,
  isAbort,
  searchPersons,
  searchPlaces,
  searchSources
} from "./js/api.js";

import { MapView } from "./js/map.js";
import { getLocale, t } from "./js/i18n.js";
import { MAX_ROUTES, pickHue, routeColor } from "./js/colors.js";
import { formatDate, formatYear, getDateMode, gregorianRangeLabel, setDateMode } from "./js/calendar.js";
import { buildItinerary, hasPoint } from "./js/itinerary.js";
import { createQueryView } from "./js/query.js";
import { $, $$, el, formatNumber } from "./js/dom.js";

const VIEWS = ["map", "query", "sources"];
const VIEW_PREF_KEY = "kadi-atlas-user-view";
const PLACE_PERSONS_LIMIT = 100;

const FLOW_LIMIT = 200;
const SOURCE_PAGE = 30;
const PLAY_FRAMES = 60;
const PLAY_DELAY_MS = 450;
const LOADING_DELAY_MS = 220;

const drawerQuery = window.matchMedia("(max-width: 1000px)");

// module-level timers live up here: bootstrap() runs before the code below
let cursorTimer = 0;
let loadingTimer = 0;

const els = {
  datasetStatus: $("#dataset-status"),
  datasetStatusLabel: $("#dataset-status-label"),
  navItems: $$(".nav-item[data-view]"),
  navSources: $("#nav-sources"),
  viewToggle: $("#view-toggle"),
  accessLink: $("#access-link"),
  aboutButton: $('[data-action="about"]'),
  aboutDialog: $("#about-dialog"),
  aboutClose: $("#about-close"),

  personSearch: $("#person-search"),
  personResults: $("#person-results"),
  personChips: $("#person-chips"),
  personHint: $("#person-hint"),
  clearPersons: $("#clear-persons"),

  yearFrom: $("#year-from"),
  yearTo: $("#year-to"),
  yearError: $("#year-error"),
  timelineMin: $("#timeline-min"),
  timelineMax: $("#timeline-max"),
  timelineRange: $("#timeline-range"),
  yearGregorian: $("#year-gregorian"),
  dateModeButtons: $$("[data-date-mode]"),

  degreeFilter: $("#degree-filter"),
  positionFilter: $("#position-filter"),
  placeSearch: $("#place-search"),
  placeResults: $("#place-results"),
  placeFilter: $("#place-filter"),
  placeFilterName: $("#place-filter-name"),
  placeFilterClear: $("#place-filter-clear"),
  directionFilter: $("#direction-filter"),
  resetFilters: $("#reset-filters"),

  metricRecords: $("#metric-records"),
  metricPersons: $("#metric-persons"),
  metricPlaces: $("#metric-places"),
  metricGeocoded: $("#metric-geocoded"),

  mapView: $("#map-view"),
  map: $("#map"),
  mapEyebrow: $("#map-eyebrow"),
  mapTitle: $("#map-title"),
  mapSubtitle: $("#map-subtitle"),
  mapCoverage: $("#map-coverage"),
  resultCount: $("#result-count"),
  mapLoading: $("#map-loading"),
  mapEmpty: $("#map-empty"),
  mapEmptyTitle: $("#map-empty-title"),
  mapEmptyText: $("#map-empty-text"),
  mapRetry: $("#map-retry"),
  mapLegend: $("#map-legend"),
  mapModeButtons: $$("[data-map-mode]"),
  zoomIn: $("#zoom-in"),
  zoomOut: $("#zoom-out"),
  fitMap: $("#fit-map"),

  timelineSlider: $("#timeline-slider"),
  timelineCurrent: $("#timeline-current"),
  timelinePlay: $("#timeline-play"),
  timelineAll: $("#timeline-all"),

  queryView: $("#query-view"),
  sourcesView: $("#sources-view"),
  sourcesForm: $("#sources-form"),
  sourcesQuery: $("#sources-query"),
  sourcesList: $("#sources-list"),
  sourcesEmpty: $("#sources-empty"),
  sourcesMore: $("#sources-more"),
  sourcesCount: $("#sources-count"),

  details: $("#details-panel"),
  detailsFab: $("#details-fab"),
  closeDetails: $("#close-details"),
  detailBack: $("#detail-back"),
  detailEyebrow: $("#detail-eyebrow"),
  detailTitle: $("#detail-title"),
  detailSubtitle: $("#detail-subtitle"),
  detailSummary: $("#detail-summary"),
  detailActions: $("#detail-actions"),
  detailTabs: $("#detail-tabs"),
  detailBody: $("#detail-body"),

  toastRegion: $("#toast-region")
};

const state = {
  meta: null,
  status: "loading",
  view: "map",
  mapMode: "flows",

  // applied filters
  yearFrom: null,
  yearTo: null,
  degree: "",
  positionType: "",
  // { id, name, hasCoordinates } for one place, or { prefix, name } for every
  // place whose name starts with `prefix`
  place: null,
  direction: "", // "" | "arrival" | "departure", only with a place

  // { role: "admin" | "user", login_enabled }; `userView` lets an admin
  // preview what visitors see
  session: { role: "admin", login_enabled: false },
  userView: readUserViewPreference(),

  // how many of the selected records the map can draw
  summary: null,

  // timeline cursor: null = the whole selected range
  cursor: null,
  playing: false,
  playTimer: 0,

  // every kadı drawn on the map: [{ person, hue, matched }]; `matched` is the
  // Set of this kadı's appointment ids that pass the sidebar filters (null
  // when no step filter is active)
  persons: [],
  activePersonId: null,

  // details panel: what is shown, which tab is open, and the record (source
  // row + appointment) that can sit in an extra tab next to it
  detail: { kind: "overview" },
  tab: null,
  record: null,

  mapRequest: 0,
  mapController: null,
  lastCount: null,
  detailRequest: 0,
  recordRequest: 0,
  lastHash: null,
  matchRequest: 0,

  // browser history: position of the current entry, and whether the state is
  // being restored from it (then nothing new is pushed)
  historyIndex: 0,
  restoring: false,

  sources: {
    q: "",
    items: [],
    total: 0,
    offset: 0,
    loading: false,
    loaded: false,
    error: null,
    request: 0,
    controller: null
  }
};

const map = new MapView(els.map, { onSelect: handleMapSelection });

setupCombobox({
  input: els.personSearch,
  list: els.personResults,
  search: (query, signal) => searchPersons(query, { signal }),
  describe: (person) => ({
    label: person.name,
    meta: t("appointments.count", { count: formatNumber(person.appointment_count) }),
    added: state.persons.some((entry) => entry.person.id === person.id)
  }),
  onPick: (person) => addPerson(person.id),
  errorMessage: () => t("errors.personSearch")
});

setupCombobox({
  input: els.placeSearch,
  list: els.placeResults,
  // the first option selects every place whose name starts with the text
  // ("Misis" → "Misis", "Misis maa Ayaş", …) when there is more than one
  search: async (query, signal) => {
    const [places, prefixCount] = await Promise.all([
      searchPlaces(query, { signal }),
      countPlacesWithPrefix(query, { signal })
    ]);
    const items = places.map((place) => ({ kind: "place", ...place }));
    if (prefixCount > 1) items.unshift({ kind: "group", prefix: query, count: prefixCount });
    return items;
  },
  describe: (item) =>
    item.kind === "group"
      ? {
          label: t("filters.prefixGroup", { prefix: item.prefix }),
          meta: t("filters.prefixGroupCount", { count: formatNumber(item.count) }),
          group: true
        }
      : {
          label: item.name,
          meta: t("appointments.count", { count: formatNumber(item.appointment_count) }),
          note: item.latitude == null ? t("common.noCoordinates") : ""
        },
  onPick: (item) =>
    pickPlace(
      item.kind === "group"
        ? { prefix: item.prefix, name: item.prefix }
        : { id: item.id, name: item.name, hasCoordinates: item.latitude != null }
    ),
  errorMessage: () => t("errors.placeSearch")
});

const queryView = createQueryView({
  getFilters: () => tableFilters(),
  describeFilters: () => describeFilters({ withPersons: true }),
  onRecord: (appointment) => openRecord({ appointment }),
  onPlace: (id) => showPlace(id),
  onPerson: (id) => addPerson(id, { stay: true }),
  onPeriod: (from, to) => {
    state.yearFrom = Math.max(from, state.meta?.min_year ?? from);
    state.yearTo = Math.min(to, state.meta?.max_year ?? to);
    syncFilterInputs();
    filtersChanged();
  },
  onChange: () => writeHash()
});

bindEvents();
renderDetail();
bootstrap();

/* The DOM is now emotionally dependent on this function. */

async function bootstrap() {
  setStatus("loading");
  setMapEmpty(null);
  setMapLoading(true);

  try {
    const [meta, session] = await Promise.all([
      getMeta(),
      getSession().catch((error) => {
        console.error(error);
        return { role: "admin", login_enabled: false };
      })
    ]);
    state.meta = meta;
    state.session = session;
  } catch (error) {
    console.error(error);
    setStatus("error");
    setMapLoading(false);
    setMapEmpty({
      title: t("errors.dataUnavailableTitle"),
      text: t("errors.dataUnavailableText"),
      retry: bootstrap
    });
    return;
  }

  setStatus("ready");
  renderMeta();
  renderAccess();
  renderDateMode();

  state.historyIndex = Number(history.state?.kadiIndex) || 0;
  history.replaceState({ kadiIndex: state.historyIndex }, "", location.href);
  updateBackButton();

  await restoreAndRender();
}

function bindEvents() {
  els.clearPersons.addEventListener("click", clearPersons);

  els.yearFrom.addEventListener("change", applyYearInputs);
  els.yearTo.addEventListener("change", applyYearInputs);

  els.degreeFilter.addEventListener("change", () => {
    state.degree = els.degreeFilter.value;
    filtersChanged();
  });

  els.positionFilter.addEventListener("change", () => {
    state.positionType = els.positionFilter.value;
    filtersChanged();
  });

  els.placeFilterClear.addEventListener("click", () => {
    setPlaceFilter(null);
    if (state.detail.kind === "place" || state.detail.kind === "placeGroup") renderDetail();
  });

  els.directionFilter.addEventListener("change", () => {
    state.direction = els.directionFilter.value;
    filtersChanged();
  });

  for (const button of els.dateModeButtons) {
    button.addEventListener("click", () => setDateMode(button.dataset.dateMode));
  }
  window.addEventListener("kadi:datemodechange", onDateModeChange);

  els.viewToggle.addEventListener("click", () => {
    state.userView = !state.userView;
    try {
      localStorage.setItem(VIEW_PREF_KEY, state.userView ? "1" : "0");
    } catch {
      // the preview then lasts for this visit only
    }
    renderAccess();
    renderDetail();
  });

  els.detailBack.addEventListener("click", () => history.back());

  els.resetFilters.addEventListener("click", resetFilters);

  for (const button of els.mapModeButtons) {
    button.addEventListener("click", () => setMapMode(button.dataset.mapMode));
  }

  els.zoomIn.addEventListener("click", () => map.zoomBy(1.6));
  els.zoomOut.addEventListener("click", () => map.zoomBy(1 / 1.6));
  els.fitMap.addEventListener("click", () => map.fit());

  els.timelineSlider.addEventListener("input", onTimelineInput);
  els.timelineAll.addEventListener("click", () => {
    stopPlayback();
    setCursor(null);
  });
  els.timelinePlay.addEventListener("click", togglePlayback);

  els.closeDetails.addEventListener("click", closeDetails);
  els.detailsFab.addEventListener("click", openDetails);
  drawerQuery.addEventListener("change", updateFab);

  for (const button of els.navItems) {
    button.addEventListener("click", () => setView(button.dataset.view));
  }

  els.aboutButton.addEventListener("click", () => els.aboutDialog.showModal());
  els.aboutClose.addEventListener("click", () => els.aboutDialog.close());
  els.aboutDialog.addEventListener("click", (event) => {
    // a click on the backdrop closes
    if (event.target === els.aboutDialog) els.aboutDialog.close();
  });

  els.sourcesForm.addEventListener("submit", (event) => {
    event.preventDefault();
    runSourceSearch({ reset: true });
  });

  let sourcesTimer = 0;
  els.sourcesQuery.addEventListener("input", () => {
    clearTimeout(sourcesTimer);
    sourcesTimer = setTimeout(() => runSourceSearch({ reset: true }), 320);
  });

  els.sourcesMore.addEventListener("click", () => runSourceSearch({ reset: false }));

  window.addEventListener("kadi:localechange", onLocaleChange);
  window.addEventListener("kadi:themechange", () => {
    renderPersonChips();
    renderLegend();
    if (state.detail.kind === "person") renderDetail();
  });

  // Back / Forward (and a hand-edited address) restore the whole screen:
  // filters, selected kadıs, view and the open detail all live in the hash.
  window.addEventListener("popstate", (event) => {
    if (!state.meta) return;
    state.historyIndex = Number(event.state?.kadiIndex) || 0;
    updateBackButton();
    if (location.hash === state.lastHash) return;
    restoreAndRender();
  });

  document.addEventListener("keydown", (event) => {
    const typing = event.target.closest?.("input, select, textarea, [contenteditable]");

    if (event.key === "/" && !typing && !event.metaKey && !event.ctrlKey && !els.aboutDialog.open) {
      event.preventDefault();
      setView("map");
      els.personSearch.focus();
      return;
    }

    if (event.key === "Escape" && !typing && drawerQuery.matches && els.details.classList.contains("is-open")) {
      closeDetails();
    }
  });
}

/* ---------- meta & filters ---------- */

function renderMeta() {
  const meta = state.meta;

  els.metricRecords.textContent = formatNumber(meta.records);
  els.metricPersons.textContent = formatNumber(meta.persons);
  els.metricPlaces.textContent = formatNumber(meta.places);
  els.metricGeocoded.textContent = formatNumber(meta.places_with_coordinates);

  const hasYears = meta.min_year != null && meta.max_year != null;

  els.timelineMin.textContent = hasYears ? formatYear(meta.min_year) : "—";
  els.timelineMax.textContent = hasYears ? formatYear(meta.max_year) : "—";

  for (const input of [els.yearFrom, els.yearTo]) {
    input.disabled = !hasYears;
    if (hasYears) {
      input.min = meta.min_year;
      input.max = meta.max_year;
    }
  }
}

function populateFilterOptions() {
  replaceOptions(els.degreeFilter, state.meta?.degrees ?? [], state.degree);
  replaceOptions(els.positionFilter, state.meta?.position_types ?? [], state.positionType);
  state.degree = els.degreeFilter.value;
  state.positionType = els.positionFilter.value;
}

function replaceOptions(select, values, current) {
  const collator = new Intl.Collator(getLocale() === "en" ? "en" : "tr");
  const unique = [...new Set(values.filter(Boolean))].sort(collator.compare);

  select.replaceChildren(new Option(t("common.all"), ""));
  for (const value of unique) select.append(new Option(value, value));

  // a value from a shared link that no longer exists falls back to "all"
  select.value = unique.includes(current) ? current : "";
  select.disabled = unique.length === 0;
}

function syncFilterInputs() {
  els.yearFrom.value = state.yearFrom ?? "";
  els.yearTo.value = state.yearTo ?? "";
  populateFilterOptions();
  renderPlaceFilter();

  setYearError("");
  syncTimeline();
  updateRangeVisual();
  renderYearHint();
  renderPersonChips();
  renderMapMode();
}

function renderPlaceFilter() {
  els.placeFilter.hidden = !state.place;
  els.placeFilterName.textContent = state.place ? placeLabel(state.place) : "";
  els.directionFilter.disabled = !state.place;
  els.directionFilter.value = state.place ? state.direction : "";
  els.directionFilter.title = state.place ? "" : t("filters.directionNeedsPlace");
}

// The Gregorian equivalent of the year range, when dates are shown converted.
function renderYearHint() {
  const show = getDateMode() !== "hijri" && state.yearFrom != null && state.yearTo != null;
  els.yearGregorian.hidden = !show;
  els.yearGregorian.textContent = show
    ? t("time.gregorianRange", { range: gregorianRangeLabel(state.yearFrom, state.yearTo) })
    : "";
}

function renderDateMode() {
  const mode = getDateMode();
  for (const button of els.dateModeButtons) {
    const active = button.dataset.dateMode === mode;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-pressed", String(active));
  }
}

function onDateModeChange() {
  renderDateMode();
  if (!state.meta) return;
  renderMeta();
  syncTimeline();
  renderYearHint();
  renderMapHeading();
  renderDetail();
  queryView.render();
}

function applyYearInputs() {
  const from = parseYear(els.yearFrom.value);
  const to = parseYear(els.yearTo.value);

  if (from === undefined || to === undefined) {
    setYearError(t("validation.yearInteger"));
    return;
  }

  if (from !== null && to !== null && from > to) {
    setYearError(t("validation.yearOrder"));
    return;
  }

  setYearError("");

  const nextFrom = from ?? state.meta?.min_year ?? null;
  const nextTo = to ?? state.meta?.max_year ?? null;

  els.yearFrom.value = nextFrom ?? "";
  els.yearTo.value = nextTo ?? "";

  if (nextFrom === state.yearFrom && nextTo === state.yearTo) return;

  state.yearFrom = nextFrom;
  state.yearTo = nextTo;
  filtersChanged();
}

function setYearError(message) {
  els.yearError.textContent = message;
  els.yearError.hidden = !message;
  els.yearFrom.setAttribute("aria-invalid", String(Boolean(message)));
  els.yearTo.setAttribute("aria-invalid", String(Boolean(message)));
}

// "" → null; anything that is not a plain integer → undefined (invalid)
function parseYear(value) {
  const text = String(value ?? "").trim();
  if (!text) return null;

  const number = Number(text);
  return Number.isInteger(number) && Math.abs(number) < 100000 ? number : undefined;
}

function samePlace(a, b) {
  return (a?.id ?? null) === (b?.id ?? null) && (a?.prefix ?? null) === (b?.prefix ?? null);
}

function setPlaceFilter(place) {
  if (samePlace(place, state.place)) return;

  state.place = place;
  if (!place) state.direction = "";
  els.placeSearch.value = "";
  renderPlaceFilter();
  filtersChanged();
}

// A place picked in the sidebar search. With kadıs on the map the filter
// narrows their itineraries and their panel stays open; otherwise the place
// (or the name group) opens in the panel.
function pickPlace(place) {
  setPlaceFilter(place);

  if (state.persons.length) {
    toast(t("filters.appliedToRoutes", { place: placeLabel(place) }));
    return;
  }

  if (place.id != null) showPlace(place.id);
  else showPlaceGroup();
}

function placeLabel(place) {
  if (!place) return "";
  return place.prefix ? t("filters.prefixLabel", { prefix: place.prefix }) : place.name;
}

function filtersChanged() {
  stopPlayback();
  state.cursor = null;
  syncTimeline();
  updateRangeVisual();
  renderYearHint();
  renderPlaceFilter();
  writeHash();
  refreshMap({ fit: true });
  refreshRouteMatches();
  queryView.invalidate();

  reloadDetailForFilters();
}

// Detail views whose lists depend on the filters.
function reloadDetailForFilters() {
  if (state.detail.kind === "flow") loadFlowDetail(state.detail.flow);
  if (state.detail.kind === "place" || state.detail.kind === "placeGroup") loadPlacePersons();
}

function resetFilters() {
  state.yearFrom = state.meta?.min_year ?? null;
  state.yearTo = state.meta?.max_year ?? null;
  state.degree = "";
  state.positionType = "";
  state.place = null;
  state.direction = "";
  els.placeSearch.value = "";

  syncFilterInputs();
  filtersChanged();
  renderDetail();
}

// The window that actually applies: the timeline cursor pulls in the end.
function effectiveWindow() {
  let to = state.yearTo;
  if (state.cursor != null) to = to == null ? state.cursor : Math.min(state.cursor, to);

  return { from: state.yearFrom, to };
}

function isFullRange() {
  const meta = state.meta;
  if (!meta || meta.min_year == null) return true;

  const { from, to } = effectiveWindow();
  return (from == null || from <= meta.min_year) && (to == null || to >= meta.max_year);
}

// The filters for the map's flows / places (selected kadıs are drawn as
// routes instead, so they are not part of these).
function queryFilters() {
  const { from, to } = effectiveWindow();
  const meta = state.meta;
  const filters = {};

  // only a narrowed end is sent: a year bound also drops the undated records,
  // which the full period must keep
  if (from != null && (meta?.min_year == null || from > meta.min_year)) filters.year_from = from;
  if (to != null && (meta?.max_year == null || to < meta.max_year)) filters.year_to = to;
  if (state.degree) filters.degree = state.degree;
  if (state.positionType) filters.position_type = state.positionType;

  return { ...filters, ...placeFilters() };
}

function placeFilters() {
  const filters = {};

  if (state.place?.id != null) filters.place_id = state.place.id;
  else if (state.place?.prefix) filters.place_prefix = state.place.prefix;
  if (state.place && state.direction) filters.direction = state.direction;

  return filters;
}

// The query page and charts: the same filters, plus the selected kadıs.
function tableFilters() {
  const filters = queryFilters();
  if (state.persons.length) filters.persons = state.persons.map((entry) => entry.person.id);
  return filters;
}

// Filters that pick steps out of a kadı's itinerary (the year window is
// applied on the map itself, so it is not one of them).
function stepFilters() {
  const filters = placeFilters();
  if (state.degree) filters.degree = state.degree;
  if (state.positionType) filters.position_type = state.positionType;
  return filters;
}

function hasStepFilters() {
  return Boolean(state.degree || state.positionType || state.place);
}

// "Süzgeç: 1150–1170 · Larende (gelenler) · …" for headings and notes.
function describeFilters({ withPersons = false, withPlace = true, bare = false } = {}) {
  const { from, to } = effectiveWindow();
  const parts = [];

  if (!isFullRange() && from != null && to != null) {
    parts.push(t("map.years", { from: formatYear(from), to: formatYear(to) }));
  }
  if (state.degree) parts.push(state.degree);
  if (state.positionType) parts.push(state.positionType);
  if (withPlace && state.place) {
    parts.push(state.direction
      ? t(`filters.placeWith.${state.direction}`, { place: placeLabel(state.place) })
      : placeLabel(state.place));
  }
  if (withPersons && state.persons.length) {
    parts.push(state.persons.map((entry) => entry.person.name).join(", "));
  }

  if (bare) return parts.join(" · ");
  return parts.length ? t("map.filteredBy", { filters: parts.join(" · ") }) : t("query.noFilters");
}

function hasActiveFilters() {
  return Boolean(state.degree || state.positionType || state.place || !isFullRange());
}

function updateRangeVisual() {
  const meta = state.meta;

  if (!meta || meta.min_year == null || meta.max_year === meta.min_year) {
    els.timelineRange.style.left = "0%";
    els.timelineRange.style.right = "0%";
    return;
  }

  const span = meta.max_year - meta.min_year;
  const { from, to } = effectiveWindow();
  const left = (((from ?? meta.min_year) - meta.min_year) / span) * 100;
  const right = (((to ?? meta.max_year) - meta.min_year) / span) * 100;

  els.timelineRange.style.left = `${clamp(left, 0, 100)}%`;
  els.timelineRange.style.right = `${100 - clamp(right, 0, 100)}%`;
}

/* ---------- timeline ---------- */

function syncTimeline() {
  const from = state.yearFrom;
  const to = state.yearTo;
  const usable = from != null && to != null && from < to;

  els.timelineSlider.disabled = !usable;
  els.timelinePlay.disabled = !usable;

  if (usable) {
    els.timelineSlider.min = from;
    els.timelineSlider.max = to;
    els.timelineSlider.value = state.cursor ?? to;
  }

  els.timelineAll.hidden = state.cursor == null;
  els.timelineCurrent.textContent =
    state.cursor != null
      ? t("time.until", { from: from != null ? formatYear(from) : "…", year: formatYear(state.cursor) })
      : from != null && to != null
        ? t("time.rangeLabel", { from: formatYear(from), to: formatYear(to) })
        : t("time.allPeriod");

  renderPlayButton();
}

function onTimelineInput() {
  stopPlayback();

  const year = Number(els.timelineSlider.value);
  if (!Number.isFinite(year)) return;

  setCursor(year >= state.yearTo ? null : year, { debounce: true });
}

function setCursor(year, { debounce = false } = {}) {
  state.cursor = year;
  syncTimeline();
  updateRangeVisual();
  queryView.invalidate();

  clearTimeout(cursorTimer);

  const run = () => refreshMap({ fit: false, quiet: true });

  // routes are drawn locally, so only server-backed views need a debounce
  if (debounce && !state.persons.length) {
    cursorTimer = setTimeout(run, 140);
    return Promise.resolve();
  }

  return run();
}

function togglePlayback() {
  if (state.playing) {
    stopPlayback();
    return;
  }

  const from = state.yearFrom;
  const to = state.yearTo;
  if (from == null || to == null || from >= to) return;

  const step = Math.max(1, Math.ceil((to - from) / PLAY_FRAMES));
  let year = state.cursor != null && state.cursor < to ? state.cursor : from;

  state.playing = true;
  renderPlayButton();

  const tick = async () => {
    if (!state.playing) return;

    await setCursor(year >= to ? null : year);
    if (!state.playing) return;

    if (year >= to) {
      stopPlayback();
      return;
    }

    year = Math.min(to, year + step);
    state.playTimer = setTimeout(tick, PLAY_DELAY_MS);
  };

  tick();
}

function stopPlayback() {
  state.playing = false;
  clearTimeout(state.playTimer);
  state.playTimer = 0;
  renderPlayButton();
}

function renderPlayButton() {
  els.timelinePlay.classList.toggle("is-playing", state.playing);
  els.timelinePlay.setAttribute("aria-label", state.playing ? t("timeline.stop") : t("timeline.play"));
  els.timelinePlay.setAttribute("aria-pressed", String(state.playing));
  els.timelinePlay.title = state.playing ? t("timeline.stopShort") : t("timeline.playShort");
}

/* ---------- map ---------- */

function setMapMode(mode) {
  if (!["flows", "points"].includes(mode) || mode === state.mapMode) return;

  state.mapMode = mode;
  state.lastCount = null;
  renderMapMode();
  writeHash();
  refreshMap({ fit: true });
}

function renderMapMode() {
  const routes = state.persons.length > 0;

  for (const button of els.mapModeButtons) {
    const active = button.dataset.mapMode === state.mapMode;
    button.classList.toggle("is-active", active && !routes);
    button.setAttribute("aria-pressed", String(active && !routes));
    button.disabled = routes;
    button.title = routes ? t("map.modeDisabled") : "";
  }
}

async function refreshMap({ fit = false, quiet = false } = {}) {
  const request = ++state.mapRequest;

  state.mapController?.abort();
  state.mapController = null;

  if (!state.meta) return;

  if (state.persons.length) {
    setMapLoading(false);
    drawRoutes({ fit });
    return;
  }

  const controller = new AbortController();
  state.mapController = controller;

  if (!quiet) setMapLoading(true);

  try {
    const filters = queryFilters();
    const options = { signal: controller.signal };
    const [result, summary] = await Promise.all([
      state.mapMode === "flows"
        ? getFlows({ ...filters, limit: FLOW_LIMIT }, options)
        : getPlaceActivity(filters, options),
      getSummary(filters, options)
    ]);
    if (request !== state.mapRequest) return;

    state.summary = summary;

    if (state.mapMode === "flows") {
      map.setFlows(result.items, { fit, time: fadeWindow() });
      state.lastCount = { kind: "flows", shown: result.items.length, total: result.total };
    } else {
      map.setPoints(result.items, { fit });
      state.lastCount = { kind: "points", shown: result.items.length, total: result.total };
    }
    map.setFocus(focusKey());

    renderMapHeading();
    renderLegend();
    renderCoverage();
    setMapEmpty(map.hasContent() ? null : emptyReason());
  } catch (error) {
    if (request !== state.mapRequest || isAbort(error)) return;

    console.error(error);
    map.clear();
    state.lastCount = null;
    state.summary = null;
    renderMapHeading();
    renderCoverage();
    setMapEmpty({
      title: t("errors.mapLoad"),
      text: error.message,
      retry: () => refreshMap({ fit: true })
    });
  } finally {
    if (request === state.mapRequest) setMapLoading(false);
  }
}

function focusKey() {
  return state.place?.id != null ? `place:${state.place.id}` : null;
}

// "Of the 512 records selected, 41 can be drawn": most records name a place
// without validated coordinates (or no old place at all), so the map is
// always a subset — say which.
function renderCoverage() {
  const note = els.mapCoverage;
  note.replaceChildren();

  if (state.persons.length) {
    const entry = activeEntry();
    if (!entry) {
      note.hidden = true;
      return;
    }
    const { stops } = buildItinerary(entry.person.appointments);
    const drawn = stops.filter(hasPoint).length;
    note.append(t("map.routeCoverage", { drawn: formatNumber(drawn), total: formatNumber(stops.length) }));
    if (drawn < stops.length) note.append(" ", t("map.routeCoverageHint"));

    // the filters picked steps, but none of them can be drawn
    const matched = entry.matched;
    if (matched && matched.size) {
      const hits = stops.filter((stop) => matched.has(stop.appointment.id));
      if (hits.length && !hits.some(hasPoint)) {
        note.append(" ", el("strong", {}, t("map.routeMatchNotOnMap", { count: formatNumber(matched.size) })));
      }
    }
    note.hidden = false;
    return;
  }

  const summary = state.summary;
  if (!summary) {
    note.hidden = true;
    return;
  }

  note.append(t("map.coverage", {
    mapped: formatNumber(summary.mapped),
    records: formatNumber(summary.records)
  }));

  if (summary.records > summary.mapped) {
    const link = el("button", { type: "button", class: "inline-link" }, t("actions.showTable"));
    link.addEventListener("click", () => setView("query"));
    note.append(" ", link);
  }

  note.hidden = false;
}

function emptyReason() {
  if (!state.meta?.places_with_coordinates) {
    return { title: t("map.noGeoTitle"), text: t("map.noGeoText") };
  }

  const summary = state.summary;
  const toTable = { label: t("actions.showTable"), run: () => setView("query") };

  // there are records, but none of them can be drawn
  if (summary && summary.records > 0) {
    const placeWithoutPoint = state.place?.id != null && state.place.hasCoordinates === false;
    return {
      title: t("map.noMappedTitle", { count: formatNumber(summary.records) }),
      text: placeWithoutPoint
        ? t("map.placeNoGeoText", { name: state.place.name })
        : t("map.noMappedText"),
      action: toTable
    };
  }

  if (hasActiveFilters()) {
    return {
      title: t("map.emptyTitle"),
      text: t("map.emptyText"),
      action: { label: t("actions.resetFilters"), run: resetFilters }
    };
  }

  return { title: t("map.emptyTitle"), text: t("map.noGeoText") };
}

// The years flow arrows are aged against: older arrows fade as the timeline
// cursor moves forward. Open ends fall back to the corpus' first/last year.
function fadeWindow() {
  const { from, to } = effectiveWindow();
  return {
    from: from ?? state.meta?.min_year ?? null,
    to: to ?? state.meta?.max_year ?? null
  };
}

function routeWindow() {
  const { from, to } = effectiveWindow();
  return { from, to, includeUndated: isFullRange() };
}

function drawRoutes({ fit = false } = {}) {
  map.setRoutes(
    state.persons.map(({ person, hue, matched }) => ({
      id: person.id,
      hue,
      appointments: person.appointments,
      matched: matched ?? null
    })),
    { activeId: state.activePersonId, window: routeWindow(), fit }
  );
  map.setFocus(focusKey());

  renderMapHeading();
  renderLegend();
  renderCoverage();

  const anyCoordinates = state.persons.some(({ person }) =>
    buildItinerary(person.appointments).stops.some(hasPoint));

  if (!anyCoordinates) {
    setMapEmpty({
      title: t("map.routeNoGeoTitle"),
      text: t("map.routeNoGeoText"),
      action: { label: t("actions.showTable"), run: () => setView("query") }
    });
  } else if (!map.hasContent()) {
    setMapEmpty({
      title: t("map.routeOutsideTitle"),
      text: t("map.routeOutsideText"),
      action: {
        label: t("actions.resetFilters"),
        run: resetFilters
      }
    });
  } else {
    setMapEmpty(null);
  }
}

// Mark the steps of each selected kadı that pass the sidebar filters (place,
// direction, degree, type). The filter set is the server's, so a route shows
// exactly the records the table lists for that kadı.
async function refreshRouteMatches() {
  const request = ++state.matchRequest;
  const entries = [...state.persons];

  if (!entries.length) return;

  if (!hasStepFilters()) {
    for (const entry of entries) entry.matched = null;
  } else {
    const filters = stepFilters();
    try {
      const results = await Promise.all(
        entries.map((entry) => getAppointmentIds({ ...filters, person_id: entry.person.id })));
      if (request !== state.matchRequest) return;
      entries.forEach((entry, index) => {
        entry.matched = results[index].ids;
      });
    } catch (error) {
      if (request !== state.matchRequest) return;
      console.error(error);
      for (const entry of entries) entry.matched = null;
      toast(t("errors.matchLoad"), "error");
    }
  }

  drawRoutes();
  if (state.detail.kind === "person") renderDetail();
}

function renderMapHeading() {
  const count = state.persons.length;

  if (count) {
    const active = activeEntry();
    const records = state.persons.reduce((sum, { person }) => sum + person.appointments.length, 0);
    const { from, to } = effectiveWindow();
    const filtered = hasStepFilters() ? describeFilters({ bare: true }) : "";

    els.mapEyebrow.textContent = t("map.routeEyebrow");
    els.mapTitle.textContent = count > 1 ? t("map.compareTitle", { count }) : active?.person.name ?? "";
    els.mapSubtitle.textContent = [
      isFullRange()
        ? t("map.routeSubtitle")
        : t("map.routeWindow", { from: from != null ? formatYear(from) : "…", to: to != null ? formatYear(to) : "…" }),
      filtered ? t("map.routeFiltered", { filters: filtered }) : ""
    ].filter(Boolean).join(" · ");
    els.resultCount.textContent = t("map.recordCount", { count: formatNumber(records) });
    return;
  }

  els.mapEyebrow.textContent = t("map.eyebrow");

  if (state.place) {
    const place = placeLabel(state.place);
    const key = state.direction || "any";
    els.mapTitle.textContent = t(`map.placeFocusTitle.${key}`, { place });
    els.mapSubtitle.textContent = [
      t(state.mapMode === "flows" ? "map.placeFocusFlows" : "map.placeFocusPoints"),
      describeFilters({ withPlace: false }) === t("query.noFilters") ? "" : describeFilters({ withPlace: false })
    ].filter(Boolean).join(" · ");
  } else {
    els.mapTitle.textContent = state.mapMode === "flows" ? t("map.title") : t("map.placesTitle");
    els.mapSubtitle.textContent = hasActiveFilters() ? describeFilters() : t("map.subtitle");
  }

  const counted = state.lastCount;

  if (!counted) {
    els.resultCount.textContent = "";
  } else if (counted.kind === "flows") {
    els.resultCount.textContent =
      counted.total > counted.shown
        ? t("map.flowCountPartial", { shown: formatNumber(counted.shown), total: formatNumber(counted.total) })
        : t("map.flowCount", { count: formatNumber(counted.total) });
  } else {
    els.resultCount.textContent = t("map.placeCount", { count: formatNumber(counted.total) });
  }
}

function renderLegend() {
  const legend = els.mapLegend;
  legend.replaceChildren();

  if (state.persons.length) {
    const entry = activeEntry();
    if (!entry) return;

    const color = routeColor(entry.hue, 0.78);
    const pill = el("span", { class: "legend-pill", "aria-hidden": "true" }, "3·21");
    pill.style.background = color;
    const solid = el("span", { class: "legend-line", "aria-hidden": "true" });
    solid.style.borderTopColor = color;
    const dashed = el("span", { class: "legend-line is-dashed", "aria-hidden": "true" });
    dashed.style.borderTopColor = color;

    legend.append(
      el("div", {}, pill, t("map.legendStops")),
      el("div", {}, solid, t("map.legendLinked")),
      el("div", {}, dashed, t("map.legendGap"))
    );
    if (state.persons.some((item) => item.matched)) {
      legend.append(el("div", {}, el("span", { class: "legend-line is-faded", "aria-hidden": "true" }), t("map.legendUnmatched")));
    }
    // route arrows only fade while the timeline is narrowed
    if (!isFullRange()) legend.append(legendAge());
    return;
  }

  if (state.mapMode === "flows") {
    legend.append(
      el("div", {}, el("span", { class: "legend-line", "aria-hidden": "true" }), t("map.legendFlow")),
      legendAge(),
      el("div", {}, el("span", { class: "legend-node", "aria-hidden": "true" }), t("common.place"))
    );
  } else {
    legend.append(
      el("div", {},
        el("span", { class: "legend-dots", "aria-hidden": "true" }, el("i"), el("i"), el("i")),
        t("map.legendPoints"))
    );
  }

  if (focusKey()) {
    legend.append(el("div", {}, el("span", { class: "legend-focus", "aria-hidden": "true" }), t("map.legendFocus")));
  }
}

function legendAge() {
  return el("div", {}, el("span", { class: "legend-line is-faded", "aria-hidden": "true" }), t("map.legendAge"));
}

// Only show the overlay for slow requests, so quick updates don't flicker.
function setMapLoading(loading) {
  clearTimeout(loadingTimer);
  els.map.setAttribute("aria-busy", String(loading));

  if (loading) {
    loadingTimer = setTimeout(() => { els.mapLoading.hidden = false; }, LOADING_DELAY_MS);
  } else {
    els.mapLoading.hidden = true;
  }
}

function setMapEmpty(reason) {
  els.mapEmpty.hidden = !reason;
  if (!reason) return;

  els.mapEmptyTitle.textContent = reason.title ?? "";
  els.mapEmptyText.textContent = reason.text ?? "";

  const action = reason.retry ? { label: t("actions.retry"), run: reason.retry } : reason.action;

  els.mapRetry.hidden = !action;
  els.mapRetry.onclick = action ? () => action.run() : null;
  if (action) els.mapRetry.textContent = action.label;
}

function handleMapSelection(selection) {
  if (!selection) return;

  if (selection.type === "appointment") {
    if (selection.personId != null && selection.personId !== state.activePersonId) {
      activatePerson(selection.personId, { redraw: false });
    }
    openRecord({ appointment: selection.data });
    return;
  }

  if (selection.type === "flow") {
    showFlow(selection.data);
    return;
  }

  if (selection.type === "place" && selection.id != null) {
    showPlace(selection.id);
  }
}

/* ---------- persons ---------- */

// `stay`: keep the current view (e.g. a kadı picked in the query table is
// added to the selection and opened in the panel, the table stays).
async function addPerson(id, { stay = false } = {}) {
  if (!stay) setView("map");

  if (state.persons.some((entry) => entry.person.id === id)) {
    activatePerson(id);
    return;
  }

  if (state.persons.length >= MAX_ROUTES) {
    toast(t("persons.limit", { count: MAX_ROUTES }));
    return;
  }

  const request = ++state.detailRequest;
  showLoadingDetail(t("details.loadingPerson"));

  try {
    const person = await getPerson(id);

    // a second click may have added it meanwhile
    if (!state.persons.some((entry) => entry.person.id === person.id)) {
      if (state.persons.length >= MAX_ROUTES) {
        toast(t("persons.limit", { count: MAX_ROUTES }));
        if (request === state.detailRequest) renderDetail();
        return;
      }
      state.persons.push({ person, hue: pickHue(state.persons.map((entry) => entry.hue)) });
    }

    stopPlayback();

    if (request === state.detailRequest) {
      activatePerson(person.id, { redraw: false });
    } else {
      renderPersonChips();
    }

    renderMapMode();
    writeHash();
    refreshMap({ fit: true });
    refreshRouteMatches();
    queryView.invalidate();
  } catch (error) {
    console.error(error);
    toast(error.status === 404 ? t("errors.notFound") : t("errors.personLoad"), "error");
    if (request === state.detailRequest) renderDetail();
  }
}

function activatePerson(id, { redraw = true } = {}) {
  if (!state.persons.some((entry) => entry.person.id === id)) return;

  const wasPerson = state.detail.kind === "person";

  state.activePersonId = id;
  state.record = null;
  state.detailRequest += 1;

  setDetail({ kind: "person" }, wasPerson && state.tab !== "record" ? state.tab : "route");
  renderPersonChips();
  openDetails();
  writeHash();

  if (redraw) {
    map.setActiveRoute(id);
    renderMapHeading();
    renderLegend();
  }
}

function removePerson(id) {
  state.persons = state.persons.filter((entry) => entry.person.id !== id);

  if (state.activePersonId === id) {
    state.activePersonId = state.persons.at(-1)?.person.id ?? null;

    if (state.detail.kind === "person") {
      state.record = null;
      setDetail(state.activePersonId != null ? { kind: "person" } : { kind: "overview" }, "route");
    }
  }

  afterPersonsChanged();
}

function clearPersons() {
  state.persons = [];
  state.activePersonId = null;

  if (state.detail.kind === "person") setDetail({ kind: "overview" });

  afterPersonsChanged();
}

function afterPersonsChanged() {
  stopPlayback();
  state.lastCount = null;
  renderPersonChips();
  renderMapMode();
  writeHash();
  refreshMap({ fit: true });
  refreshRouteMatches();
  queryView.invalidate();
}

function activeEntry() {
  return state.persons.find((entry) => entry.person.id === state.activePersonId) ?? null;
}

function renderPersonChips() {
  els.personChips.replaceChildren();
  els.clearPersons.hidden = state.persons.length < 2;
  els.personHint.hidden = state.persons.length > 0;

  for (const { person, hue } of state.persons) {
    const active = person.id === state.activePersonId;
    const item = el("li", { class: active ? "person-chip is-active" : "person-chip" });

    item.style.setProperty("--chip-ramp", routeColor(hue, 0.78));
    item.style.setProperty("--chip-color", routeColor(hue, 0.85));

    const select = el(
      "button",
      { type: "button", class: "person-chip-name", "aria-pressed": String(active), title: person.name },
      el("span", { class: "person-chip-swatch", "aria-hidden": "true" }),
      el("span", { class: "person-chip-label" }, person.name)
    );
    select.addEventListener("click", () => activatePerson(person.id));

    const label = t("persons.remove", { name: person.name });
    const remove = el("button", { type: "button", class: "chip-remove", "aria-label": label, title: label }, "×");
    remove.addEventListener("click", () => removePerson(person.id));

    item.append(select, remove);
    els.personChips.append(item);
  }
}

/* ---------- details panel ---------- */

function setDetail(detail, tab = null) {
  state.detail = detail;
  state.tab = tab;
  if (detail.kind === "overview") state.record = null;
  renderDetail();
}

function showLoadingDetail(message) {
  els.detailEyebrow.textContent = t("details.eyebrow");
  els.detailTitle.textContent = message;
  els.detailSubtitle.textContent = "";
  els.detailSummary.replaceChildren();
  els.detailActions.replaceChildren();
  els.detailTabs.hidden = true;
  els.detailBody.replaceChildren(loadingBlock());
  openDetails();
}

function renderDetail() {
  const detail = state.detail;

  els.detailSubtitle.replaceChildren();
  els.detailSummary.replaceChildren();
  els.detailActions.replaceChildren();
  els.detailBody.replaceChildren();

  let tabs;

  switch (detail.kind) {
    case "person":
      tabs = renderPersonHeader();
      break;
    case "place":
      tabs = renderPlaceHeader(detail);
      break;
    case "placeGroup":
      tabs = renderPlaceGroupHeader(detail);
      break;
    case "flow":
      tabs = renderFlowHeader(detail);
      break;
    case "record":
      tabs = renderRecordHeader();
      break;
    default:
      tabs = renderOverview();
  }

  if (state.record && tabs.length && !tabs.some((tab) => tab.id === "record")) {
    tabs.push({ id: "record", label: t("details.record"), render: renderRecordPanel });
  }

  renderTabs(tabs);
  updateFab();
}

function renderTabs(tabs) {
  els.detailTabs.replaceChildren();
  els.detailTabs.hidden = tabs.length < 2;
  els.detailBody.removeAttribute("aria-labelledby");

  if (!tabs.length) return;

  if (!tabs.some((tab) => tab.id === state.tab)) state.tab = tabs[0].id;

  const select = (id) => {
    state.tab = id;
    renderDetail();
    $(`#detail-tab-${id}`)?.focus();
  };

  tabs.forEach((tab, index) => {
    const active = tab.id === state.tab;
    const button = el("button", {
      type: "button",
      role: "tab",
      id: `detail-tab-${tab.id}`,
      class: active ? "details-tab is-active" : "details-tab",
      "aria-selected": String(active),
      "aria-controls": "detail-body",
      tabindex: active ? "0" : "-1"
    }, tab.label);

    button.addEventListener("click", () => select(tab.id));
    button.addEventListener("keydown", (event) => {
      const move = { ArrowRight: 1, ArrowLeft: -1 }[event.key];
      if (!move) return;
      event.preventDefault();
      select(tabs[(index + move + tabs.length) % tabs.length].id);
    });

    els.detailTabs.append(button);
  });

  if (tabs.length > 1) els.detailBody.setAttribute("aria-labelledby", `detail-tab-${state.tab}`);
  tabs.find((tab) => tab.id === state.tab).render();
}

function renderOverview() {
  const meta = state.meta;

  els.detailEyebrow.textContent = t("details.eyebrow");
  els.detailTitle.textContent = t("details.overview");
  els.detailSubtitle.textContent = t("details.empty");

  if (meta) {
    renderSummary([
      [t("dataset.records"), formatNumber(meta.records)],
      [t("dataset.persons"), formatNumber(meta.persons)],
      [t("dataset.places"), formatNumber(meta.places)],
      [t("dataset.years"), meta.min_year != null ? `${meta.min_year}–${meta.max_year}` : "—"]
    ]);
  }

  return [{
    id: "guide",
    label: t("details.overview"),
    render: () => {
      const steps = el("ol", { class: "guide-list" },
        ...["search", "place", "compare", "time", "query", "source"].map((key) =>
          el("li", {}, el("strong", {}, t(`guide.${key}Title`)), el("span", {}, t(`guide.${key}`))))
      );

      const about = el("button", { type: "button", class: "text-button" }, t("about.link"));
      about.addEventListener("click", () => els.aboutDialog.showModal());

      els.detailBody.append(el("div", { class: "detail-section" }, steps, about));
    }
  }];
}

/* person */

function renderPersonHeader() {
  const entry = activeEntry();
  if (!entry) return renderOverview();

  const { person, matched } = entry;
  const appointments = person.appointments;
  const dated = appointments.filter((a) => a.year_numeric != null);
  const { stops, stopOf } = buildItinerary(appointments);
  const places = new Set(stops.map((stop) => stop.key));
  const drawn = stops.filter(hasPoint).length;

  els.detailEyebrow.textContent = t("details.person");
  els.detailTitle.textContent = person.name;
  els.detailSubtitle.append(
    el("span", { class: "status-pill" }, resolutionLabel(person.resolution_status)),
    " ",
    t("details.provisionalNote")
  );

  renderSummary([
    [t("details.appointments"), formatNumber(appointments.length)],
    [t("details.placesVisited"), formatNumber(places.size)],
    [t("details.onMap"), `${formatNumber(drawn)} / ${formatNumber(stops.length)}`],
    [t("details.span"), dated.length ? `${dateLabel(dated[0])} – ${dateLabel(dated.at(-1))}` : "—"]
  ]);

  if (drawn) {
    const focus = el("button", { type: "button", class: "secondary-button compact" }, t("actions.focusRoute"));
    focus.addEventListener("click", () => {
      setView("map");
      map.fitRoute(person.id);
      closeDetailsOnDrawer();
    });
    els.detailActions.append(focus);
  }

  const table = el("button", { type: "button", class: "secondary-button compact" }, t("actions.showTable"));
  table.addEventListener("click", () => setView("query"));
  els.detailActions.append(table);

  if (person.wikidata_qid) {
    els.detailActions.append(externalLink(`https://www.wikidata.org/wiki/${encodeURIComponent(person.wikidata_qid)}`, "Wikidata"));
  }

  return [
    { id: "route", label: t("details.route"), render: () => renderItinerary(entry) },
    {
      id: "appointments",
      label: t("details.appointments"),
      render: () => {
        if (matched) els.detailBody.append(matchNote(matched, appointments));
        renderAppointmentList(appointments, { stopOf, matched });
      }
    }
  ];
}

function matchNote(matched, appointments) {
  const count = appointments.filter((a) => matched.has(a.id)).length;
  return el("p", { class: "detail-note is-match-note" },
    t("route.matchNote", { count: formatNumber(count), total: formatNumber(appointments.length), filters: describeFilters({ bare: true }) }));
}

// The places a kadı passed through, in itinerary order (A → B → C …). The API
// already ordered the steps along the old place → new place chain; a new
// start is only added where that chain breaks.
// The kadı's stops, numbered exactly like the badges on the map. A place
// visited twice appears twice; stops without coordinates are listed but
// flagged, and gaps in the record get their own row.
function renderItinerary(entry) {
  const { person, hue, matched } = entry;
  const { stops } = buildItinerary(person.appointments);

  if (!stops.length) {
    els.detailBody.append(emptyBlock(t("details.noAppointments")));
    return;
  }

  const color = routeColor(hue, 0.78);
  const list = el("ol", { class: "route-list" });

  for (const stop of stops) {
    if (stop.gap) list.append(el("li", { class: "route-gap" }, t("route.gapRow")));

    const hit = !matched || matched.has(stop.appointment.id) || stop.events.some((e) => matched.has(e.id));
    const item = el("li", {
      class: ["route-stop", matched && !hit ? "is-dim" : "", matched && hit ? "is-match" : ""].filter(Boolean).join(" ")
    });
    item.style.setProperty("--step-color", color);

    const marker = el("span", { class: "route-marker", "aria-hidden": "true" }, String(stop.n));
    const text = el("span", { class: "route-text" },
      el("strong", {}, stop.name ?? t("common.unknownPlace")),
      el("span", {}, stopLabel(stop)),
      hasPoint(stop) ? null : el("span", { class: "route-nomap" }, t("route.notOnMap")));

    const button = el("button", {
      type: "button",
      class: "route-stop-button",
      title: hasPoint(stop) ? t("route.showOnMap") : t("route.openRecord"),
      "aria-label": `${t("route.stopNo", { n: stop.n })}: ${stop.name ?? t("common.unknownPlace")}, ${stopLabel(stop)}`
    }, marker, text);

    button.addEventListener("click", () => {
      if (hasPoint(stop) && stop.placeId != null) {
        setView("map");
        map.highlight(`place:${stop.placeId}`);
      }
      openRecord({ appointment: stop.appointment });
    });

    item.append(button);
    list.append(item);

    for (const event of stop.events) {
      list.append(el("li", { class: "route-event" }, successorLine(event, dateLabel(event) ?? t("time.unknown"))));
    }
  }

  els.detailBody.append(...[
    el("p", { class: "detail-note" }, t("route.help")),
    matched ? matchNote(matched, person.appointments) : null,
    list
  ].filter(Boolean));
}

function stopLabel(stop) {
  const year = dateLabel(stop.appointment) ?? t("time.unknown");

  switch (stop.kind) {
    case "start":
      return t("route.start");
    case "restart":
      return t("route.restart");
    case "departed":
      return t("route.departedAt", { year });
    default:
      return t("route.arrived", { year });
  }
}

function dateLabel(appointment) {
  return formatDate(appointment?.year_numeric, appointment?.year_label);
}

function successorLine(appointment, year) {
  const name = appointment.new_person?.name ?? appointment.raw_new_kadi;
  return name ? t("route.leftPost", { year, successor: name }) : t("route.departedAt", { year });
}

// `stopOf` (a kadı's own list) names each record's stop number on the route;
// `matched` dims the records outside the sidebar filters.
function renderAppointmentList(appointments, { stopOf = null, matched = null } = {}) {
  if (!appointments.length) {
    els.detailBody.append(emptyBlock(t("details.noAppointments")));
    return;
  }

  const own = stopOf !== null;
  const list = el("ol", { class: "detail-list" });

  appointments.forEach((appointment, index) => {
    const brokenChain = own && index > 0 && appointment.follows_previous === false;
    const selected = state.record?.appointment?.id === appointment.id;
    const dim = matched && !matched.has(appointment.id);

    const row = el("li", {
      class: ["detail-row", brokenChain ? "is-gap" : "", selected ? "is-selected" : "", dim ? "is-dim" : ""].filter(Boolean).join(" ")
    });

    const stop = own ? stopOf.get(appointment.id) : null;
    const time = el("span", { class: "detail-row-time" },
      [stop ? t("route.stopNo", { n: stop }) : null, dateLabel(appointment) ?? t("time.unknown")]
        .filter(Boolean).join(" · "));

    if (brokenChain) time.append(" ", el("em", { class: "chain-gap", title: t("route.gapRow") }, t("route.gap")));

    const departed = own && appointment.role === "departed";
    const oldName = appointment.old_person?.name ?? appointment.raw_old_kadi;
    const newName = appointment.new_person?.name ?? appointment.raw_new_kadi;

    // in a kadı's own list, name the other party; elsewhere, both of them
    const people =
      !own
        ? oldName || newName ? `${oldName ?? "—"} → ${newName ?? "—"}` : null
        : departed
          ? newName ? t("route.successor", { name: newName }) : null
          : oldName ? t("route.predecessor", { name: oldName }) : null;

    const button = el("button", { type: "button", class: "detail-row-button", "aria-pressed": String(selected) },
      time,
      el("strong", {}, departed
        ? t("route.departedRow", { place: appointment.destination ?? "—" })
        : `${appointment.origin ?? "—"} → ${appointment.destination ?? "—"}`),
      people ? el("span", { class: "detail-row-people" }, people) : null,
      el("span", { class: "detail-row-meta" }, [
        appointment.degree,
        appointment.position_type,
        salaryLabel(appointment.old_salary, appointment.salary)
      ].filter(Boolean).join(" · ")));

    button.addEventListener("click", () => openRecord({ appointment }));
    row.append(button);
    list.append(row);
  });

  els.detailBody.append(list);
}

/* place */

// `push: false` when the detail is being restored from the URL, so reopening
// a link does not add a history entry of its own.
async function showPlace(id, { push = true } = {}) {
  const request = ++state.detailRequest;
  showLoadingDetail(t("details.loadingPlace"));

  try {
    const place = await getPlace(id);
    if (request !== state.detailRequest) return;

    state.record = null;
    map.highlight(`place:${place.id}`);
    setDetail({ kind: "place", place, persons: null }, "persons");
    writeHash({ push });
    loadPlacePersons();
  } catch (error) {
    if (request !== state.detailRequest) return;
    console.error(error);
    toast(error.status === 404 ? t("errors.notFound") : t("errors.placeLoad"), "error");
    setDetail({ kind: "overview" });
  }
}

// Every place whose name starts with the prefix of the place filter.
function showPlaceGroup(prefix = state.place?.prefix, { push = true } = {}) {
  if (!prefix) return;

  state.detailRequest += 1;
  state.record = null;
  const detail = { kind: "placeGroup", prefix, persons: null, places: null };
  setDetail(detail, "persons");
  openDetails();
  writeHash({ push });
  loadPlacePersons();
  loadGroupPlaces(detail);
}

// The kadıs of a place (or name group) under the current filters — the same
// records the map and the table show for it. With a direction filter only the
// kadıs who moved (the incoming kadı of each record) are listed.
async function loadPlacePersons() {
  const detail = state.detail;
  if (detail.kind !== "place" && detail.kind !== "placeGroup") return;

  const token = (detail.personsToken ?? 0) + 1;
  detail.personsToken = token;

  const own = detail.kind === "place" ? { id: detail.place.id } : { prefix: detail.prefix };
  const filters = queryFilters();
  delete filters.place_id;
  delete filters.place_prefix;
  delete filters.direction;
  if (own.id != null) filters.place_id = own.id;
  else filters.place_prefix = own.prefix;
  if (samePlace(state.place, own) && state.direction) filters.direction = state.direction;

  const by = filters.direction ? "new_kadi" : "person";
  const both = { ...filters };
  delete both.direction;

  try {
    // the panel's numbers all come from the same filtered records as its list
    const [stats, arrivals, departures] = await Promise.all([
      getStats(by, { ...filters, limit: PLACE_PERSONS_LIMIT }),
      getSummary({ ...both, direction: "arrival" }),
      getSummary({ ...both, direction: "departure" })
    ]);
    if (state.detail !== detail || detail.personsToken !== token) return;
    detail.persons = { ...stats, direction: filters.direction ?? "" };
    detail.counts = { arrivals: arrivals.records, departures: departures.records };
  } catch (error) {
    if (state.detail !== detail || detail.personsToken !== token) return;
    console.error(error);
    detail.persons = { error };
  }

  renderDetail();
}

async function loadGroupPlaces(detail) {
  try {
    const places = await searchPlaces(detail.prefix, { match: "prefix", limit: 100 });
    if (state.detail !== detail) return;
    detail.places = places;
  } catch (error) {
    if (state.detail !== detail) return;
    console.error(error);
    detail.places = { error };
  }
  renderDetail();
}

function renderPlaceHeader(detail) {
  const place = detail.place;

  els.detailEyebrow.textContent = t("details.place");
  els.detailTitle.textContent = place.name;
  els.detailSubtitle.textContent =
    place.latitude != null && place.longitude != null
      ? `${formatCoordinate(place.latitude, "N", "S")}, ${formatCoordinate(place.longitude, "E", "W")}`
      : t("details.placeNoCoordinates");

  renderSummary([
    [t("details.arrivals"), detail.counts ? formatNumber(detail.counts.arrivals) : "…"],
    [t("details.departures"), detail.counts ? formatNumber(detail.counts.departures) : "…"],
    [t("details.distinctPersons"), detail.persons?.groups != null ? formatNumber(detail.persons.groups) : "…"],
    [t("details.wikidata"), place.wikidata_qid ?? "—"]
  ]);

  const own = { id: place.id, name: place.name, hasCoordinates: place.latitude != null };
  renderPlaceActions(own);

  if (place.wikidata_qid) {
    els.detailActions.append(externalLink(`https://www.wikidata.org/wiki/${encodeURIComponent(place.wikidata_qid)}`, "Wikidata"));
  }

  return [
    { id: "persons", label: t("details.persons"), render: () => renderPlacePersons(detail) },
    { id: "names", label: t("details.names"), render: () => renderPlaceNames(place) }
  ];
}

function renderPlaceGroupHeader(detail) {
  const places = Array.isArray(detail.places) ? detail.places : null;

  els.detailEyebrow.textContent = t("details.placeGroup");
  els.detailTitle.textContent = t("filters.prefixLabel", { prefix: detail.prefix });
  els.detailSubtitle.textContent = t("details.placeGroupNote");

  renderSummary([
    [t("dataset.places"), places ? formatNumber(places.length) : "…"],
    [t("details.arrivals"), detail.counts ? formatNumber(detail.counts.arrivals) : "…"],
    [t("details.departures"), detail.counts ? formatNumber(detail.counts.departures) : "…"],
    [t("details.distinctPersons"), detail.persons?.groups != null ? formatNumber(detail.persons.groups) : "…"]
  ]);

  renderPlaceActions({ prefix: detail.prefix, name: detail.prefix });

  return [
    { id: "persons", label: t("details.persons"), render: () => renderPlacePersons(detail) },
    { id: "places", label: t("details.groupPlaces"), render: () => renderGroupPlaces(detail) }
  ];
}

// "Filter by this place" + "show in table" for a place or a name group.
function renderPlaceActions(own) {
  const filtered = samePlace(state.place, own);
  const filter = el("button", {
    type: "button",
    class: filtered ? "secondary-button compact is-on" : "secondary-button compact",
    "aria-pressed": String(filtered)
  }, filtered ? t("actions.removePlaceFilter") : t("actions.filterByPlace"));

  filter.addEventListener("click", () => {
    setPlaceFilter(filtered ? null : own);
    renderDetail();
  });

  const table = el("button", { type: "button", class: "secondary-button compact" }, t("actions.showTable"));
  table.addEventListener("click", () => {
    setPlaceFilter(own);
    setView("query");
  });

  els.detailActions.append(filter, table);
}

function renderPlacePersons(detail) {
  const data = detail.persons;

  if (!data) {
    els.detailBody.append(loadingBlock());
    return;
  }

  if (data.error) {
    const retry = el("button", { type: "button", class: "text-button" }, t("actions.retry"));
    retry.addEventListener("click", () => loadPlacePersons());
    els.detailBody.append(el("div", { class: "detail-empty" }, t("errors.placeLoad"), " ", retry));
    return;
  }

  const filters = describeFilters({ withPlace: false });
  els.detailBody.append(el("p", { class: "detail-note" },
    t(data.direction ? `details.personsBy.${data.direction}` : "details.personsBy.any", {
      count: formatNumber(data.groups),
      records: formatNumber(data.records)
    }),
    filters === t("query.noFilters") ? "" : ` ${filters}`));

  if (!data.items.length) {
    els.detailBody.append(emptyBlock(t("details.noPersons")));
    return;
  }

  const list = el("ul", { class: "person-list" });

  for (const person of data.items) {
    const added = state.persons.some((entry) => entry.person.id === person.id);
    const button = el("button", { type: "button", class: "person-list-button" },
      el("strong", {}, person.label ?? "—"),
      el("span", {}, t("details.recordCount", { count: formatNumber(person.count) })),
      el("span", { class: added ? "person-list-action is-added" : "person-list-action" },
        added ? t("persons.onMap") : t("persons.showRoute")));

    button.addEventListener("click", () => addPerson(person.id));
    list.append(el("li", {}, button));
  }

  els.detailBody.append(list);

  if (data.groups > data.items.length) {
    els.detailBody.append(el("p", { class: "detail-note" },
      t("details.personsCapped", { shown: formatNumber(data.items.length), total: formatNumber(data.groups) })));
  }
}

function renderGroupPlaces(detail) {
  const places = detail.places;

  if (!places) {
    els.detailBody.append(loadingBlock());
    return;
  }

  if (places.error) {
    els.detailBody.append(emptyBlock(t("errors.placeLoad")));
    return;
  }

  const list = el("ul", { class: "person-list" });

  for (const place of places) {
    const button = el("button", { type: "button", class: "person-list-button" },
      el("strong", {}, place.name),
      el("span", {}, [
        t("appointments.count", { count: formatNumber(place.appointment_count) }),
        place.latitude == null ? t("common.noCoordinates") : null
      ].filter(Boolean).join(" · ")),
      el("span", { class: "person-list-action" }, t("actions.open")));

    button.addEventListener("click", () => showPlace(place.id));
    list.append(el("li", {}, button));
  }

  els.detailBody.append(list);
}

function renderPlaceNames(place) {
  if (!place.historical_names.length) {
    els.detailBody.append(emptyBlock(t("details.noNames")));
    return;
  }

  const list = el("ul", { class: "name-list" });

  for (const name of place.historical_names) {
    list.append(el("li", {},
      el("strong", {}, name.name),
      el("span", {}, [
        intervalLabel(name.valid_from, name.valid_to),
        name.calendar && name.calendar !== "unknown" ? calendarLabel(name.calendar) : null,
        name.language
      ].filter(Boolean).join(" · "))));
  }

  els.detailBody.append(list);
}

/* flow */

function showFlow(flow, { push = true } = {}) {
  state.record = null;
  setDetail({ kind: "flow", flow, appointments: null, total: 0, error: null }, "appointments");
  openDetails();
  writeHash({ push });
  loadFlowDetail(flow);
}

async function loadFlowDetail(flow) {
  const request = ++state.detailRequest;
  const filters = queryFilters();
  delete filters.place_id;

  try {
    const { items, total } = await getAppointments({
      ...filters,
      origin_place_id: flow.origin_id,
      destination_place_id: flow.destination_id,
      limit: 200
    });

    if (request !== state.detailRequest || state.detail.flow !== flow) return;
    // a flow restored from a link only knows its place ids
    if (!flow.origin && items[0]) flow.origin = items[0].origin ?? "";
    if (!flow.destination && items[0]) flow.destination = items[0].destination ?? "";
    setDetail({ kind: "flow", flow, appointments: items, total, error: null }, state.tab);
  } catch (error) {
    if (request !== state.detailRequest || state.detail.flow !== flow) return;
    console.error(error);
    setDetail({ kind: "flow", flow, appointments: [], total: 0, error }, state.tab);
  }
}

function renderFlowHeader(detail) {
  const { flow } = detail;
  const count = detail.appointments ? detail.total : flow.count;

  els.detailEyebrow.textContent = t("details.flow");
  els.detailTitle.textContent = `${flow.origin} → ${flow.destination}`;
  els.detailSubtitle.textContent = t("details.flowSubtitle");

  renderSummary([
    [t("details.appointments"), formatNumber(count)],
    [t("details.window"), windowLabel()]
  ]);

  return [{
    id: "appointments",
    label: t("details.appointments"),
    render: () => {
      if (detail.error) {
        const retry = el("button", { type: "button", class: "text-button" }, t("actions.retry"));
        retry.addEventListener("click", () => loadFlowDetail(flow));
        els.detailBody.append(el("div", { class: "detail-empty" }, t("errors.flowLoad"), " ", retry));
      } else if (!detail.appointments) {
        els.detailBody.append(loadingBlock());
      } else {
        renderAppointmentList(detail.appointments);
        if (detail.total > detail.appointments.length) {
          els.detailBody.append(el("p", { class: "detail-note" },
            t("details.listCapped", { shown: detail.appointments.length, total: formatNumber(detail.total) })));
        }
      }
    }
  }];
}

/* record: one source row and the appointment derived from it */

async function openRecord({ appointment = null, sourceId = null, push = true }) {
  const id = sourceId ?? appointment?.source_record_id ?? null;
  const request = ++state.recordRequest;
  const withSource = canSeeSources();

  if (appointment) map.highlight(`appointment:${appointment.id}`);

  state.record = { appointment, sourceId: id, source: null, loading: id != null, error: null };
  state.tab = "record";
  renderDetail();
  openDetails();
  writeHash({ push });

  if (id == null) return;

  try {
    const [source, linked] = await Promise.all([
      withSource ? getSource(id) : null,
      appointment ? null : getAppointments({ source_record_id: id, limit: 5 })
    ]);

    if (request !== state.recordRequest) return;

    state.record = {
      appointment: appointment ?? linked?.items[0] ?? null,
      sourceId: id,
      source,
      loading: false,
      error: null
    };
  } catch (error) {
    if (request !== state.recordRequest) return;
    console.error(error);
    state.record = { ...state.record, loading: false, error };
  }

  renderDetail();
}

function openSourceRecord(sourceId, { push = true } = {}) {
  state.detailRequest += 1;
  state.detail = { kind: "record" };
  openRecord({ sourceId, push });
}

function renderRecordHeader() {
  const record = state.record;
  const appointment = record?.appointment;

  els.detailEyebrow.textContent = t("details.source");
  els.detailTitle.textContent = record?.source?.doc_id ?? appointment?.source_doc_id ?? t("source.record");
  els.detailSubtitle.textContent = appointment
    ? `${appointment.origin ?? "—"} → ${appointment.destination ?? "—"}`
    : record?.loading ? t("common.loading") : "";

  return [{ id: "record", label: t("details.record"), render: renderRecordPanel }];
}

function renderRecordPanel() {
  const record = state.record;

  if (!record) {
    els.detailBody.append(emptyBlock(t("source.none")));
    return;
  }

  const { appointment, source } = record;
  const section = el("div", { class: "record" });

  if (appointment) {
    const fields = el("dl", { class: "record-fields" });
    const add = (label, value) => {
      if (value === null || value === undefined || value === "") return;
      fields.append(el("dt", {}, label), el("dd", {}, value));
    };

    add(t("record.date"), appointment.year_label);
    if (appointment.year_numeric != null) {
      add(t("record.gregorian"), formatDate(appointment.year_numeric, null, "gregorian"));
    }
    add(t("record.calendar"), calendarLabel(appointment.calendar));
    add(t("record.oldKadi"), personLink(appointment.old_person, appointment.raw_old_kadi));
    add(t("record.newKadi"), personLink(appointment.new_person, appointment.raw_new_kadi));
    add(t("record.oldPlace"), placeLink(appointment.origin_place_id, appointment.origin));
    add(t("record.newPlace"), placeLink(appointment.destination_place_id, appointment.destination));
    add(t("filters.degree"), appointment.degree);
    add(t("filters.positionType"), appointment.position_type);
    add(t("record.period"), appointment.period);
    add(t("record.salary"), salaryLabel(appointment.old_salary, appointment.salary));
    add(t("record.region"), appointment.region_raw);
    add(t("record.document"), appointment.source_doc_id ?? source?.doc_id);
    add(t("record.folio"), appointment.varak_no ?? source?.varak_no);

    section.append(fields);
  }

  if (!canSeeSources()) {
    section.append(el("p", { class: "detail-note" }, t("source.adminOnly")));
  } else if (record.loading) {
    section.append(loadingBlock());
  } else if (record.error) {
    const retry = el("button", { type: "button", class: "text-button" }, t("actions.retry"));
    retry.addEventListener("click", () => openRecord({ appointment, sourceId: record.sourceId }));
    section.append(el("div", { class: "detail-empty" }, t("errors.sourceLoad"), " ", retry));
  } else if (source) {
    section.append(
      el("h3", { class: "record-heading" }, t("source.text")),
      source.text ? el("blockquote", { class: "source-text" }, source.text) : emptyBlock(t("source.notFound"))
    );

    if (source.certificate) {
      section.append(el("p", { class: "detail-note" }, `${t("source.certificate")}: ${source.certificate}`));
    }

    const raw = Object.entries(source.raw ?? {}).filter(([, value]) => value !== null && value !== "");

    if (raw.length) {
      const table = el("dl", { class: "raw-fields" });
      for (const [key, value] of raw) {
        table.append(el("dt", {}, key), el("dd", {}, typeof value === "object" ? JSON.stringify(value) : String(value)));
      }
      section.append(el("details", { class: "raw-record" }, el("summary", {}, t("source.raw")), table));
    }
  } else if (!appointment) {
    section.append(emptyBlock(t("source.none")));
  } else {
    section.append(emptyBlock(t("source.notFound")));
  }

  els.detailBody.append(section);
}

function personLink(person, raw) {
  if (!person?.id) return raw ?? null;

  const button = el("button", { type: "button", class: "inline-link" }, person.name || raw || "—");
  button.addEventListener("click", () => addPerson(person.id));
  return button;
}

function placeLink(id, name) {
  if (id == null) return name ?? null;

  const button = el("button", { type: "button", class: "inline-link" }, name ?? "—");
  button.addEventListener("click", () => showPlace(id));
  return button;
}

/* ---------- small blocks ---------- */

function renderSummary(items) {
  els.detailSummary.replaceChildren();

  for (const [label, value] of items) {
    const text = String(value ?? "—");
    els.detailSummary.append(el("div", {}, el("span", {}, label), el("strong", { title: text }, text)));
  }
}

function loadingBlock() {
  return el("div", { class: "detail-loading", role: "status" },
    el("span", { class: "loading-indicator", "aria-hidden": "true" }),
    t("common.loading"));
}

function emptyBlock(text) {
  return el("div", { class: "detail-empty" }, text);
}

function externalLink(href, label) {
  return el("a", { class: "secondary-button compact", href, target: "_blank", rel: "noopener noreferrer" }, `${label} ↗`);
}

function openDetails() {
  els.details.classList.add("is-open");
  updateFab();
}

function closeDetails() {
  els.details.classList.remove("is-open");
  updateFab();
}

function closeDetailsOnDrawer() {
  if (drawerQuery.matches) closeDetails();
}

function updateFab() {
  els.detailsFab.hidden = !drawerQuery.matches || els.details.classList.contains("is-open");
}

/* ---------- views ---------- */

function setView(view) {
  if (!VIEWS.includes(view)) return;
  if (view === "sources" && !canSeeSources()) view = "map";

  const changed = state.view !== view;

  state.view = view;
  els.mapView.hidden = view !== "map";
  els.queryView.hidden = view !== "query";
  els.sourcesView.hidden = view !== "sources";

  for (const button of els.navItems) {
    const active = button.dataset.view === view;
    button.classList.toggle("is-active", active);
    if (active) button.setAttribute("aria-current", "page");
    else button.removeAttribute("aria-current");
  }

  if (view !== "map") stopPlayback();

  if (view === "query") queryView.activate();

  if (view === "sources") {
    if (!state.sources.loaded && !state.sources.loading) runSourceSearch({ reset: true });
    if (changed) els.sourcesQuery.focus({ preventScroll: true });
  }

  writeHash();
}

async function runSourceSearch({ reset }) {
  const sources = state.sources;

  if (reset) {
    sources.q = els.sourcesQuery.value.trim();
    sources.offset = 0;
    sources.items = [];
    sources.total = 0;
    // typing should not leave one history entry per keystroke
    writeHash({ push: false });
  }

  sources.controller?.abort();
  const controller = new AbortController();
  const request = ++sources.request;

  sources.controller = controller;
  sources.loading = true;
  sources.error = null;
  renderSources();

  try {
    const { items, total } = await searchSources(
      { q: sources.q, limit: SOURCE_PAGE, offset: sources.offset },
      { signal: controller.signal }
    );

    if (request !== sources.request) return;

    sources.items = [...sources.items, ...items];
    sources.total = total;
    sources.offset = sources.items.length;
    sources.loaded = true;
  } catch (error) {
    if (request !== sources.request || isAbort(error)) return;
    console.error(error);
    sources.error = error;
  } finally {
    if (request === sources.request) {
      sources.loading = false;
      renderSources();
    }
  }
}

function renderSources() {
  const sources = state.sources;
  const selected = state.detail.kind === "record" ? state.record?.sourceId : null;

  els.sourcesList.replaceChildren();
  els.sourcesList.setAttribute("aria-busy", String(sources.loading));

  for (const source of sources.items) {
    const meta = [source.varak_no ? t("source.folio", { value: source.varak_no }) : null, source.certificate]
      .filter(Boolean).join(" · ");

    const button = el("button", {
      type: "button",
      class: source.id === selected ? "source-card is-selected" : "source-card"
    },
      el("span", { class: "source-card-head" },
        el("strong", {}, source.doc_id ?? "—"),
        meta ? el("span", {}, meta) : null),
      el("span", { class: "source-card-snippet" }, ...highlightedSnippet(source.snippet)));

    button.addEventListener("click", () => {
      $$(".source-card.is-selected", els.sourcesList).forEach((card) => card.classList.remove("is-selected"));
      button.classList.add("is-selected");
      openSourceRecord(source.id);
    });

    els.sourcesList.append(el("li", {}, button));
  }

  if (sources.loading) els.sourcesList.append(el("li", {}, loadingBlock()));

  const empty = !sources.loading && !sources.items.length;

  els.sourcesEmpty.hidden = !(sources.error || empty);
  els.sourcesEmpty.textContent = sources.error
    ? t("errors.sourceSearch")
    : sources.q ? t("sources.noResults", { q: sources.q }) : t("sources.none");

  els.sourcesMore.hidden = sources.loading || Boolean(sources.error) || sources.items.length >= sources.total;

  els.sourcesCount.textContent = sources.loaded
    ? t(sources.q ? "sources.countMatches" : "sources.count", { count: formatNumber(sources.total) })
    : "";
}

// ts_headline wraps hits in <mark>…</mark>; everything else is plain text and
// is never parsed as HTML.
function highlightedSnippet(snippet) {
  const nodes = [];
  let marking = false;

  for (const part of String(snippet ?? "").split(/(<mark>|<\/mark>)/)) {
    if (part === "<mark>") marking = true;
    else if (part === "</mark>") marking = false;
    else if (part) nodes.push(marking ? el("mark", {}, part) : document.createTextNode(part));
  }

  if (!nodes.length) nodes.push(el("em", {}, t("source.notFound")));
  return nodes;
}

/* ---------- search combobox ---------- */

function setupCombobox({ input, list, search, describe, onPick, errorMessage, minLength = 2 }) {
  let timer = 0;
  let controller = null;
  let request = 0;
  let results = [];
  let active = -1;

  const close = () => {
    list.hidden = true;
    list.replaceChildren();
    input.setAttribute("aria-expanded", "false");
    input.removeAttribute("aria-activedescendant");
    active = -1;
  };

  const open = () => {
    list.hidden = false;
    input.setAttribute("aria-expanded", "true");
  };

  const message = (text) => {
    list.replaceChildren(el("div", { class: "search-message", role: "status" }, text));
    open();
  };

  const setActive = (index) => {
    const options = $$('[role="option"]', list);
    if (!options.length) return;

    active = (index + options.length) % options.length;
    options.forEach((option, i) => option.setAttribute("aria-selected", String(i === active)));
    input.setAttribute("aria-activedescendant", options[active].id);
    options[active].scrollIntoView({ block: "nearest" });
  };

  const pick = (item) => {
    close();
    input.value = "";
    results = [];
    onPick(item);
  };

  const render = () => {
    list.replaceChildren();
    active = -1;
    input.removeAttribute("aria-activedescendant");

    if (!results.length) {
      message(t("search.noResults"));
      return;
    }

    results.forEach((item, index) => {
      const info = describe(item);
      const option = el("div", {
        role: "option",
        id: `${list.id}-option-${index}`,
        class: ["search-result", info.added ? "is-added" : "", info.group ? "is-group" : ""].filter(Boolean).join(" "),
        "aria-selected": "false"
      },
        el("span", { class: "search-result-label" }, info.label),
        el("small", {}, info.added ? t("persons.onMap") : info.meta),
        info.note ? el("small", { class: "search-result-note" }, info.note) : null);

      // keep focus in the input while clicking an option
      option.addEventListener("mousedown", (event) => event.preventDefault());
      option.addEventListener("click", () => pick(item));
      list.append(option);
    });

    open();
  };

  input.addEventListener("input", () => {
    clearTimeout(timer);
    controller?.abort();
    results = [];
    request += 1;

    const query = input.value.trim();

    if (query.length < minLength) {
      close();
      return;
    }

    timer = setTimeout(async () => {
      const current = ++request;
      controller = new AbortController();
      message(t("search.searching"));

      try {
        const found = await search(query, controller.signal);
        if (current !== request) return;
        results = found;
        render();
      } catch (error) {
        if (current !== request || isAbort(error)) return;
        console.error(error);
        message(errorMessage());
      }
    }, 200);
  });

  input.addEventListener("keydown", (event) => {
    const ready = !list.hidden && results.length > 0;

    if (event.key === "ArrowDown" && ready) {
      event.preventDefault();
      setActive(active + 1);
    } else if (event.key === "ArrowUp" && ready) {
      event.preventDefault();
      setActive(active - 1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (ready) pick(results[Math.max(active, 0)]);
    } else if (event.key === "Escape") {
      if (!list.hidden) {
        event.preventDefault();
        event.stopPropagation();
        close();
      }
    }
  });

  input.addEventListener("blur", () => setTimeout(close, 120));
  input.addEventListener("focus", () => {
    if (results.length && input.value.trim().length >= minLength) render();
  });
}

/* ---------- shareable state in the URL hash ---------- */

// Everything needed to rebuild the screen lives in the URL hash, so the
// browser's Back button returns to the previous screen with its filters and
// a link reproduces it. Several writes in one tick become one history entry.
let hashWrite = null;

function writeHash({ push = true } = {}) {
  if (!state.meta) return;

  if (hashWrite) {
    hashWrite.push ||= push;
    return;
  }

  hashWrite = { push };
  queueMicrotask(() => {
    const { push: wantPush } = hashWrite;
    hashWrite = null;
    commitHash(wantPush && !state.restoring);
  });
}

function commitHash(push) {
  const params = new URLSearchParams();
  const meta = state.meta;

  if (state.view !== "map") params.set("view", state.view);
  if (state.persons.length) params.set("k", state.persons.map((entry) => entry.person.id).join(","));
  if (state.persons.length > 1 && state.activePersonId != null) params.set("a", state.activePersonId);
  if (state.mapMode !== "flows") params.set("m", state.mapMode);
  if (state.yearFrom != null && state.yearFrom !== meta.min_year) params.set("from", state.yearFrom);
  if (state.yearTo != null && state.yearTo !== meta.max_year) params.set("to", state.yearTo);
  if (state.degree) params.set("degree", state.degree);
  if (state.positionType) params.set("type", state.positionType);
  if (state.place?.id != null) params.set("place", `${state.place.id}:${state.place.name}`);
  if (state.place?.prefix) params.set("pp", state.place.prefix);
  if (state.place && state.direction) params.set("dir", state.direction);

  const detail = detailToken();
  if (detail) params.set("d", detail);
  if (state.record?.sourceId != null) params.set("r", state.record.sourceId);

  if (state.view === "query") {
    const query = queryView.getState();
    if (query.mode !== "table") params.set("qm", query.mode);
    if (query.page) params.set("qp", query.page);
    if (query.sort !== "year") params.set("qs", query.sort);
    if (query.top !== 10) params.set("qt", query.top);
  }
  if (state.view === "sources" && state.sources.q) params.set("sq", state.sources.q);

  const hash = params.toString() ? `#${params}` : "";
  state.lastHash = hash;

  if (hash === location.hash) return;

  const url = `${location.pathname}${location.search}${hash}`;
  if (push) {
    state.historyIndex += 1;
    history.pushState({ kadiIndex: state.historyIndex }, "", url);
  } else {
    history.replaceState({ kadiIndex: state.historyIndex }, "", url);
  }
  updateBackButton();
}

// The open detail, when it is not implied by the selected kadıs.
function detailToken() {
  switch (state.detail.kind) {
    case "place":
      return `place:${state.detail.place.id}`;
    case "placeGroup":
      return `group:${state.detail.prefix}`;
    case "flow":
      return `flow:${state.detail.flow.origin_id}-${state.detail.flow.destination_id}`;
    case "record":
      return state.record?.sourceId != null ? "record" : null;
    default:
      return null;
  }
}

function updateBackButton() {
  els.detailBack.hidden = state.historyIndex <= 0;
}

async function restoreAndRender() {
  state.restoring = true;

  try {
    const detail = await restoreFromHash();
    syncFilterInputs();
    renderDetail();
    refreshMap({ fit: true });
    refreshRouteMatches();
    queryView.invalidate();
    restoreDetail(detail);
  } finally {
    // writes queued while restoring replace the entry instead of adding one
    queueMicrotask(() => {
      state.restoring = false;
    });
  }
}

async function restoreDetail({ token, record }) {
  const split = token?.indexOf(":") ?? -1;
  const kind = split > 0 ? token.slice(0, split) : token;
  const value = split > 0 ? token.slice(split + 1) : null;
  const quiet = { push: false };

  if (kind === "place" && Number(value) > 0) {
    await showPlace(Number(value), quiet);
  } else if (kind === "group" && value) {
    showPlaceGroup(value, quiet);
  } else if (kind === "flow" && /^\d+-\d+$/.test(value ?? "")) {
    const [origin, destination] = value.split("-").map(Number);
    showFlow({ origin_id: origin, destination_id: destination, origin: "", destination: "", count: 0 }, quiet);
  } else if (kind === "record" && record) {
    openSourceRecord(record, quiet);
    return;
  }

  // a record tab sits next to the restored detail
  if (record) openRecord({ sourceId: record, push: false });
}

// Apply the hash to the state; returns what the detail panel should reopen.
async function restoreFromHash() {
  const params = new URLSearchParams(location.hash.slice(1));
  const meta = state.meta;
  const year = (key, fallback) => {
    const value = parseYear(params.get(key));
    return value == null ? fallback : value;
  };

  stopPlayback();

  state.mapMode = params.get("m") === "points" ? "points" : "flows";
  state.yearFrom = year("from", meta.min_year);
  state.yearTo = year("to", meta.max_year);

  if (state.yearFrom != null && state.yearTo != null && state.yearFrom > state.yearTo) {
    state.yearFrom = meta.min_year;
    state.yearTo = meta.max_year;
  }

  state.degree = params.get("degree") ?? "";
  state.positionType = params.get("type") ?? "";
  state.cursor = null;

  const place = params.get("place")?.match(/^(\d+):(.*)$/s);
  const prefix = params.get("pp")?.trim();
  state.place = place
    ? { id: Number(place[1]), name: place[2] || `#${place[1]}` }
    : prefix ? { prefix, name: prefix } : null;
  state.direction = state.place && ["arrival", "departure"].includes(params.get("dir")) ? params.get("dir") : "";

  const ids = [...new Set(
    (params.get("k") ?? "").split(",").map(Number).filter((id) => Number.isInteger(id) && id > 0)
  )].slice(0, MAX_ROUTES);

  const known = new Map(state.persons.map((entry) => [entry.person.id, entry]));
  const loaded = await Promise.all(ids.map(async (id) => {
    if (known.has(id)) return known.get(id);
    try {
      return { person: await getPerson(id), hue: null, matched: null };
    } catch (error) {
      console.error(error);
      return null;
    }
  }));

  if (loaded.includes(null)) toast(t("errors.personLinkMissing"), "error");

  state.persons = [];
  for (const entry of loaded.filter(Boolean)) {
    state.persons.push({
      person: entry.person,
      hue: entry.hue ?? pickHue(state.persons.map((e) => e.hue)),
      matched: entry.matched ?? null
    });
  }

  const active = Number(params.get("a"));
  state.activePersonId = state.persons.some((entry) => entry.person.id === active)
    ? active
    : state.persons.at(-1)?.person.id ?? null;

  state.record = null;
  state.detailRequest += 1;
  state.detail = state.persons.length ? { kind: "person" } : { kind: "overview" };
  state.tab = state.persons.length ? "route" : null;

  state.lastCount = null;
  populateFilterOptions();

  queryView.setState({
    mode: params.get("qm") === "charts" ? "charts" : "table",
    page: Math.max(0, Number.parseInt(params.get("qp") ?? "0", 10) || 0),
    sort: params.get("qs") ?? "year",
    top: Number(params.get("qt")) || 10
  }, { silent: true });

  const sourcesQuery = params.get("sq") ?? "";
  if (sourcesQuery !== state.sources.q) {
    els.sourcesQuery.value = sourcesQuery;
    state.sources.loaded = false;
  }

  const view = params.get("view");
  setView(VIEWS.includes(view) ? view : "map");

  const record = Number(params.get("r"));
  return { token: params.get("d"), record: Number.isInteger(record) && record > 0 ? record : null };
}

/* ---------- locale ---------- */

function onLocaleChange() {
  if (state.meta) populateFilterOptions();

  setStatus(state.status);
  syncTimeline();
  renderPlaceFilter();
  renderYearHint();
  renderMapMode();
  renderPersonChips();
  renderDetail();
  renderLegend();
  renderAccess();

  if (state.meta) {
    renderMapHeading();
    renderCoverage();
    queryView.render();
  }
  if (state.sources.loaded || state.sources.loading) renderSources();

  if (!els.mapEmpty.hidden) {
    if (state.status === "error") {
      setMapEmpty({ title: t("errors.dataUnavailableTitle"), text: t("errors.dataUnavailableText"), retry: bootstrap });
    } else if (state.persons.length) {
      drawRoutes();
    } else if (state.lastCount) {
      setMapEmpty(emptyReason());
    }
  }
}

/* ---------- admin view / user view ---------- */

// Source texts are for the admin view. The server enforces it when an admin
// password is configured; an admin can also preview the user view here.
function canSeeSources() {
  return state.session.role === "admin" && !state.userView;
}

function readUserViewPreference() {
  try {
    return localStorage.getItem(VIEW_PREF_KEY) === "1";
  } catch {
    return false;
  }
}

function renderAccess() {
  const { role, login_enabled: loginEnabled } = state.session;
  const admin = role === "admin";

  els.navSources.hidden = !canSeeSources();
  if (!canSeeSources() && state.view === "sources") setView("map");

  els.viewToggle.hidden = !admin;
  els.viewToggle.textContent = state.userView ? t("access.userView") : t("access.adminView");
  els.viewToggle.title = t("access.toggleHelp");
  els.viewToggle.setAttribute("aria-pressed", String(state.userView));
  els.viewToggle.classList.toggle("is-on", state.userView);

  els.accessLink.hidden = !loginEnabled;
  if (loginEnabled) {
    els.accessLink.href = admin ? "/admin/logout" : "/admin/login";
    els.accessLink.textContent = admin ? t("access.logout") : t("access.login");
  }
}

/* ---------- formatting ---------- */

function setStatus(status) {
  state.status = status;
  els.datasetStatus.dataset.status = status;
  els.datasetStatusLabel.textContent =
    status === "ready"
      ? t("status.records", { count: formatNumber(state.meta?.records ?? 0) })
      : status === "error" ? t("status.connectionError") : t("status.loading");
}

function windowLabel() {
  const { from, to } = effectiveWindow();
  if (from == null && to == null) return t("time.allPeriod");
  return `${from ?? "…"}–${to ?? "…"}`;
}

function salaryLabel(oldSalary, salary) {
  if (oldSalary == null && salary == null) return "";
  return t("salary.change", { old: oldSalary ?? "—", new: salary ?? "—" });
}

function intervalLabel(from, to) {
  if (from == null && to == null) return t("time.undated");
  if (from != null && to != null) return t("time.interval", { from, to });
  if (from != null) return t("time.after", { year: from });
  return t("time.before", { year: to });
}

function lookup(prefix, value, fallback = value) {
  if (!value) return fallback ?? "";
  const key = `${prefix}.${value}`;
  const label = t(key);
  return label === key ? fallback ?? "" : label;
}

const resolutionLabel = (value) => lookup("resolution", value);
const calendarLabel = (value) => lookup("calendar", value);

function formatCoordinate(value, positive, negative) {
  return `${Math.abs(value).toFixed(3)}° ${value >= 0 ? positive : negative}`;
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

/* Toasts: tiny rectangles delivering consequences. */

function toast(message, type = "info", duration = 4500) {
  // don't stack the same message twice
  for (const old of $$(".toast", els.toastRegion)) {
    if (old.textContent === message) old.remove();
  }

  const element = el("div", {
    class: type === "error" ? "toast is-error" : "toast",
    role: type === "error" ? "alert" : "status"
  }, message);

  els.toastRegion.append(element);
  window.setTimeout(() => element.remove(), duration);
}
