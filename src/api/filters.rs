//! The appointment filters shared by every endpoint that reads `appointments`
//! (`/appointments`, `/flows`, `/place-activity`, `/stats`, the CSV export).
//!
//! One definition means one meaning: a filter set on the map gives the same
//! records in the table, the charts and the export. Each endpoint extracts
//! [`FilterParams`] next to its own query struct (unknown keys are ignored by
//! both), resolves it once and pushes the clauses into its SQL.
//!
//! All clauses assume the appointments view is aliased `a`.

use serde::Deserialize;
use sqlx::{Postgres, QueryBuilder};

use super::validate_year_range;
use crate::error::{ApiError, ApiResult};
use crate::normalize::{fold_term, normalize_key};

/// At most this many ids in `persons=1,2,3`.
const MAX_PERSONS: usize = 50;

#[derive(Debug, Default, Deserialize)]
pub struct FilterParams {
    /// Inclusive `year_numeric` window (calendar-agnostic, see architecture.md).
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
    /// One person id; matches the outgoing or the incoming kadı.
    pub person: Option<i64>,
    /// Alias for `person`.
    pub person_id: Option<i64>,
    /// Several person ids, comma-separated (`persons=4,17`); any of them.
    pub persons: Option<String>,
    /// One place id; matches origin or destination (see `direction`).
    pub place: Option<i64>,
    /// Alias for `place`.
    pub place_id: Option<i64>,
    /// Every place whose normalized name *starts with* this text
    /// (`Misis` → `Misis`, `Misis maa Ayaş`, …); see `direction`.
    pub place_prefix: Option<String>,
    /// With `place` / `place_prefix`: `any` (default), `arrival` (the place is
    /// the new place: a kadı came there) or `departure` (the place is the old
    /// place: a kadı left it for another post).
    pub direction: Option<String>,
    pub origin_place: Option<i64>,
    pub destination_place: Option<i64>,
    pub source_record_id: Option<i64>,
    pub degree: Option<String>,
    pub position_type: Option<String>,
    /// Substring match against the raw region string.
    pub region: Option<String>,
    /// `true`: both places geocoded; `false`: at least one is not.
    pub has_coordinates: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Any,
    Arrival,
    Departure,
}

/// A place filter: one place or a name-prefix group.
#[derive(Debug, Clone)]
enum PlaceMatch {
    Id(i64),
    Prefix(String),
}

/// Validated, normalized filters, ready to push into SQL.
#[derive(Debug, Clone, Default)]
pub struct Filters {
    year_from: Option<i32>,
    year_to: Option<i32>,
    persons: Vec<i64>,
    place: Option<PlaceMatch>,
    direction: Option<Direction>,
    origin_place: Option<i64>,
    destination_place: Option<i64>,
    source_record_id: Option<i64>,
    degree: Option<String>,
    position_type: Option<String>,
    region: Option<String>,
    has_coordinates: Option<bool>,
}

impl FilterParams {
    pub fn resolve(&self) -> ApiResult<Filters> {
        validate_year_range(self.year_from, self.year_to)?;

        let mut persons: Vec<i64> = self.person.or(self.person_id).into_iter().collect();
        if let Some(list) = self.persons.as_deref() {
            for part in list.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                let id: i64 = part.parse().map_err(|_| {
                    ApiError::bad_request("`persons` must be a comma-separated list of ids")
                })?;
                if !persons.contains(&id) {
                    persons.push(id);
                }
            }
        }
        if persons.len() > MAX_PERSONS {
            return Err(ApiError::bad_request(format!(
                "`persons` accepts at most {MAX_PERSONS} ids"
            )));
        }

        let prefix = match self.place_prefix.as_deref() {
            None => None,
            Some(raw) => Some(normalize_key(raw).ok_or_else(|| {
                ApiError::bad_request("`place_prefix` must contain a letter or digit")
            })?),
        };
        let place = match (self.place.or(self.place_id), prefix) {
            (Some(_), Some(_)) => {
                return Err(ApiError::bad_request(
                    "use either `place` or `place_prefix`, not both",
                ))
            }
            (Some(id), None) => Some(PlaceMatch::Id(id)),
            (None, Some(prefix)) => Some(PlaceMatch::Prefix(prefix)),
            (None, None) => None,
        };

        let direction = match self.direction.as_deref().map(str::trim) {
            None | Some("") | Some("any") => None,
            Some("arrival") => Some(Direction::Arrival),
            Some("departure") => Some(Direction::Departure),
            Some(_) => {
                return Err(ApiError::bad_request(
                    "`direction` must be one of: any, arrival, departure",
                ))
            }
        };

        Ok(Filters {
            year_from: self.year_from,
            year_to: self.year_to,
            persons,
            place,
            direction,
            origin_place: self.origin_place,
            destination_place: self.destination_place,
            source_record_id: self.source_record_id,
            // vocabulary values are stored folded; fold the filter the same way
            degree: self
                .degree
                .as_deref()
                .map(|d| fold_term(d).unwrap_or_default()),
            position_type: self
                .position_type
                .as_deref()
                .map(|p| fold_term(p).unwrap_or_default()),
            region: self.region.clone(),
            has_coordinates: self.has_coordinates,
        })
    }
}

/// Emits ` WHERE ` before the first clause and ` AND ` before the others.
#[derive(Debug, Default)]
pub struct Clauses {
    started: bool,
}

impl Clauses {
    pub fn new() -> Self {
        Self::default()
    }

    /// Continue a query that already has a `WHERE`.
    pub fn after_where() -> Self {
        Self { started: true }
    }

    pub fn next(&mut self, qb: &mut QueryBuilder<'_, Postgres>) {
        qb.push(if self.started { " AND " } else { " WHERE " });
        self.started = true;
    }
}

impl Filters {
    pub fn apply(&self, qb: &mut QueryBuilder<'_, Postgres>, clauses: &mut Clauses) {
        if let Some(year_from) = self.year_from {
            clauses.next(qb);
            qb.push("a.year_numeric >= ").push_bind(year_from);
        }
        if let Some(year_to) = self.year_to {
            clauses.next(qb);
            qb.push("a.year_numeric <= ").push_bind(year_to);
        }
        if !self.persons.is_empty() {
            clauses.next(qb);
            qb.push("(a.old_person_id = ANY(")
                .push_bind(self.persons.clone())
                .push(") OR a.new_person_id = ANY(")
                .push_bind(self.persons.clone())
                .push("))");
        }
        if let Some(place) = &self.place {
            clauses.next(qb);
            let direction = self.direction.unwrap_or(Direction::Any);
            let columns: &[&str] = match direction {
                Direction::Any => &["a.origin_place_id", "a.destination_place_id"],
                Direction::Arrival => &["a.destination_place_id"],
                Direction::Departure => &["a.origin_place_id"],
            };
            qb.push("(");
            for (i, column) in columns.iter().enumerate() {
                if i > 0 {
                    qb.push(" OR ");
                }
                match place {
                    PlaceMatch::Id(id) => {
                        qb.push(*column).push(" = ").push_bind(*id);
                    }
                    PlaceMatch::Prefix(prefix) => {
                        // normalized keys contain only [a-z0-9 ] and other
                        // letters, never LIKE wildcards, so no escaping needed
                        qb.push(*column)
                            .push(" IN (SELECT id FROM places WHERE normalized_name LIKE ")
                            .push_bind(format!("{prefix}%"))
                            .push(")");
                    }
                }
            }
            qb.push(")");
        }
        if let Some(origin) = self.origin_place {
            clauses.next(qb);
            qb.push("a.origin_place_id = ").push_bind(origin);
        }
        if let Some(destination) = self.destination_place {
            clauses.next(qb);
            qb.push("a.destination_place_id = ").push_bind(destination);
        }
        if let Some(source) = self.source_record_id {
            clauses.next(qb);
            qb.push("a.source_record_id = ").push_bind(source);
        }
        if let Some(degree) = &self.degree {
            clauses.next(qb);
            qb.push("a.degree = ").push_bind(degree.clone());
        }
        if let Some(position) = &self.position_type {
            clauses.next(qb);
            qb.push("a.position_type = ").push_bind(position.clone());
        }
        if let Some(region) = &self.region {
            clauses.next(qb);
            qb.push("a.region_raw ILIKE '%' || ")
                .push_bind(region.clone())
                .push(" || '%'");
        }
        match self.has_coordinates {
            Some(true) => {
                clauses.next(qb);
                qb.push(GEOCODED_BOTH);
            }
            Some(false) => {
                clauses.next(qb);
                qb.push("NOT ").push(GEOCODED_BOTH);
            }
            None => {}
        }
    }
}

/// Both ends of the record have a validated point.
pub const GEOCODED_BOTH: &str = "(EXISTS (SELECT 1 FROM places gx WHERE gx.id = a.origin_place_id AND gx.geom IS NOT NULL) \
     AND EXISTS (SELECT 1 FROM places gy WHERE gy.id = a.destination_place_id AND gy.geom IS NOT NULL))";

#[cfg(test)]
mod tests {
    use super::*;

    fn sql(params: FilterParams) -> String {
        let filters = params.resolve().unwrap();
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("SELECT 1 FROM appointments a");
        filters.apply(&mut qb, &mut Clauses::new());
        qb.sql().to_string()
    }

    #[test]
    fn empty_filters_add_nothing() {
        assert_eq!(sql(FilterParams::default()), "SELECT 1 FROM appointments a");
    }

    #[test]
    fn direction_picks_the_place_column() {
        let arrival = sql(FilterParams {
            place: Some(3),
            direction: Some("arrival".into()),
            ..Default::default()
        });
        assert!(
            arrival.contains("(a.destination_place_id = $1)"),
            "{arrival}"
        );
        assert!(!arrival.contains("origin_place_id"));

        let departure = sql(FilterParams {
            place_prefix: Some("Misis".into()),
            direction: Some("departure".into()),
            ..Default::default()
        });
        assert!(departure.contains(
            "a.origin_place_id IN (SELECT id FROM places WHERE normalized_name LIKE $1)"
        ));

        let any = sql(FilterParams {
            place: Some(3),
            ..Default::default()
        });
        assert!(any.contains("a.origin_place_id = $1 OR a.destination_place_id = $2"));
    }

    #[test]
    fn invalid_values_are_rejected() {
        let bad = |p: FilterParams| matches!(p.resolve(), Err(ApiError::BadRequest(_)));
        assert!(bad(FilterParams {
            direction: Some("sideways".into()),
            ..Default::default()
        }));
        assert!(bad(FilterParams {
            persons: Some("1,x".into()),
            ..Default::default()
        }));
        assert!(bad(FilterParams {
            place: Some(1),
            place_prefix: Some("a".into()),
            ..Default::default()
        }));
        assert!(bad(FilterParams {
            place_prefix: Some(" - ".into()),
            ..Default::default()
        }));
        assert!(bad(FilterParams {
            year_from: Some(1200),
            year_to: Some(1100),
            ..Default::default()
        }));
    }

    #[test]
    fn persons_merge_and_dedupe() {
        let filters = FilterParams {
            person: Some(4),
            persons: Some("4, 17,,9".into()),
            ..Default::default()
        }
        .resolve()
        .unwrap();
        assert_eq!(filters.persons, vec![4, 17, 9]);
    }
}
