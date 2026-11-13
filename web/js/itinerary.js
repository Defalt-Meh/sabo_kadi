// One numbering for a kadı's itinerary, shared by the side panel and the map,
// so "stop 21" means the same thing in both places.
//
// The API delivers the records in itinerary order (year first, then the
// old place → new place chain; see src/api/persons.rs `order_journey`). Each
// record becomes stops:
//
//   appointed / both  A → B   : a start stop at A when the chain breaks there
//                               (and A is known), then an arrival stop at B
//   departed (replaced at B)  : nothing new when the kadı was already at B;
//                               a stop at B when that is where we first meet them
//
// A stop is `linked` when a record connects it to the stop before it (the
// kadı demonstrably moved from there to here). Everything else is a gap in the
// records, drawn dashed on the map.

export function buildItinerary(appointments = []) {
  const stops = [];
  const stopOf = new Map(); // appointment id → stop number

  const add = (fields) => {
    const stop = { n: stops.length + 1, events: [], ...fields };
    stops.push(stop);
    return stop;
  };

  appointments.forEach((appointment, index) => {
    const breaks = index === 0 || appointment.follows_previous === false;

    if (appointment.role === "departed") {
      const current = stops.at(-1);

      if (breaks || !current) {
        const stop = add({
          kind: "departed",
          appointment,
          linked: false,
          gap: index > 0,
          ...placeOf(appointment, "destination")
        });
        stopOf.set(appointment.id, stop.n);
      } else {
        current.events.push(appointment);
        stopOf.set(appointment.id, current.n);
      }
      return;
    }

    let linked = !breaks && stops.length > 0;
    let gap = breaks && index > 0;

    if (breaks && (appointment.origin || appointment.origin_place_id != null)) {
      add({
        kind: index === 0 ? "start" : "restart",
        appointment,
        linked: false,
        gap,
        ...placeOf(appointment, "origin")
      });
      linked = true;
      gap = false;
    }

    const stop = add({ kind: "arrival", appointment, linked, gap, ...placeOf(appointment, "destination") });
    stopOf.set(appointment.id, stop.n);
  });

  return { stops, stopOf };
}

function placeOf(appointment, side) {
  const lat = appointment[`${side}_latitude`];
  const lon = appointment[`${side}_longitude`];
  const placeId = appointment[`${side}_place_id`] ?? null;
  const name = appointment[side] ?? null;

  return {
    name,
    placeId,
    latitude: Number.isFinite(lat) ? lat : null,
    longitude: Number.isFinite(lon) ? lon : null,
    key: placeId != null ? `id:${placeId}` : `raw:${String(name ?? "").toLocaleLowerCase("tr")}`
  };
}

export function hasPoint(stop) {
  return stop.latitude != null && stop.longitude != null;
}

// "3·21" — the stop numbers at one place, shortened for a map badge.
export function badgeText(numbers, max = 3) {
  if (numbers.length <= max) return numbers.join("·");
  return `${numbers.slice(0, max).join("·")}+${numbers.length - max}`;
}
