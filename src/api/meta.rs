//! `GET /api/v1/meta` — corpus-level summary for clients (counts, year span).

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::app::AppState;
use crate::error::ApiResult;

#[derive(Serialize)]
pub struct Meta {
    api_version: &'static str,
    counts: Counts,
    year_numeric_range: YearSpan,
    /// Distinct `calendar` values present on appointments.
    calendars: Vec<String>,
    /// Distinct `degree` values, for filter menus.
    degrees: Vec<String>,
    /// Distinct `position_type` values, for filter menus.
    position_types: Vec<String>,
    notes: MetaNotes,
}

#[derive(Serialize)]
struct Counts {
    sources: i64,
    persons: i64,
    persons_confirmed: i64,
    places: i64,
    places_with_coordinates: i64,
    appointments: i64,
    appointments_with_flow: i64,
}

#[derive(Serialize)]
struct YearSpan {
    min: Option<i32>,
    max: Option<i32>,
}

#[derive(Serialize)]
struct MetaNotes {
    year_numeric: &'static str,
    identity: &'static str,
}

pub async fn meta(State(state): State<AppState>) -> ApiResult<Json<Meta>> {
    let row = sqlx::query_as::<_, MetaRow>(
        r#"
        SELECT
            (SELECT count(*) FROM source_records)                                  AS sources,
            (SELECT count(*) FROM persons)                                         AS persons,
            (SELECT count(*) FROM persons WHERE resolution_status = 'confirmed')   AS persons_confirmed,
            (SELECT count(*) FROM places)                                          AS places,
            (SELECT count(*) FROM places WHERE geom IS NOT NULL)                   AS places_with_coordinates,
            (SELECT count(*) FROM appointments)                                    AS appointments,
            (SELECT count(*) FROM appointments
                 WHERE origin_place_id IS NOT NULL
                   AND destination_place_id IS NOT NULL)                          AS appointments_with_flow,
            (SELECT min(year_numeric) FROM appointments)                           AS year_min,
            (SELECT max(year_numeric) FROM appointments)                           AS year_max
        "#,
    )
    .fetch_one(&state.pool)
    .await?;

    let calendars = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT calendar::text FROM appointments ORDER BY 1",
    )
    .fetch_all(&state.pool)
    .await?;

    let degrees = distinct_values(&state, "degree").await?;
    let position_types = distinct_values(&state, "position_type").await?;

    Ok(Json(Meta {
        api_version: "v1",
        counts: Counts {
            sources: row.sources,
            persons: row.persons,
            persons_confirmed: row.persons_confirmed,
            places: row.places,
            places_with_coordinates: row.places_with_coordinates,
            appointments: row.appointments,
            appointments_with_flow: row.appointments_with_flow,
        },
        year_numeric_range: YearSpan {
            min: row.year_min,
            max: row.year_max,
        },
        calendars,
        degrees,
        position_types,
        notes: MetaNotes {
            year_numeric:
                "Best-effort integer parsed from the original date string for range filtering only; \
                 calendar-agnostic, not an authoritative Gregorian year.",
            identity:
                "persons/places are provisional groupings by normalized name, not confirmed \
                 historical identities.",
        },
    }))
}

/// Distinct non-empty values of a descriptive appointment column, capped so a
/// messy column cannot blow up the response.
async fn distinct_values(state: &AppState, column: &'static str) -> ApiResult<Vec<String>> {
    // `column` is one of two compile-time constants, never user input.
    let sql = format!(
        "SELECT {column} FROM appointments \
         WHERE {column} IS NOT NULL AND btrim({column}) <> '' \
         GROUP BY {column} ORDER BY {column} LIMIT 500"
    );
    Ok(sqlx::query_scalar::<_, String>(&sql)
        .fetch_all(&state.pool)
        .await?)
}

#[derive(sqlx::FromRow)]
struct MetaRow {
    sources: i64,
    persons: i64,
    persons_confirmed: i64,
    places: i64,
    places_with_coordinates: i64,
    appointments: i64,
    appointments_with_flow: i64,
    year_min: Option<i32>,
    year_max: Option<i32>,
}
