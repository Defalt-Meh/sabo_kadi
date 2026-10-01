import { t } from "./i18n.js";
import {
  arrivalPosition,
  departurePosition,
  onRouteColor,
  routeColor,
  stepPosition
} from "./colors.js";

// Geometry lives in Web-Mercator "world" units; nodes, arrows and labels are
// drawn in screen space on top, so they keep a constant size at any zoom.

const SVG_NS = "http://www.w3.org/2000/svg";
const WORLD = 8192;
const MIN_SCALE = 0.1;
const MAX_SCALE = 80;
const FIT_PADDING = 64;
const FIT_MAX_SCALE = 24;
const DRAG_THRESHOLD = 4;
const EARTH_KM = 40075;
const LABEL_FONT = '600 11px Inter, "Segoe UI", system-ui, sans-serif';

// Ottoman core lands, used when there is nothing to fit to.
const DEFAULT_BOUNDS = { west: 14, east: 48, south: 29, north: 46 };

export function project(longitude, latitude) {
  const phi = (clamp(Number(latitude), -85, 85) * Math.PI) / 180;

  return {
    x: ((Number(longitude) + 180) / 360) * WORLD,
    y: ((1 - Math.log(Math.tan(Math.PI / 4 + phi / 2)) / Math.PI) / 2) * WORLD
  };
}

function unprojectY(y) {
  const n = Math.PI - (2 * Math.PI * y) / WORLD;
  return (180 / Math.PI) * Math.atan(Math.sinh(n));
}

export class MapView {
  constructor(element, { onSelect = null, basemapUrl = "data/basemap.json" } = {}) {
    if (!(element instanceof HTMLElement)) {
      throw new TypeError("MapView requires a valid HTML element.");
    }

    this.element = element;
    this.onSelect = onSelect;
    this.mode = "empty";
    this.edges = [];
    this.nodes = [];
    this.routes = [];
    this.routeWindow = null;
    this.selectedKey = null;
    this.activeRouteId = null;
    this.view = { scale: 1, x: 0, y: 0 };
    this.width = 0;
    this.height = 0;
    this.pointers = new Map();
    this.gesture = null;
    this.suppressClick = false;
    this.frame = 0;
    this.animation = 0;
    this.labelWidths = new Map();
    this.measureContext = document.createElement("canvas").getContext("2d");

    this.buildScaffold();
    this.bindEvents();
    this.measure();
    this.fitBounds(boundsFromLonLat(DEFAULT_BOUNDS));
    this.loadBasemap(basemapUrl);

    this.resizeObserver = new ResizeObserver(() => this.onResize());
    this.resizeObserver.observe(this.element);

    window.addEventListener("kadi:themechange", () => this.redraw());
    window.addEventListener("kadi:localechange", () => this.redraw());
  }

  /* ---------- public API ---------- */

  setFlows(flows, { fit = true } = {}) {
    const valid = (flows ?? []).filter(validFlow);
    const maxCount = Math.max(1, ...valid.map((flow) => flow.count || 1));
    const places = new Map();

    this.mode = "flows";
    this.edges = valid.map((flow) => {
      const count = Math.max(1, flow.count || 1);
      const origin = project(flow.origin_longitude, flow.origin_latitude);
      const destination = project(flow.destination_longitude, flow.destination_latitude);
      const label = t("map.flowAria", {
        origin: flow.origin || t("common.unknownPlace"),
        destination: flow.destination || t("common.unknownPlace"),
        count
      });

      for (const [id, name, point] of [
        [flow.origin_id, flow.origin, origin],
        [flow.destination_id, flow.destination, destination]
      ]) {
        const key = placeKey(id, point);
        const place = places.get(key) ?? { key, id, name, ...point, weight: 0 };
        place.weight += count;
        places.set(key, place);
      }

      return {
        key: `flow:${flow.origin_id}:${flow.destination_id}`,
        ...curve(origin, destination),
        width: 1 + 5 * Math.sqrt(count / maxCount),
        opacity: 0.3 + 0.5 * Math.sqrt(count / maxCount),
        color: null,
        label,
        select: () => ({ type: "flow", data: flow })
      };
    });

    const maxWeight = Math.max(1, ...[...places.values()].map((place) => place.weight));

    this.nodes = [...places.values()].map((place) => ({
      key: place.id != null ? `place:${place.id}` : place.key,
      x: place.x,
      y: place.y,
      r: 3.5 + 4 * Math.sqrt(place.weight / maxWeight),
      label: place.name,
      title: place.name,
      priority: place.weight,
      select: () => ({ type: "place", id: place.id, data: place })
    }));

    this.draw({ fit });
  }

  setPoints(places, { fit = true } = {}) {
    const valid = (places ?? []).filter(
      (place) => Number.isFinite(place.latitude) && Number.isFinite(place.longitude)
    );
    const maxTotal = Math.max(1, ...valid.map((place) => place.arrivals + place.departures));

    this.mode = "points";
    this.edges = [];
    this.nodes = valid.map((place) => {
      const total = place.arrivals + place.departures;
      const point = project(place.longitude, place.latitude);

      return {
        key: `place:${place.id}`,
        ...point,
        r: 3 + 11 * Math.sqrt(total / maxTotal),
        label: place.name,
        title: t("map.placeTitle", {
          name: place.name,
          arrivals: place.arrivals,
          departures: place.departures
        }),
        priority: total,
        filled: true,
        select: () => ({ type: "place", id: place.id, data: place })
      };
    });

    this.draw({ fit });
  }

  // routes: [{ id, hue, appointments }], appointments in itinerary order.
  // `window` hides steps outside a year range without changing their colours
  // or numbers, so a route can be played through time.
  setRoutes(routes, { activeId = null, window = null, fit = true } = {}) {
    this.mode = "route";
    this.activeRouteId = activeId;
    this.routes = routes ?? [];
    this.routeWindow = window;
    this.buildRoutes();
    this.draw({ fit });
  }

  setActiveRoute(activeId) {
    if (this.mode !== "route") return;

    this.activeRouteId = activeId;
    this.buildRoutes();
    this.draw();
  }

  clear() {
    this.mode = "empty";
    this.edges = [];
    this.nodes = [];
    this.draw();
  }

  highlight(key) {
    this.selectedKey = key ?? null;
    this.applySelection();
    this.scheduleLayout();
  }

  hasContent() {
    return this.edges.length > 0 || this.nodes.length > 0;
  }

  fit() {
    const bounds = this.contentBounds();
    this.fitBounds(bounds ?? boundsFromLonLat(DEFAULT_BOUNDS), { animate: true });
  }

  fitRoute(id) {
    const bounds = this.contentBounds((item) => item.routeId === id);
    if (bounds) this.fitBounds(bounds, { animate: true });
  }

  zoomBy(factor) {
    this.zoomAt(this.width / 2, this.height / 2, factor, { animate: true });
  }

  destroy() {
    this.resizeObserver.disconnect();
    cancelAnimationFrame(this.frame);
    cancelAnimationFrame(this.animation);
    this.element.replaceChildren();
  }

  /* ---------- construction ---------- */

  buildScaffold() {
    this.element.replaceChildren();

    this.svg = svg("svg", { class: "map-svg", role: "group" });
    this.viewport = svg("g", { class: "map-viewport" });
    this.basemap = svg("g", { class: "map-basemap", "aria-hidden": "true" });
    this.graticule = svg("g", { class: "map-graticule", "aria-hidden": "true" });
    this.edgeLayer = svg("g", { class: "map-edges" });
    this.overlay = svg("g", { class: "map-overlay" });
    this.arrowLayer = svg("g", { class: "map-arrows", "aria-hidden": "true" });
    this.nodeLayer = svg("g", { class: "map-nodes" });
    this.labelLayer = svg("g", { class: "map-labels", "aria-hidden": "true" });

    this.viewport.append(this.basemap, this.graticule, this.edgeLayer);
    this.overlay.append(this.arrowLayer, this.nodeLayer, this.labelLayer);
    this.svg.append(this.viewport, this.overlay);

    this.scaleBar = document.createElement("div");
    this.scaleBar.className = "map-scale";
    this.scaleBar.setAttribute("aria-hidden", "true");
    this.scaleBarLine = document.createElement("span");
    this.scaleBarText = document.createElement("small");
    this.scaleBar.append(this.scaleBarLine, this.scaleBarText);

    const attribution = document.createElement("div");
    attribution.className = "map-attribution";
    attribution.textContent = "Natural Earth";

    this.element.append(this.svg, this.scaleBar, attribution);
    this.drawGraticule();
  }

  async loadBasemap(url) {
    try {
      const response = await fetch(url, { cache: "force-cache" });
      if (!response.ok) return;

      const data = await response.json();
      const rings = (list) => (list ?? []).map((ring) => pathFromLonLat(ring, true)).join("");

      this.basemap.replaceChildren(
        svg("path", { class: "map-land", d: rings(data.land) }),
        svg("path", { class: "map-lake", d: rings(data.lakes) }),
        svg("path", {
          class: "map-river",
          d: (data.rivers ?? []).map((line) => pathFromLonLat(line, false)).join("")
        })
      );
    } catch {
      // The routes still make sense on a blank sea.
    }
  }

  drawGraticule() {
    const lines = [];

    for (let lon = -30; lon <= 90; lon += 5) {
      const top = project(lon, 70);
      const bottom = project(lon, 0);
      lines.push(`M${top.x.toFixed(1)} ${top.y.toFixed(1)}V${bottom.y.toFixed(1)}`);
    }

    for (let lat = 0; lat <= 70; lat += 5) {
      const left = project(-30, lat);
      const right = project(90, lat);
      lines.push(`M${left.x.toFixed(1)} ${left.y.toFixed(1)}H${right.x.toFixed(1)}`);
    }

    this.graticule.replaceChildren(svg("path", { d: lines.join("") }));
  }

  buildRoutes() {
    const edges = [];
    const nodes = [];
    const window = this.routeWindow;
    const multiple = this.routes.length > 1;

    // the active route is drawn last so it sits on top
    const ordered = [...this.routes].sort(
      (a, b) => Number(a.id === this.activeRouteId) - Number(b.id === this.activeRouteId)
    );

    for (const route of ordered) {
      const all = route.appointments ?? [];
      const count = all.length;
      const muted = multiple && route.id !== this.activeRouteId;
      const places = new Map();
      let order = 0;

      all.forEach((appointment, index) => {
        // an outgoing kadı does not travel in this record: they were at
        // new_place and left it, so only that place belongs to their route
        const departed = appointment.role === "departed";
        const hasOrigin = !departed && hasPoint(appointment, "origin");
        const hasDestination = hasPoint(appointment, "destination");
        const visible = inWindow(appointment, window);

        // numbers follow the whole itinerary, even for hidden steps
        for (const [side, present, position] of [
          ["origin", hasOrigin, departurePosition(index, count)],
          ["destination", hasDestination, arrivalPosition(index, count)]
        ]) {
          if (!present) continue;

          const point = project(appointment[`${side}_longitude`], appointment[`${side}_latitude`]);
          const key = placeKey(appointment[`${side}_place_id`], point);

          if (!places.has(key)) {
            order += 1;
            places.set(key, {
              key,
              id: appointment[`${side}_place_id`] ?? null,
              name: appointment[side] ?? "",
              ...point,
              order,
              position,
              visible: false
            });
          }

          if (visible) places.get(key).visible = true;
        }

        if (departed || !hasOrigin || !hasDestination || !visible) return;

        const origin = project(appointment.origin_longitude, appointment.origin_latitude);
        const destination = project(appointment.destination_longitude, appointment.destination_latitude);

        edges.push({
          key: `appointment:${appointment.id}`,
          routeId: route.id,
          ...curve(origin, destination),
          width: muted ? 2 : 3,
          opacity: muted ? 0.45 : 0.95,
          color: routeColor(route.hue, stepPosition(index, count)),
          muted,
          label: `${appointment.sequence ?? index + 1}. ${appointment.origin ?? t("common.unknownPlace")} → ${appointment.destination ?? t("common.unknownPlace")}`,
          select: () => ({ type: "appointment", data: appointment, personId: route.id })
        });
      });

      for (const place of places.values()) {
        if (!place.visible) continue;

        nodes.push({
          key: place.id != null ? `place:${place.id}` : place.key,
          routeId: route.id,
          x: place.x,
          y: place.y,
          r: muted ? 7.5 : 9.5,
          label: place.name,
          title: `${place.order}. ${place.name}`,
          number: place.order,
          fill: routeColor(route.hue, place.position),
          stroke: routeColor(route.hue, 1),
          text: onRouteColor(place.position),
          muted,
          priority: (muted ? 0 : 1e6) + (1e3 - place.order),
          select: () => ({ type: "place", id: place.id, data: place, personId: route.id })
        });
      }
    }

    this.edges = edges;
    this.nodes = nodes;
  }

  /* ---------- rendering ---------- */

  redraw() {
    if (this.mode === "route") this.buildRoutes();
    this.draw();
  }

  draw({ fit = false } = {}) {
    this.edgeLayer.replaceChildren();
    this.arrowLayer.replaceChildren();
    this.nodeLayer.replaceChildren();
    this.labelLayer.replaceChildren();
    this.element.dataset.mode = this.mode;

    for (const edge of this.edges) {
      const path = svg("path", {
        class: edge.muted ? "map-edge is-muted" : "map-edge",
        d: edge.d,
        tabindex: "0",
        "data-key": edge.key,
        "aria-label": edge.label
      });

      path.style.strokeWidth = `${edge.width}px`;
      path.style.strokeOpacity = String(edge.opacity);
      if (edge.color) path.style.stroke = edge.color;

      path.append(title(edge.label));
      this.bindSelect(path, edge);
      this.edgeLayer.append(path);

      const arrow = svg("path", { class: "map-arrow", d: "M-4.5 -3.6L4 0L-4.5 3.6Z" });
      arrow.style.fillOpacity = String(Math.min(1, edge.opacity + 0.25));
      if (edge.color) arrow.style.fill = edge.color;
      if (edge.muted) arrow.classList.add("is-muted");
      edge.arrow = arrow;
      this.arrowLayer.append(arrow);
    }

    for (const node of this.nodes) {
      const group = svg("g", {
        class: [
          "map-node",
          node.filled ? "is-filled" : "",
          node.number != null ? "is-numbered" : "",
          node.muted ? "is-muted" : ""
        ].filter(Boolean).join(" "),
        tabindex: "0",
        "data-key": node.key,
        "aria-label": node.title || node.label || t("common.unknownPlace")
      });

      const hit = svg("circle", { class: "map-node-hit", r: Math.max(node.r + 5, 11) });
      const circle = svg("circle", { class: "map-node-dot", r: node.r });

      if (node.fill) circle.style.fill = node.fill;
      if (node.stroke) circle.style.stroke = node.stroke;

      group.append(title(node.title || node.label || ""), hit, circle);

      if (node.number != null) {
        const number = svg("text", { class: "map-node-number", y: "0.36em" });
        number.textContent = String(node.number);
        if (node.text) number.style.fill = node.text;
        group.append(number);
      }

      this.bindSelect(group, node);
      this.nodeLayer.append(group);
      node.element = group;

      if (node.label) {
        const label = svg("text", { class: node.muted ? "map-label is-muted" : "map-label" });
        label.textContent = node.label;
        node.labelElement = label;
        this.labelLayer.append(label);
      }
    }

    this.applySelection();

    if (fit) {
      const bounds = this.contentBounds();
      if (bounds) this.fitBounds(bounds);
    }

    this.layout();
  }

  bindSelect(element, item) {
    const activate = () => {
      if (this.suppressClick) return;
      this.highlight(item.key);
      this.onSelect?.(item.select());
    };

    element.addEventListener("click", activate);
    element.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      activate();
    });
  }

  applySelection() {
    for (const element of this.svg.querySelectorAll("[data-key]")) {
      const active = element.dataset.key === this.selectedKey;
      element.classList.toggle("is-selected", active);
      // bring the selection to the front of its layer
      if (active) element.parentNode.append(element);
    }
  }

  scheduleLayout() {
    if (this.frame) return;

    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.layout();
    });
  }

  // Re-position the screen-space overlay after any view change.
  layout() {
    const { scale, x, y } = this.view;

    this.viewport.setAttribute("transform", `matrix(${scale} 0 0 ${scale} ${x} ${y})`);

    for (const edge of this.edges) {
      const p = bezier(edge, 0.58);
      const d = bezierTangent(edge, 0.58);
      const length = Math.hypot(edge.x2 - edge.x1, edge.y2 - edge.y1) * scale;
      const angle = (Math.atan2(d.y, d.x) * 180) / Math.PI;

      edge.arrow.setAttribute(
        "transform",
        `translate(${(p.x * scale + x).toFixed(1)} ${(p.y * scale + y).toFixed(1)}) rotate(${angle.toFixed(1)})`
      );
      edge.arrow.style.display = length > 34 ? "" : "none";
    }

    for (const node of this.nodes) {
      node.sx = node.x * scale + x;
      node.sy = node.y * scale + y;
      node.element.setAttribute("transform", `translate(${node.sx.toFixed(1)} ${node.sy.toFixed(1)})`);
    }

    this.placeLabels();
    this.updateScaleBar();
  }

  // Greedy label placement: most important first, right of the node, then
  // left, then above; anything that would collide is hidden.
  placeLabels() {
    const grid = new Map();
    const cell = 80;
    const margin = 40;
    const labelled = this.nodes
      .filter((node) => node.labelElement)
      .sort((a, b) =>
        Number(b.key === this.selectedKey) - Number(a.key === this.selectedKey) ||
        (b.priority ?? 0) - (a.priority ?? 0)
      );

    const cellsFor = (box) => {
      const keys = [];
      for (let cx = Math.floor(box.x / cell); cx <= Math.floor((box.x + box.w) / cell); cx += 1) {
        for (let cy = Math.floor(box.y / cell); cy <= Math.floor((box.y + box.h) / cell); cy += 1) {
          keys.push(`${cx}:${cy}`);
        }
      }
      return keys;
    };

    const collides = (box, ignore) =>
      cellsFor(box).some((key) =>
        (grid.get(key) ?? []).some(
          (other) =>
            other.owner !== ignore &&
            box.x < other.x + other.w &&
            box.x + box.w > other.x &&
            box.y < other.y + other.h &&
            box.y + box.h > other.y
        )
      );

    const occupy = (box) => {
      for (const key of cellsFor(box)) {
        if (!grid.has(key)) grid.set(key, []);
        grid.get(key).push(box);
      }
    };

    // the dots of important places are obstacles for labels too
    for (const node of labelled.slice(0, 400)) {
      occupy({ x: node.sx - node.r, y: node.sy - node.r, w: node.r * 2, h: node.r * 2, owner: node });
    }

    let shown = 0;
    const budget = this.mode === "points" ? 140 : 220;

    for (const node of labelled) {
      const label = node.labelElement;
      const onScreen =
        node.sx > -margin && node.sx < this.width + margin &&
        node.sy > -margin && node.sy < this.height + margin;

      if (!onScreen || shown >= budget) {
        label.style.display = "none";
        continue;
      }

      const width = this.labelWidth(node.label);
      const gap = node.r + 4;
      const candidates = [
        { x: node.sx + gap, y: node.sy - 7, anchor: "start" },
        { x: node.sx - gap - width, y: node.sy - 7, anchor: "end" },
        { x: node.sx - width / 2, y: node.sy - node.r - 17, anchor: "middle" }
      ];

      const selected = node.key === this.selectedKey;
      const spot =
        candidates.find((c) => !collides({ x: c.x - 2, y: c.y, w: width + 4, h: 14 }, node)) ??
        (selected ? candidates[0] : null);

      if (!spot) {
        label.style.display = "none";
        continue;
      }

      occupy({ x: spot.x - 2, y: spot.y, w: width + 4, h: 14, owner: node });
      shown += 1;

      const textX =
        spot.anchor === "start" ? spot.x : spot.anchor === "end" ? spot.x + width : spot.x + width / 2;

      label.style.display = "";
      label.setAttribute("x", textX.toFixed(1));
      label.setAttribute("y", (spot.y + 10.5).toFixed(1));
      label.setAttribute("text-anchor", spot.anchor);
    }
  }

  labelWidth(text) {
    if (!this.labelWidths.has(text)) {
      this.measureContext.font = LABEL_FONT;
      this.labelWidths.set(text, Math.ceil(this.measureContext.measureText(text).width));
    }

    return this.labelWidths.get(text);
  }

  updateScaleBar() {
    const centerLat = unprojectY((this.height / 2 - this.view.y) / this.view.scale);
    const kmPerPixel =
      ((EARTH_KM / WORLD) * Math.cos((centerLat * Math.PI) / 180)) / this.view.scale;
    const target = kmPerPixel * 110;
    const magnitude = 10 ** Math.floor(Math.log10(target));
    const nice = [1, 2, 5, 10].map((m) => m * magnitude).filter((v) => v <= target).pop() ?? magnitude;

    this.scaleBarLine.style.width = `${Math.round(nice / kmPerPixel)}px`;
    this.scaleBarText.textContent = `${nice.toLocaleString()} km`;
  }

  /* ---------- view ---------- */

  measure() {
    const rect = this.element.getBoundingClientRect();
    this.width = Math.max(1, Math.round(rect.width));
    this.height = Math.max(1, Math.round(rect.height));
    this.svg.setAttribute("width", this.width);
    this.svg.setAttribute("height", this.height);
    this.svg.setAttribute("viewBox", `0 0 ${this.width} ${this.height}`);
  }

  onResize() {
    const oldWidth = this.width;
    const oldHeight = this.height;

    this.measure();
    if (oldWidth === this.width && oldHeight === this.height) return;

    // a map that was hidden (0×0) has no meaningful view yet: fit instead
    if (oldWidth <= 1 || oldHeight <= 1) {
      this.fitBounds(this.contentBounds() ?? boundsFromLonLat(DEFAULT_BOUNDS));
      return;
    }

    // keep the same world point in the middle
    this.view.x += (this.width - oldWidth) / 2;
    this.view.y += (this.height - oldHeight) / 2;
    this.layout();
  }

  contentBounds(include = () => true) {
    const xs = [];
    const ys = [];

    for (const node of this.nodes) {
      if (!include(node)) continue;
      xs.push(node.x);
      ys.push(node.y);
    }

    for (const edge of this.edges) {
      if (!include(edge)) continue;
      xs.push(edge.x1, edge.x2, edge.cx);
      ys.push(edge.y1, edge.y2, edge.cy);
    }

    if (!xs.length) return null;

    return {
      minX: Math.min(...xs),
      maxX: Math.max(...xs),
      minY: Math.min(...ys),
      maxY: Math.max(...ys)
    };
  }

  fitBounds(bounds, { animate = false } = {}) {
    const usableWidth = Math.max(this.width - FIT_PADDING * 2, 40);
    const usableHeight = Math.max(this.height - FIT_PADDING * 2, 40);
    const scale = clamp(
      Math.min(
        usableWidth / Math.max(bounds.maxX - bounds.minX, 1e-6),
        usableHeight / Math.max(bounds.maxY - bounds.minY, 1e-6)
      ),
      MIN_SCALE,
      FIT_MAX_SCALE
    );

    this.setView(
      {
        scale,
        x: this.width / 2 - ((bounds.minX + bounds.maxX) / 2) * scale,
        y: this.height / 2 - ((bounds.minY + bounds.maxY) / 2) * scale
      },
      { animate }
    );
  }

  zoomAt(px, py, factor, { animate = false } = {}) {
    const base = animate && this.target ? this.target : this.view;
    const scale = clamp(base.scale * factor, MIN_SCALE, MAX_SCALE);
    const worldX = (px - base.x) / base.scale;
    const worldY = (py - base.y) / base.scale;

    this.setView({ scale, x: px - worldX * scale, y: py - worldY * scale }, { animate });
  }

  setView(next, { animate = false } = {}) {
    cancelAnimationFrame(this.animation);
    this.target = null;

    const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

    if (!animate || reduceMotion) {
      this.view = next;
      this.scheduleLayout();
      return;
    }

    const start = { ...this.view };
    const began = performance.now();
    const duration = 280;
    const centerX = this.width / 2;
    const centerY = this.height / 2;
    const fromWorld = { x: (centerX - start.x) / start.scale, y: (centerY - start.y) / start.scale };
    const toWorld = { x: (centerX - next.x) / next.scale, y: (centerY - next.y) / next.scale };

    this.target = next;

    const step = (now) => {
      const k = easeOut(Math.min(1, (now - began) / duration));
      // interpolate zoom in log space so it feels even
      const scale = Math.exp(Math.log(start.scale) + (Math.log(next.scale) - Math.log(start.scale)) * k);
      const worldX = fromWorld.x + (toWorld.x - fromWorld.x) * k;
      const worldY = fromWorld.y + (toWorld.y - fromWorld.y) * k;

      this.view = { scale, x: centerX - worldX * scale, y: centerY - worldY * scale };
      this.layout();

      if (k < 1) {
        this.animation = requestAnimationFrame(step);
      } else {
        this.view = next;
        this.target = null;
        this.layout();
      }
    };

    this.animation = requestAnimationFrame(step);
  }

  /* ---------- input ---------- */

  bindEvents() {
    const el = this.element;

    el.addEventListener("wheel", (event) => {
      event.preventDefault();
      cancelAnimationFrame(this.animation);
      this.target = null;

      const rect = el.getBoundingClientRect();
      const speed = event.ctrlKey ? 0.01 : 0.0018;
      const delta = event.deltaMode === 1 ? event.deltaY * 16 : event.deltaY;
      this.zoomAt(event.clientX - rect.left, event.clientY - rect.top, Math.exp(-delta * speed));
    }, { passive: false });

    el.addEventListener("pointerdown", (event) => {
      if (event.pointerType === "mouse" && event.button !== 0) return;

      cancelAnimationFrame(this.animation);
      this.target = null;
      this.pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (this.pointers.size === 1) this.suppressClick = false;
      this.startGesture();
    });

    el.addEventListener("pointermove", (event) => {
      if (!this.pointers.has(event.pointerId)) return;

      this.pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
      this.moveGesture(event);
    });

    const end = (event) => {
      if (!this.pointers.has(event.pointerId)) return;

      this.pointers.delete(event.pointerId);
      if (el.hasPointerCapture?.(event.pointerId)) el.releasePointerCapture(event.pointerId);
      if (!this.pointers.size) el.classList.remove("is-dragging");
      // carry on with the remaining finger, if any
      this.startGesture();
    };

    el.addEventListener("pointerup", end);
    el.addEventListener("pointercancel", end);
    el.addEventListener("lostpointercapture", end);

    el.addEventListener("dblclick", (event) => {
      if (event.target.closest?.("[data-key]")) return;
      const rect = el.getBoundingClientRect();
      this.zoomAt(event.clientX - rect.left, event.clientY - rect.top, event.shiftKey ? 0.5 : 2, { animate: true });
    });

    el.addEventListener("keydown", (event) => {
      if (event.target !== el) return;

      const step = event.shiftKey ? 160 : 60;
      const moves = {
        ArrowLeft: [step, 0],
        ArrowRight: [-step, 0],
        ArrowUp: [0, step],
        ArrowDown: [0, -step]
      };

      if (moves[event.key]) {
        event.preventDefault();
        this.setView({
          ...this.view,
          x: this.view.x + moves[event.key][0],
          y: this.view.y + moves[event.key][1]
        });
      } else if (event.key === "+" || event.key === "=") {
        event.preventDefault();
        this.zoomBy(1.5);
      } else if (event.key === "-" || event.key === "_") {
        event.preventDefault();
        this.zoomBy(1 / 1.5);
      } else if (event.key === "0") {
        event.preventDefault();
        this.fit();
      }
    });
  }

  startGesture() {
    const points = [...this.pointers.values()];

    if (!points.length) {
      this.gesture = null;
      return;
    }

    this.gesture = {
      start: midpoint(points),
      view: { ...this.view },
      distance: points.length > 1 ? distance(points[0], points[1]) : null,
      moved: this.gesture?.moved ?? false
    };
  }

  moveGesture(event) {
    if (!this.gesture) return;

    const points = [...this.pointers.values()];
    const center = midpoint(points);
    const dx = center.x - this.gesture.start.x;
    const dy = center.y - this.gesture.start.y;

    if (!this.gesture.moved && points.length === 1 && Math.hypot(dx, dy) < DRAG_THRESHOLD) return;

    if (!this.gesture.moved) {
      this.gesture.moved = true;
      // capture only once it is a drag, so plain clicks still reach nodes
      this.element.setPointerCapture?.(event.pointerId);
      this.element.classList.add("is-dragging");
    }

    this.suppressClick = true;

    const rect = this.element.getBoundingClientRect();
    const start = this.gesture.view;
    let scale = start.scale;

    if (points.length > 1 && this.gesture.distance) {
      scale = clamp(
        start.scale * (distance(points[0], points[1]) / this.gesture.distance),
        MIN_SCALE,
        MAX_SCALE
      );
    }

    const originX = this.gesture.start.x - rect.left;
    const originY = this.gesture.start.y - rect.top;
    const worldX = (originX - start.x) / start.scale;
    const worldY = (originY - start.y) / start.scale;

    this.view = {
      scale,
      x: originX + dx - worldX * scale,
      y: originY + dy - worldY * scale
    };

    this.scheduleLayout();
  }
}

/* ---------- helpers ---------- */

function curve(a, b) {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const length = Math.hypot(dx, dy);

  if (length < 1e-6) {
    // same place: a small loop so the record is still visible and clickable
    const r = 1.5;
    return {
      x1: a.x, y1: a.y, x2: b.x, y2: b.y, cx: a.x, cy: a.y - r * 1.5,
      d: `M${a.x} ${a.y}C${a.x + r * 2} ${a.y - r * 2} ${a.x - r * 2} ${a.y - r * 2} ${a.x} ${a.y}`
    };
  }

  const bend = Math.min(length * 0.16, 60);
  const cx = (a.x + b.x) / 2 - (dy / length) * bend;
  const cy = (a.y + b.y) / 2 + (dx / length) * bend;

  return { x1: a.x, y1: a.y, x2: b.x, y2: b.y, cx, cy, d: `M${a.x} ${a.y}Q${cx} ${cy} ${b.x} ${b.y}` };
}

function bezier(e, s) {
  const u = 1 - s;
  return {
    x: u * u * e.x1 + 2 * u * s * e.cx + s * s * e.x2,
    y: u * u * e.y1 + 2 * u * s * e.cy + s * s * e.y2
  };
}

function bezierTangent(e, s) {
  return {
    x: 2 * (1 - s) * (e.cx - e.x1) + 2 * s * (e.x2 - e.cx),
    y: 2 * (1 - s) * (e.cy - e.y1) + 2 * s * (e.y2 - e.cy)
  };
}

function pathFromLonLat(points, closed) {
  let d = "";

  points.forEach(([lon, lat], index) => {
    const p = project(lon, lat);
    d += `${index ? "L" : "M"}${p.x.toFixed(1)} ${p.y.toFixed(1)}`;
  });

  return closed && d ? `${d}Z` : d;
}

function boundsFromLonLat({ west, east, south, north }) {
  const a = project(west, north);
  const b = project(east, south);
  return { minX: a.x, minY: a.y, maxX: b.x, maxY: b.y };
}

function placeKey(id, point) {
  return id != null ? `id:${id}` : `xy:${point.x.toFixed(2)}:${point.y.toFixed(2)}`;
}

function hasPoint(appointment, side) {
  return Number.isFinite(appointment?.[`${side}_latitude`]) &&
    Number.isFinite(appointment?.[`${side}_longitude`]);
}

function inWindow(appointment, window) {
  if (!window) return true;

  const year = appointment.year_numeric;

  if (!Number.isFinite(year)) return window.includeUndated !== false;
  if (window.from != null && year < window.from) return false;
  if (window.to != null && year > window.to) return false;

  return true;
}

function validFlow(flow) {
  return [
    flow?.origin_latitude,
    flow?.origin_longitude,
    flow?.destination_latitude,
    flow?.destination_longitude
  ].every(Number.isFinite);
}

function midpoint(points) {
  return {
    x: points.reduce((sum, p) => sum + p.x, 0) / points.length,
    y: points.reduce((sum, p) => sum + p.y, 0) / points.length
  };
}

function distance(a, b) {
  return Math.max(1, Math.hypot(a.x - b.x, a.y - b.y));
}

function easeOut(k) {
  return 1 - (1 - k) ** 3;
}

function title(text) {
  const element = svg("title");
  element.textContent = text;
  return element;
}

function svg(name, attributes = {}) {
  const element = document.createElementNS(SVG_NS, name);

  for (const [key, value] of Object.entries(attributes)) {
    if (value === undefined || value === null) continue;
    element.setAttribute(key, String(value));
  }

  return element;
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}
