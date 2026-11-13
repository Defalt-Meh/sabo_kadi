// The query page: every record that matches the sidebar filters, as a sortable
// table (with CSV export) and as charts computed from the same records.

import { appointmentsCsvUrl, getAppointments, getStats, isAbort } from "./api.js";
import { formatDate, formatYear, gregorianLabel, hijriToGregorianSpan } from "./calendar.js";
import { t } from "./i18n.js";
import { $, $$, el, formatNumber } from "./dom.js";

const PAGE_SIZE = 50;
const TIME_BY_YEAR_MAX_SPAN = 60;

const COLUMNS = [
  { key: "date", sort: "year", desc: "year_desc", label: "query.colDate" },
  { key: "old_kadi", sort: "old_kadi", label: "record.oldKadi" },
  { key: "new_kadi", sort: "new_kadi", label: "record.newKadi" },
  { key: "origin", sort: "origin", label: "record.oldPlace" },
  { key: "destination", sort: "destination", label: "record.newPlace" },
  { key: "degree", sort: "degree", label: "filters.degree" },
  { key: "position_type", sort: "position_type", label: "filters.positionType" },
  { key: "record", label: "query.colRecord" }
];

const SORTS = new Set(COLUMNS.flatMap((column) => [column.sort, column.desc]).filter(Boolean));

// The charts: what to group by, and what a click on a bar opens.
const CHARTS = [
  { by: "destination", title: "charts.destination", entity: "place" },
  { by: "origin", title: "charts.origin", entity: "place" },
  { by: "time", title: "charts.time", kind: "time" },
  { by: "new_kadi", title: "charts.newKadi", entity: "person" },
  { by: "degree", title: "charts.degree" },
  { by: "position_type", title: "charts.positionType" }
];

export function createQueryView({ getFilters, describeFilters, onRecord, onPlace, onPerson, onPeriod, onChange }) {
  const els = {
    count: $("#query-count"),
    filters: $("#query-filters"),
    modeButtons: $$("[data-query-mode]"),
    csv: $("#query-csv"),
    tablePanel: $("#query-table-panel"),
    head: $("#query-head"),
    rows: $("#query-rows"),
    prev: $("#query-prev"),
    next: $("#query-next"),
    range: $("#query-range"),
    chartsPanel: $("#query-charts-panel"),
    top: $("#chart-top"),
    grid: $("#charts-grid"),
    status: $("#query-status")
  };

  const state = {
    mode: "table",
    page: 0,
    sort: "year",
    top: 10,
    table: null, // { items, total }
    charts: null, // Map(by → stats)
    loading: false,
    error: null,
    request: 0,
    controller: null,
    stale: true
  };

  for (const button of els.modeButtons) {
    button.addEventListener("click", () => setState({ mode: button.dataset.queryMode }));
  }
  els.prev.addEventListener("click", () => setState({ page: Math.max(0, state.page - 1) }));
  els.next.addEventListener("click", () => setState({ page: state.page + 1 }));
  els.top.addEventListener("change", () => setState({ top: Number(els.top.value) || 10 }));

  /* ---------- state ---------- */

  function getState() {
    return { mode: state.mode, page: state.page, sort: state.sort, top: state.top };
  }

  // Apply a (partial) state; `silent` skips the onChange notification (used
  // when restoring from the URL).
  function setState(next, { silent = false } = {}) {
    const mode = next.mode === "charts" ? "charts" : next.mode === "table" ? "table" : state.mode;
    const sort = next.sort && SORTS.has(next.sort) ? next.sort : state.sort;
    const top = [10, 20, 50].includes(next.top) ? next.top : state.top;
    const page = Number.isInteger(next.page) && next.page >= 0 ? next.page : state.page;

    const changed = mode !== state.mode || sort !== state.sort || top !== state.top || page !== state.page;
    if (sort !== state.sort) state.table = null;
    if (top !== state.top) state.charts = null;

    Object.assign(state, { mode, sort, top, page: sort !== state.sort ? 0 : page });
    if (changed) state.stale = true;

    renderMode();
    if (!silent && changed) onChange?.(getState());
    if (changed && isVisible()) load();
  }

  // The filters changed: start again from the first page.
  function invalidate() {
    state.page = 0;
    state.table = null;
    state.charts = null;
    state.stale = true;
    if (isVisible()) load();
  }

  function isVisible() {
    return !$("#query-view").hidden;
  }

  function activate() {
    renderMode();
    if (state.stale) load();
    else render();
  }

  /* ---------- loading ---------- */

  async function load() {
    state.controller?.abort();
    const controller = new AbortController();
    const request = ++state.request;
    const filters = getFilters();

    state.controller = controller;
    state.loading = true;
    state.error = null;
    render();

    try {
      if (state.mode === "table") {
        state.table = await getAppointments(
          { ...filters, sort: state.sort, limit: PAGE_SIZE, offset: state.page * PAGE_SIZE },
          { signal: controller.signal }
        );
      } else {
        const results = await Promise.all(
          CHARTS.map((chart) =>
            getStats(chartBy(chart, filters), { ...filters, limit: state.top }, { signal: controller.signal }))
        );
        state.charts = new Map(CHARTS.map((chart, index) => [chart.by, results[index]]));
      }
      if (request !== state.request) return;
      state.stale = false;
    } catch (error) {
      if (request !== state.request || isAbort(error)) return;
      console.error(error);
      state.error = error;
    } finally {
      if (request === state.request) {
        state.loading = false;
        render();
      }
    }
  }

  // Short windows are drawn year by year, long ones by decade.
  function chartBy(chart, filters) {
    if (chart.kind !== "time") return chart.by;
    const span = (filters.year_to ?? Infinity) - (filters.year_from ?? -Infinity);
    return span <= TIME_BY_YEAR_MAX_SPAN ? "year" : "decade";
  }

  /* ---------- rendering ---------- */

  function renderMode() {
    for (const button of els.modeButtons) {
      const active = button.dataset.queryMode === state.mode;
      button.classList.toggle("is-active", active);
      button.setAttribute("aria-pressed", String(active));
    }
    els.tablePanel.hidden = state.mode !== "table";
    els.chartsPanel.hidden = state.mode !== "charts";
    els.top.value = String(state.top);
  }

  function render() {
    const filters = getFilters();

    renderMode();
    els.filters.textContent = describeFilters();
    els.csv.href = appointmentsCsvUrl({ ...filters, sort: state.sort });
    els.csv.title = t("query.csvHelp");

    const total = state.mode === "table"
      ? state.table?.total
      : state.charts?.values().next().value?.records;
    els.count.textContent = total == null ? "" : t("query.count", { count: formatNumber(total) });

    els.status.hidden = !(state.loading || state.error);
    els.status.textContent = state.error ? t("errors.queryLoad") : state.loading ? t("common.loading") : "";
    els.status.classList.toggle("is-error", Boolean(state.error));

    if (state.mode === "table") renderTable();
    else renderCharts(filters);
  }

  function renderTable() {
    renderHead();
    els.rows.replaceChildren();

    const data = state.table;
    const items = data?.items ?? [];

    if (data && !items.length) {
      els.rows.append(el("tr", {}, el("td", { colspan: COLUMNS.length, class: "data-empty" }, t("query.empty"))));
    }

    for (const appointment of items) {
      els.rows.append(el("tr", {},
        el("td", { class: "is-date" }, formatDate(appointment.year_numeric, appointment.year_label) ?? t("time.unknown")),
        el("td", {}, entityLink(appointment.old_person?.id, appointment.old_person?.name || appointment.raw_old_kadi, onPerson)),
        el("td", {}, entityLink(appointment.new_person?.id, appointment.new_person?.name || appointment.raw_new_kadi, onPerson)),
        el("td", {}, entityLink(appointment.origin_place_id, appointment.origin, onPlace)),
        el("td", {}, entityLink(appointment.destination_place_id, appointment.destination, onPlace)),
        el("td", {}, appointment.degree ?? ""),
        el("td", {}, appointment.position_type ?? ""),
        el("td", { class: "is-doc" }, recordButton(appointment))
      ));
    }

    const total = data?.total ?? 0;
    const first = total ? state.page * PAGE_SIZE + 1 : 0;
    const last = Math.min(total, (state.page + 1) * PAGE_SIZE);

    els.range.textContent = data
      ? t("query.range", { first: formatNumber(first), last: formatNumber(last), total: formatNumber(total) })
      : "";
    els.prev.disabled = state.loading || state.page === 0;
    els.next.disabled = state.loading || last >= total;
  }

  function renderHead() {
    const row = el("tr");

    for (const column of COLUMNS) {
      if (!column.sort) {
        row.append(el("th", { scope: "col" }, t(column.label)));
        continue;
      }

      const ascending = state.sort === column.sort;
      const descending = column.desc && state.sort === column.desc;
      const sorted = ascending || descending;
      const button = el("button", { type: "button", class: "th-sort" },
        t(column.label),
        el("span", { class: "th-sort-icon", "aria-hidden": "true" }, sorted ? (descending ? "▼" : "▲") : "↕"));

      button.addEventListener("click", () => {
        const next = ascending && column.desc ? column.desc : column.sort;
        setState({ sort: next, page: 0 });
      });

      row.append(el("th", {
        scope: "col",
        "aria-sort": sorted ? (descending ? "descending" : "ascending") : "none"
      }, button));
    }

    els.head.replaceChildren(row);
  }

  function recordButton(appointment) {
    const label = appointment.source_doc_id ?? t("details.record");
    const button = el("button", { type: "button", class: "inline-link", title: label }, label);
    button.addEventListener("click", () => onRecord(appointment));
    return button;
  }

  function renderCharts(filters) {
    els.grid.replaceChildren();
    if (!state.charts) return;

    for (const chart of CHARTS) {
      const data = state.charts.get(chart.by);
      if (!data) continue;

      const by = chartBy(chart, filters);
      const card = el("article", { class: chart.kind === "time" ? "chart-card is-wide" : "chart-card" });
      const title = chart.kind === "time" ? t(by === "year" ? "charts.timeYear" : "charts.timeDecade") : t(chart.title);

      card.append(el("h2", { class: "chart-title" }, title));

      if (!data.items.length) {
        card.append(el("p", { class: "chart-empty" }, t("charts.empty")));
      } else if (chart.kind === "time") {
        card.append(timeChart(data, by));
      } else {
        card.append(barChart(data, chart));
      }

      card.append(el("p", { class: "chart-foot" }, chartFoot(data, chart)));
      els.grid.append(card);
    }
  }

  function chartFoot(data, chart) {
    if (chart.kind === "time") {
      return data.undated
        ? t("charts.undated", { count: formatNumber(data.undated) })
        : t("charts.records", { count: formatNumber(data.records) });
    }
    return t("charts.shown", {
      shown: formatNumber(data.items.length),
      groups: formatNumber(data.groups),
      records: formatNumber(data.records)
    });
  }

  // Ranked horizontal bars: label, bar, value. One series, one colour; the
  // value is printed at the bar end, the full label is in the tooltip.
  function barChart(data, chart) {
    const max = Math.max(1, ...data.items.map((item) => item.count));
    const list = el("ol", { class: "bar-chart" });

    for (const item of data.items) {
      const label = item.label ?? t("charts.blank");
      const tooltip = t("charts.tooltip", { label, count: formatNumber(item.count) });
      const clickable = chart.entity && item.id != null;
      const row = el(clickable ? "button" : "div", {
        class: "bar-row",
        title: tooltip,
        type: clickable ? "button" : null
      },
        el("span", { class: "bar-label" }, label),
        el("span", { class: "bar-track", "aria-hidden": "true" },
          el("span", { class: "bar", style: `width: ${(item.count / max) * 100}%` })),
        el("span", { class: "bar-value" }, formatNumber(item.count)));

      if (clickable) {
        row.addEventListener("click", () => (chart.entity === "place" ? onPlace(item.id) : onPerson(item.id)));
      }

      list.append(el("li", {}, row));
    }

    return list;
  }

  // Records per year or decade as columns. Every bucket between the first and
  // the last is drawn, so gaps in the record show as gaps.
  function timeChart(data, by) {
    const step = by === "year" ? 1 : 10;
    const counts = new Map(data.items.map((item) => [item.value, item.count]));
    const first = data.items[0].value;
    const last = data.items.at(-1).value;
    const buckets = [];
    for (let value = first; value <= last; value += step) buckets.push({ value, count: counts.get(value) ?? 0 });

    const max = Math.max(1, ...buckets.map((bucket) => bucket.count));
    const every = Math.max(1, Math.ceil(buckets.length / 12));
    const columns = el("div", { class: "column-chart", role: "list" });

    buckets.forEach((bucket, index) => {
      const end = bucket.value + step - 1;
      const range = step === 1 ? String(bucket.value) : `${bucket.value}–${end}`;
      const gregorian = step === 1
        ? gregorianLabel(bucket.value)
        : `${hijriToGregorianSpan(bucket.value)[0]}–${hijriToGregorianSpan(end)[1]}`;
      const tooltip = t("charts.timeTooltip", { range, gregorian, count: formatNumber(bucket.count) });

      const column = el("button", { type: "button", class: "column", role: "listitem", title: tooltip, "aria-label": tooltip },
        el("span", { class: "column-bar", style: `height: ${(bucket.count / max) * 100}%` }));
      column.addEventListener("click", () => onPeriod(bucket.value, end));

      const tick = el("span", { class: "column-tick", "aria-hidden": "true" },
        index % every === 0 ? formatYear(bucket.value).replace(/^≈ /, "") : "");

      columns.append(el("div", { class: "column-slot" }, column, tick));
    });

    return el("div", { class: "column-wrap" },
      el("span", { class: "column-max", "aria-hidden": "true" }, formatNumber(max)),
      columns);
  }

  return { activate, invalidate, render, setState, getState };
}

function entityLink(id, name, open) {
  if (!name) return "";
  if (id == null) return name;

  const button = el("button", { type: "button", class: "inline-link" }, name);
  button.addEventListener("click", () => open(id));
  return button;
}
