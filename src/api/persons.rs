//! `GET /api/v1/persons` and `GET /api/v1/persons/{id}`.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};

use super::{resolve_page, Page, PersonRef, PlaceRef, Query};
use crate::app::AppState;
use crate::error::{ApiError, ApiResult};
use crate::normalize::normalize_key;

const ALLOWED_STATUS: &[&str] = &["provisional", "under_review", "confirmed", "split_needed"];

#[derive(Debug, Deserialize)]
pub struct PersonQuery {
    /// Free-text match against the (normalized) name.
    pub query: Option<String>,
    /// Alias for `query`.
    pub q: Option<String>,
    pub resolution_status: Option<String>,
    pub has_wikidata: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl PersonQuery {
    fn search_term(&self) -> Option<&str> {
        self.query.as_deref().or(self.q.as_deref())
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PersonListItem {
    pub id: i64,
    pub canonical_name: String,
    pub normalized_name: String,
    pub wikidata_qid: Option<String>,
    pub resolution_status: String,
    pub appointment_count: i64,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<PersonQuery>,
) -> ApiResult<Json<Page<PersonListItem>>> {
    let page = resolve_page(q.limit, q.offset, &state)?;

    if let Some(status) = &q.resolution_status {
        if !ALLOWED_STATUS.contains(&status.as_str()) {
            return Err(ApiError::bad_request(format!(
                "`resolution_status` must be one of: {}",
                ALLOWED_STATUS.join(", ")
            )));
        }
    }
    let name_needle = q.search_term().and_then(normalize_key);

    let mut count_qb: QueryBuilder<Postgres> = QueryBuilder::new("SELECT count(*) FROM persons p");
    apply_filters(&mut count_qb, &q, name_needle.as_deref());
    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.pool).await?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT p.id, p.canonical_name, p.normalized_name, p.wikidata_qid, p.resolution_status, \
         (SELECT count(*) FROM appointments a \
          WHERE a.old_person_id = p.id OR a.new_person_id = p.id) AS appointment_count \
         FROM persons p",
    );
    apply_filters(&mut qb, &q, name_needle.as_deref());
    qb.push(" ORDER BY p.canonical_name, p.id LIMIT ");
    qb.push_bind(page.limit);
    qb.push(" OFFSET ");
    qb.push_bind(page.offset);

    let items = qb
        .build_query_as::<PersonListItem>()
        .fetch_all(&state.pool)
        .await?;

    Ok(Json(Page::new(items, total, page)))
}

fn apply_filters<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    q: &'a PersonQuery,
    name_needle: Option<&'a str>,
) {
    let mut first = true;
    let mut clause = |qb: &mut QueryBuilder<'a, Postgres>| {
        qb.push(if first { " WHERE " } else { " AND " });
        first = false;
    };

    if let Some(needle) = name_needle {
        clause(qb);
        qb.push("p.normalized_name LIKE '%' || ")
            .push_bind(needle)
            .push(" || '%'");
    }
    if let Some(status) = &q.resolution_status {
        clause(qb);
        qb.push("p.resolution_status = ").push_bind(status.as_str());
    }
    match q.has_wikidata {
        Some(true) => {
            clause(qb);
            qb.push("p.wikidata_qid IS NOT NULL");
        }
        Some(false) => {
            clause(qb);
            qb.push("p.wikidata_qid IS NULL");
        }
        None => {}
    }
}

// ---------------------------------------------------------------------------
// Detail
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PersonDetail {
    pub id: i64,
    pub canonical_name: String,
    pub normalized_name: String,
    pub wikidata_qid: Option<String>,
    pub resolution_status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Every appointment record this name grouping appears in, in itinerary
    /// order: by `year_numeric`, with same-year and undated steps ordered by
    /// following the `old_place -> new_place` chain. See [`order_journey`].
    pub journey: Vec<JourneyStep>,
}

#[derive(Debug, Serialize)]
pub struct JourneyStep {
    /// 1-based position in the itinerary.
    pub sequence: usize,
    /// `true` when this step starts where the previous one ended, i.e. the
    /// chain is unbroken. `false` for the first step and after any gap.
    pub follows_previous: bool,
    pub appointment_id: i64,
    pub source_record_id: i64,
    pub source_doc_id: String,
    /// `appointed` (this person is the incoming kadı), `departed` (outgoing),
    /// or `both`.
    pub role: &'static str,
    pub year_original: Option<String>,
    pub year_numeric: Option<i32>,
    pub calendar: String,
    pub degree: Option<String>,
    pub position_type: Option<String>,
    pub period: Option<String>,
    pub salary: Option<String>,
    pub old_salary: Option<String>,
    pub region_raw: Option<String>,
    pub raw_old_place: Option<String>,
    pub raw_new_place: Option<String>,
    pub raw_old_kadi: Option<String>,
    pub raw_new_kadi: Option<String>,
    /// Outgoing kadı of the record (the predecessor when `role` is `appointed`).
    pub old_person: Option<PersonRef>,
    /// Incoming kadı of the record (the successor when `role` is `departed`).
    pub new_person: Option<PersonRef>,
    pub origin: Option<PlaceRef>,
    pub destination: Option<PlaceRef>,
}

#[derive(sqlx::FromRow)]
struct PersonRow {
    id: i64,
    canonical_name: String,
    normalized_name: String,
    wikidata_qid: Option<String>,
    resolution_status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct JourneyRow {
    appointment_id: i64,
    source_record_id: i64,
    source_doc_id: String,
    is_incoming: bool,
    is_outgoing: bool,
    year_original: Option<String>,
    year_numeric: Option<i32>,
    calendar: String,
    degree: Option<String>,
    position_type: Option<String>,
    period: Option<String>,
    salary: Option<String>,
    old_salary: Option<String>,
    region_raw: Option<String>,
    raw_old_place: Option<String>,
    raw_new_place: Option<String>,
    raw_old_kadi: Option<String>,
    raw_new_kadi: Option<String>,
    old_person_id: Option<i64>,
    old_person_name: Option<String>,
    old_person_status: Option<String>,
    new_person_id: Option<i64>,
    new_person_name: Option<String>,
    new_person_status: Option<String>,
    origin_id: Option<i64>,
    origin_name: Option<String>,
    origin_lat: Option<f64>,
    origin_lon: Option<f64>,
    dest_id: Option<i64>,
    dest_name: Option<String>,
    dest_lat: Option<f64>,
    dest_lon: Option<f64>,
}

pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<PersonDetail>> {
    let person = sqlx::query_as::<_, PersonRow>(
        "SELECT id, canonical_name, normalized_name, wikidata_qid, resolution_status, \
         created_at, updated_at FROM persons WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found(format!("person {id} not found")))?;

    let rows = sqlx::query_as::<_, JourneyRow>(
        r#"
        SELECT
            a.id                        AS appointment_id,
            a.source_record_id,
            s.doc_id                    AS source_doc_id,
            COALESCE(a.new_person_id = $1, false) AS is_incoming,
            COALESCE(a.old_person_id = $1, false) AS is_outgoing,
            a.year_original,
            a.year_numeric,
            a.calendar::text            AS calendar,
            a.degree,
            a.position_type,
            a.period,
            a.salary,
            a.old_salary,
            a.region_raw,
            a.raw_old_place,
            a.raw_new_place,
            a.raw_old_kadi,
            a.raw_new_kadi,
            po.id AS old_person_id, po.canonical_name AS old_person_name,
            po.resolution_status AS old_person_status,
            pn.id AS new_person_id, pn.canonical_name AS new_person_name,
            pn.resolution_status AS new_person_status,
            o.id   AS origin_id,   o.canonical_name AS origin_name,
            o.latitude AS origin_lat, o.longitude AS origin_lon,
            d.id   AS dest_id,     d.canonical_name AS dest_name,
            d.latitude AS dest_lat,   d.longitude AS dest_lon
        FROM appointments a
        JOIN source_records s ON s.id = a.source_record_id
        LEFT JOIN persons po ON po.id = a.old_person_id
        LEFT JOIN persons pn ON pn.id = a.new_person_id
        LEFT JOIN places o ON o.id = a.origin_place_id
        LEFT JOIN places d ON d.id = a.destination_place_id
        WHERE a.old_person_id = $1 OR a.new_person_id = $1
        ORDER BY a.year_numeric NULLS LAST, a.id
        "#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let journey = rows
        .into_iter()
        .map(|r| JourneyStep {
            sequence: 0,
            follows_previous: false,
            appointment_id: r.appointment_id,
            source_record_id: r.source_record_id,
            source_doc_id: r.source_doc_id,
            role: match (r.is_incoming, r.is_outgoing) {
                (true, true) => "both",
                (true, false) => "appointed",
                (false, true) => "departed",
                (false, false) => "unknown",
            },
            year_original: r.year_original,
            year_numeric: r.year_numeric,
            calendar: r.calendar,
            degree: r.degree,
            position_type: r.position_type,
            period: r.period,
            salary: r.salary,
            old_salary: r.old_salary,
            region_raw: r.region_raw,
            raw_old_place: r.raw_old_place,
            raw_new_place: r.raw_new_place,
            raw_old_kadi: r.raw_old_kadi,
            raw_new_kadi: r.raw_new_kadi,
            old_person: PersonRef::from_parts(
                r.old_person_id,
                r.old_person_name,
                r.old_person_status,
            ),
            new_person: PersonRef::from_parts(
                r.new_person_id,
                r.new_person_name,
                r.new_person_status,
            ),
            origin: PlaceRef::from_parts(r.origin_id, r.origin_name, r.origin_lat, r.origin_lon),
            destination: PlaceRef::from_parts(r.dest_id, r.dest_name, r.dest_lat, r.dest_lon),
        })
        .collect();
    let journey = order_journey(journey);

    Ok(Json(PersonDetail {
        id: person.id,
        canonical_name: person.canonical_name,
        normalized_name: person.normalized_name,
        wikidata_qid: person.wikidata_qid,
        resolution_status: person.resolution_status,
        created_at: person.created_at,
        updated_at: person.updated_at,
        journey,
    }))
}

// ---------------------------------------------------------------------------
// Itinerary ordering
// ---------------------------------------------------------------------------

/// Order a person's steps into an itinerary.
///
/// An `appointed` step moves the person from `old_place` to `new_place`; a
/// `departed` step is them leaving `new_place` (see [`JourneyStep::origin_key`]).
///
/// Sorting by year alone is not enough: several records often share a year
/// (1224: B -> C and C -> D), and the year order says nothing about which came
/// first. The year stays the primary constraint, but within the earliest
/// pending year the step that departs from where the person currently is wins,
/// so the old place -> new place links form a chain (A -> B -> C -> D). Undated
/// steps are pulled in when they continue the chain, otherwise appended.
///
/// `steps` must arrive sorted by `year_numeric NULLS LAST, id`; that order is
/// the tie-breaker whenever the graph does not decide.
pub(crate) fn order_journey(steps: Vec<JourneyStep>) -> Vec<JourneyStep> {
    let mut remaining = steps;
    let mut ordered: Vec<JourneyStep> = Vec::with_capacity(remaining.len());
    let mut here: Option<String> = None;

    while !remaining.is_empty() {
        let min_year = remaining.iter().filter_map(|s| s.year_numeric).min();
        let group: Vec<usize> = (0..remaining.len())
            .filter(|&i| remaining[i].year_numeric == min_year)
            .collect();
        let departs_here = |i: &usize| here.is_some() && remaining[*i].origin_key() == here;

        let pick = group
            .iter()
            .copied()
            .find(departs_here)
            .or_else(|| {
                // an undated step that continues the chain beats a jump
                (0..remaining.len())
                    .filter(|&i| remaining[i].year_numeric.is_none())
                    .find(departs_here)
            })
            .or_else(|| {
                // otherwise start from a step no other same-year step leads into
                group.iter().copied().find(|&i| {
                    let origin = remaining[i].origin_key();
                    origin.is_none()
                        || !group
                            .iter()
                            .any(|&j| j != i && remaining[j].destination_key() == origin)
                })
            })
            .unwrap_or(group[0]);

        let mut step = remaining.remove(pick);
        step.sequence = ordered.len() + 1;
        step.follows_previous = here.is_some() && step.origin_key() == here;
        here = step.destination_key();
        ordered.push(step);
    }

    ordered
}

impl JourneyStep {
    /// Where the person was when the record starts. A record moves the
    /// *incoming* kadı from `old_place` to `new_place`, where they replace the
    /// outgoing one — so an outgoing (`departed`) person was at `new_place`.
    fn origin_key(&self) -> Option<String> {
        if self.role == "departed" {
            return self.destination_key();
        }
        place_key(self.origin.as_ref(), self.raw_old_place.as_deref())
    }

    fn destination_key(&self) -> Option<String> {
        place_key(self.destination.as_ref(), self.raw_new_place.as_deref())
    }
}

/// Identity used to link steps: the resolved place id, else the normalized raw
/// name.
fn place_key(place: Option<&PlaceRef>, raw: Option<&str>) -> Option<String> {
    match place {
        Some(p) => Some(format!("id:{}", p.id)),
        None => raw.and_then(normalize_key).map(|k| format!("raw:{k}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: i64, year: Option<i32>, from: &str, to: &str) -> JourneyStep {
        JourneyStep {
            sequence: 0,
            follows_previous: false,
            appointment_id: id,
            source_record_id: id,
            source_doc_id: id.to_string(),
            role: "appointed",
            year_original: year.map(|y| y.to_string()),
            year_numeric: year,
            calendar: "unknown".into(),
            degree: None,
            position_type: None,
            period: None,
            salary: None,
            old_salary: None,
            region_raw: None,
            raw_old_place: Some(from.into()),
            raw_new_place: Some(to.into()),
            raw_old_kadi: None,
            raw_new_kadi: None,
            old_person: None,
            new_person: None,
            origin: None,
            destination: None,
        }
    }

    fn route(steps: &[JourneyStep]) -> String {
        let mut out = steps[0].raw_old_place.clone().unwrap();
        for s in steps {
            out.push_str(s.raw_new_place.as_deref().unwrap());
        }
        out
    }

    /// Input in `year NULLS LAST, id` order, as the SQL returns it.
    fn sorted(mut steps: Vec<JourneyStep>) -> Vec<JourneyStep> {
        steps.sort_by_key(|s| (s.year_numeric.is_none(), s.year_numeric, s.appointment_id));
        steps
    }

    #[test]
    fn same_year_steps_follow_the_place_chain() {
        // C -> D was recorded (lower id) before B -> C, both in 1224.
        let steps = sorted(vec![
            step(1, Some(1223), "A", "B"),
            step(2, Some(1224), "C", "D"),
            step(3, Some(1224), "B", "C"),
            step(4, Some(1227), "D", "E"),
        ]);
        let out = order_journey(steps);
        assert_eq!(route(&out), "ABCDE");
        assert_eq!(
            out.iter().map(|s| s.sequence).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert_eq!(
            out.iter().map(|s| s.follows_previous).collect::<Vec<_>>(),
            [false, true, true, true]
        );
    }

    #[test]
    fn same_year_chain_without_a_known_predecessor() {
        // first year is ambiguous on its own: start from the chain head
        let out = order_journey(sorted(vec![
            step(1, Some(1224), "C", "D"),
            step(2, Some(1224), "B", "C"),
        ]));
        assert_eq!(route(&out), "BCD");
    }

    #[test]
    fn year_order_is_never_overridden() {
        // B -> C departs from "here" but is later in time than X -> Y
        let out = order_journey(sorted(vec![
            step(1, Some(1223), "A", "B"),
            step(2, Some(1224), "X", "Y"),
            step(3, Some(1230), "B", "C"),
        ]));
        assert_eq!(
            out.iter().map(|s| s.appointment_id).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(!out[1].follows_previous);
    }

    #[test]
    fn undated_steps_slot_in_where_they_continue_the_chain() {
        let out = order_journey(sorted(vec![
            step(1, Some(1223), "A", "B"),
            step(2, Some(1227), "C", "D"),
            step(3, None, "B", "C"),
            step(4, None, "Q", "R"),
        ]));
        assert_eq!(
            out.iter().map(|s| s.appointment_id).collect::<Vec<_>>(),
            [1, 3, 2, 4]
        );
    }

    #[test]
    fn departing_from_the_current_post_keeps_the_chain() {
        // someone else's X -> B record replaces our kadı at B, then he moves on
        let mut left = step(2, Some(1225), "X", "B");
        left.role = "departed";
        let out = order_journey(sorted(vec![
            step(3, Some(1225), "B", "C"),
            left,
            step(1, Some(1224), "A", "B"),
        ]));
        assert_eq!(
            out.iter().map(|s| s.appointment_id).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(out[1].follows_previous, "left B, where he was");
        assert!(out[2].follows_previous, "and went on from B");
    }

    #[test]
    fn resolved_place_ids_link_steps_even_if_raw_spelling_differs() {
        let place = |id| PlaceRef {
            id,
            canonical_name: String::new(),
            latitude: None,
            longitude: None,
        };
        let mut a = step(1, Some(1224), "Kilis", "Halep");
        let mut b = step(2, Some(1224), "Haleb", "Şam");
        a.destination = Some(place(7));
        b.origin = Some(place(7));
        let out = order_journey(vec![b, a]);
        assert_eq!(out[0].appointment_id, 1);
        assert!(out[1].follows_previous);
    }
}
