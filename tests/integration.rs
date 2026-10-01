//! End-to-end tests: HTTP surface + XLSX importer, against a real PostgreSQL +
//! PostGIS database (see `tests/common/mod.rs` for connection configuration).

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{write_fixture, Cell, STD_HEADERS};
use kadi_atlas::importer::{import_path, ImportOptions};
use serde_json::Value;
use tower::ServiceExt;

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

/// Build a standard-shaped row. `coords` is `(old_lat, old_lon, new_lat, new_lon)`.
#[allow(clippy::too_many_arguments)]
fn row(
    doc_id: &'static str,
    old_kadi: Cell,
    new_kadi: Cell,
    old_place: Cell,
    new_place: Cell,
    tarih: Cell,
    text: Cell,
    coords: Option<(f64, f64, f64, f64)>,
) -> Vec<Cell> {
    let mut r = vec![Cell::Empty; STD_HEADERS.len()];
    r[0] = Cell::S(doc_id);
    r[1] = Cell::S("mevleviyet");
    r[2] = Cell::S("kaza");
    r[8] = old_kadi;
    r[9] = new_kadi;
    r[10] = old_place;
    r[11] = new_place;
    r[12] = tarih;
    r[14] = Cell::S("Rumeli");
    r[16] = text;
    if let Some((ola, olo, nla, nlo)) = coords {
        r[17] = Cell::N(ola);
        r[18] = Cell::N(olo);
        r[19] = Cell::N(nla);
        r[20] = Cell::N(nlo);
    }
    r
}

#[tokio::test]
async fn health_ok() {
    let (app, _pool) = common::setup_app().await;
    let (status, body) = get(&app, "/api/v1/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["database"], "ok");
}

#[tokio::test]
async fn invalid_query_is_400() {
    let (app, _pool) = common::setup_app().await;

    let (s1, b1) = get(&app, "/api/v1/persons?limit=0").await;
    assert_eq!(s1, StatusCode::BAD_REQUEST);
    assert_eq!(b1["error"]["code"], "bad_request");
    // error code/message are also mirrored at the top level for simple clients
    assert_eq!(b1["code"], "bad_request");
    assert!(b1["message"].as_str().unwrap().contains("limit"));

    let (s2, _) = get(&app, "/api/v1/persons?limit=not-a-number").await;
    assert_eq!(s2, StatusCode::BAD_REQUEST);

    let (s3, _) = get(&app, "/api/v1/appointments?year_from=1200&year_to=1100").await;
    assert_eq!(s3, StatusCode::BAD_REQUEST);

    let (s4, _) = get(&app, "/api/v1/persons?resolution_status=bogus").await;
    assert_eq!(s4, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn missing_entity_is_404() {
    let (app, _pool) = common::setup_app().await;
    let (status, body) = get(&app, "/api/v1/persons/2000000001").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");

    let (s2, _) = get(&app, "/api/v1/places/2000000001").await;
    assert_eq!(s2, StatusCode::NOT_FOUND);

    let (s3, b3) = get(&app, "/api/v1/definitely/not/here").await;
    assert_eq!(s3, StatusCode::NOT_FOUND);
    assert_eq!(b3["error"]["code"], "not_found");
}

#[tokio::test]
async fn import_is_idempotent() {
    let (_app, pool) = common::setup_app().await;

    let rows = vec![
        row(
            "idem-1",
            Cell::S("Ahmed Efendi"),
            Cell::S("Mehmed Efendi"),
            Cell::S("Selanik"),
            Cell::S("Manastır"),
            Cell::S("1123"),
            Cell::S("ber-vech-i arpalık tevcih olundu"),
            Some((40.64, 22.94, 41.0, 21.34)),
        ),
        // placeholder values -> NULL in normalized columns, raw kept
        row(
            "idem-2",
            Cell::S("-"),
            Cell::S("Mehmed Efendi"),
            Cell::S("-"),
            Cell::S("Manastır"),
            Cell::S("-"),
            Cell::Empty,
            None,
        ),
        // mostly-empty row
        row(
            "idem-3",
            Cell::Empty,
            Cell::S("Ali Efendi"),
            Cell::Empty,
            Cell::S("Yenişehir"),
            Cell::S("evahir-i Ramazan 1130"),
            Cell::Empty,
            None,
        ),
        row(
            "idem-4",
            Cell::S("Ahmed Efendi"),
            Cell::S("Osman Efendi"),
            Cell::S("Manastır"),
            Cell::S("Selanik"),
            Cell::S("1125"),
            Cell::Empty,
            None,
        ),
    ];
    let (_d, path) = write_fixture(STD_HEADERS, &rows);
    let opts = ImportOptions::default();

    // This test asserts insert vs. update counts, so start from a clean slate
    // for its own doc_id namespace (the test DB persists between runs).
    sqlx::query("DELETE FROM source_records WHERE doc_id LIKE 'idem-%'")
        .execute(&pool)
        .await
        .unwrap();

    let r1 = import_path(&pool, &path, &opts).await.unwrap();
    assert_eq!(r1.rows_total, 4);
    assert_eq!(r1.source_records_inserted, 4);
    assert_eq!(r1.appointments_inserted, 4);
    assert!(r1.committed);

    let r2 = import_path(&pool, &path, &opts).await.unwrap();
    assert_eq!(
        r2.source_records_inserted, 0,
        "second import must not insert"
    );
    assert_eq!(r2.source_records_updated, 4);
    assert_eq!(r2.appointments_inserted, 0);

    let sources: i64 =
        sqlx::query_scalar("SELECT count(*) FROM source_records WHERE doc_id LIKE 'idem-%'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sources, 4);

    let appts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM appointments a JOIN source_records s ON s.id = a.source_record_id \
         WHERE s.doc_id LIKE 'idem-%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(appts, 4);

    // placeholder handling: raw kept, normalized NULL
    let (raw_old_kadi, old_person_id): (Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT a.raw_old_kadi, a.old_person_id FROM appointments a \
         JOIN source_records s ON s.id = a.source_record_id WHERE s.doc_id = 'idem-2'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(raw_old_kadi.as_deref(), Some("-"));
    assert!(old_person_id.is_none(), "'-' must not resolve to a person");

    let raw_json: Value =
        sqlx::query_scalar("SELECT raw FROM source_records WHERE doc_id = 'idem-2'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        raw_json["old_place"], "-",
        "raw JSON preserves the placeholder"
    );

    // coordinates -> PostGIS point
    let has_geom: bool =
        sqlx::query_scalar("SELECT geom IS NOT NULL FROM places WHERE normalized_name = 'selanik'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(has_geom, "Selanik got coordinates from idem-1");

    // calendar is never guessed
    let calendars: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT a.calendar::text FROM appointments a \
         JOIN source_records s ON s.id = a.source_record_id WHERE s.doc_id LIKE 'idem-%'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(calendars, vec!["unknown".to_string()]);
}

#[tokio::test]
async fn person_place_and_appointment_queries() {
    let (app, pool) = common::setup_app().await;

    let rows = vec![
        row(
            "pq-1",
            Cell::S("Zeynelabidin Efendi"),
            Cell::S("Kmpq Halef Efendi"),
            Cell::S("Pqville Eski"),
            Cell::S("Pqville Yeni"),
            Cell::S("1100"),
            Cell::S("kaza-i mezbure tevcih"),
            Some((39.0, 32.0, 39.5, 32.5)),
        ),
        row(
            "pq-2",
            Cell::S("Kmpq Halef Efendi"),
            Cell::S("Zeynelabidin Efendi"),
            Cell::S("Pqville Yeni"),
            Cell::S("Pqville Uc"),
            Cell::S("1105"),
            Cell::Empty,
            None,
        ),
    ];
    let (_d, path) = write_fixture(STD_HEADERS, &rows);
    import_path(&pool, &path, &ImportOptions::default())
        .await
        .unwrap();

    // person search
    let (status, body) = get(&app, "/api/v1/persons?query=zeynelabidin").await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let person_id = items[0]["id"].as_i64().unwrap();
    assert!(items[0]["appointment_count"].as_i64().unwrap() >= 2);

    // person detail carries a journey with coordinates
    let (status, detail) = get(&app, &format!("/api/v1/persons/{person_id}")).await;
    assert_eq!(status, StatusCode::OK);
    let journey = detail["journey"].as_array().unwrap();
    assert_eq!(journey.len(), 2);
    let step_with_coords = journey
        .iter()
        .find(|s| s["destination"]["latitude"].is_number())
        .expect("at least one journey step has destination coordinates");
    assert_eq!(step_with_coords["destination"]["latitude"], 39.5);

    // appointment filtering by person
    let (status, body) = get(
        &app,
        &format!("/api/v1/appointments?person={person_id}&limit=10"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let appts = body["items"].as_array().unwrap();
    assert_eq!(appts.len(), 2);
    for a in appts {
        let old = a["old_person"]["id"].as_i64();
        let new = a["new_person"]["id"].as_i64();
        assert!(old == Some(person_id) || new == Some(person_id));
    }

    // year filtering
    let (_s, body) = get(
        &app,
        &format!("/api/v1/appointments?person={person_id}&year_from=1103&year_to=1110"),
    )
    .await;
    let appts = body["items"].as_array().unwrap();
    assert_eq!(appts.len(), 1);
    assert_eq!(appts[0]["year_numeric"], 1105);

    // frontend-style aliases: `q` and `person_id`
    let (_s, alias_persons) = get(&app, "/api/v1/persons?q=zeynelabidin").await;
    assert_eq!(
        alias_persons["items"][0]["id"].as_i64(),
        Some(person_id),
        "`q` is accepted as an alias for `query`"
    );
    let (_s, alias_appts) = get(&app, &format!("/api/v1/appointments?person_id={person_id}")).await;
    assert_eq!(
        alias_appts["items"].as_array().unwrap().len(),
        2,
        "`person_id` is accepted as an alias for `person`"
    );

    // place search + detail
    let (status, body) = get(&app, "/api/v1/places?query=pqville%20yeni").await;
    assert_eq!(status, StatusCode::OK);
    let places = body["items"].as_array().unwrap();
    assert_eq!(places.len(), 1);
    let place_id = places[0]["id"].as_i64().unwrap();
    assert!(places[0]["inflow_count"].as_i64().unwrap() >= 1);
    assert!(places[0]["outflow_count"].as_i64().unwrap() >= 1);

    let (status, place_detail) = get(&app, &format!("/api/v1/places/{place_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!place_detail["names"].as_array().unwrap().is_empty());
    assert!(place_detail["stats"]["distinct_persons"].as_i64().unwrap() >= 1);

    // full-text source search
    let (status, body) = get(&app, "/api/v1/sources?query=mezbure").await;
    assert_eq!(status, StatusCode::OK);
    let hits = body["items"].as_array().unwrap();
    assert!(hits.iter().any(|h| h["doc_id"] == "pq-1"));
}

#[tokio::test]
async fn flow_aggregation_counts_and_coordinates() {
    let (app, pool) = common::setup_app().await;

    let rows = vec![
        // Origin -> Dest, both endpoints geocoded, three rows across three years.
        row(
            "flow-1",
            Cell::S("Flowperson A"),
            Cell::S("Flowperson B"),
            Cell::S("Flowtown Origin"),
            Cell::S("Flowtown Dest"),
            Cell::S("1140"),
            Cell::Empty,
            Some((41.0, 29.0, 38.4, 27.1)),
        ),
        row(
            "flow-2",
            Cell::S("Flowperson C"),
            Cell::S("Flowperson D"),
            Cell::S("Flowtown Origin"),
            Cell::S("Flowtown Dest"),
            Cell::S("1141"),
            Cell::Empty,
            Some((41.0, 29.0, 38.4, 27.1)),
        ),
        // Coordinates live on the place, not the row: this row still aggregates
        // into the geocoded Origin->Dest flow.
        row(
            "flow-3",
            Cell::S("Flowperson E"),
            Cell::S("Flowperson F"),
            Cell::S("Flowtown Origin"),
            Cell::S("Flowtown Dest"),
            Cell::S("1142"),
            Cell::Empty,
            None,
        ),
        // Origin -> Nowhere: destination never geocoded, excluded by default.
        row(
            "flow-4",
            Cell::S("Flowperson G"),
            Cell::S("Flowperson H"),
            Cell::S("Flowtown Origin"),
            Cell::S("Flowtown Nowhere"),
            Cell::S("1143"),
            Cell::Empty,
            None,
        ),
        row(
            "flow-5",
            Cell::S("Flowperson I"),
            Cell::S("Flowperson J"),
            Cell::S("Flowtown Origin"),
            Cell::S("Flowtown Nowhere"),
            Cell::S("1144"),
            Cell::Empty,
            None,
        ),
    ];
    let (_d, path) = write_fixture(STD_HEADERS, &rows);
    import_path(&pool, &path, &ImportOptions::default())
        .await
        .unwrap();

    let origin_id: i64 =
        sqlx::query_scalar("SELECT id FROM places WHERE normalized_name = 'flowtown origin'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let dest_id: i64 =
        sqlx::query_scalar("SELECT id FROM places WHERE normalized_name = 'flowtown dest'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let nowhere_id: i64 =
        sqlx::query_scalar("SELECT id FROM places WHERE normalized_name = 'flowtown nowhere'")
            .fetch_one(&pool)
            .await
            .unwrap();

    let find_flow = |body: &Value, o: i64, d: i64| -> Option<Value> {
        body["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| {
                f["origin"]["id"].as_i64() == Some(o) && f["destination"]["id"].as_i64() == Some(d)
            })
            .cloned()
    };

    // default: require_coordinates = true
    let (status, body) = get(&app, "/api/v1/flows?year_from=1100&year_to=1200&limit=200").await;
    assert_eq!(status, StatusCode::OK);
    let geocoded = find_flow(&body, origin_id, dest_id).expect("geocoded flow present");
    assert_eq!(
        geocoded["count"], 3,
        "all three Origin->Dest rows aggregate"
    );
    assert_eq!(geocoded["origin"]["latitude"], 41.0);
    assert_eq!(geocoded["destination"]["longitude"], 27.1);
    assert!(
        find_flow(&body, origin_id, nowhere_id).is_none(),
        "flow into a non-geocoded place is excluded by default"
    );

    // require_coordinates = false brings the non-geocoded pair back
    let (_s, body) = get(
        &app,
        "/api/v1/flows?year_from=1100&year_to=1200&require_coordinates=false&limit=200",
    )
    .await;
    let nowhere_flow = find_flow(&body, origin_id, nowhere_id).expect("non-geocoded flow present");
    assert_eq!(nowhere_flow["count"], 2);
    assert!(nowhere_flow["destination"]["latitude"].is_null());

    // min_count filter
    let (_s, body) = get(
        &app,
        "/api/v1/flows?year_from=1100&year_to=1200&min_count=3&limit=200",
    )
    .await;
    let flows = body["items"].as_array().unwrap();
    assert!(flows.iter().all(|f| f["count"].as_i64().unwrap() >= 3));
    assert!(find_flow(&body, origin_id, dest_id).is_some());
}

#[tokio::test]
async fn frontend_support_endpoints() {
    let (app, pool) = common::setup_app().await;

    // Chain A -> B -> C for one kadı, the same-year pair recorded out of order.
    let rows = vec![
        row(
            "fe-1",
            Cell::S("Fesupport Onceki"),
            Cell::S("Fesupport Gezgin Efendi"),
            Cell::S("Fetown A"),
            Cell::S("Fetown B"),
            Cell::S("1223"),
            Cell::S("fetest tevcih"),
            Some((40.0, 30.0, 40.5, 30.5)),
        ),
        row(
            "fe-3",
            Cell::S("Fesupport Diger"),
            Cell::S("Fesupport Gezgin Efendi"),
            Cell::S("Fetown C"),
            Cell::S("Fetown D"),
            Cell::S("1224"),
            Cell::Empty,
            Some((41.0, 31.0, 41.5, 31.5)),
        ),
        row(
            "fe-2",
            Cell::S("Fesupport Baska"),
            Cell::S("Fesupport Gezgin Efendi"),
            Cell::S("Fetown B"),
            Cell::S("Fetown C"),
            Cell::S("1224"),
            Cell::Empty,
            Some((40.5, 30.5, 41.0, 31.0)),
        ),
    ];
    let (_d, path) = write_fixture(STD_HEADERS, &rows);
    import_path(&pool, &path, &ImportOptions::default())
        .await
        .unwrap();

    // /meta exposes filter vocabularies
    let (status, meta) = get(&app, "/api/v1/meta").await;
    assert_eq!(status, StatusCode::OK);
    assert!(meta["degrees"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d == "mevleviyet"));
    assert!(meta["position_types"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d == "kaza"));

    // journey follows the place chain and carries source ids
    let (_s, persons) = get(&app, "/api/v1/persons?q=fesupport%20gezgin").await;
    let person_id = persons["items"][0]["id"].as_i64().unwrap();
    let (_s, detail) = get(&app, &format!("/api/v1/persons/{person_id}")).await;
    let journey = detail["journey"].as_array().unwrap();
    let docs: Vec<_> = journey
        .iter()
        .map(|s| s["source_doc_id"].as_str().unwrap())
        .collect();
    assert_eq!(docs, ["fe-1", "fe-2", "fe-3"]);
    assert!(journey.iter().all(|s| s["source_record_id"].is_i64()));
    assert_eq!(journey[2]["sequence"], 3);
    assert_eq!(journey[2]["follows_previous"], true);

    // appointments can be looked up by source record
    let source_id = journey[0]["source_record_id"].as_i64().unwrap();
    let (status, appts) = get(
        &app,
        &format!("/api/v1/appointments?source_record_id={source_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(appts["total"], 1);
    assert_eq!(appts["items"][0]["source_doc_id"], "fe-1");

    // place activity: B is a destination once and an origin once
    let b_id: i64 = sqlx::query_scalar("SELECT id FROM places WHERE normalized_name = 'fetown b'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (status, activity) = get(
        &app,
        &format!("/api/v1/place-activity?place={b_id}&year_from=1223&year_to=1224"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let b = activity["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"].as_i64() == Some(b_id))
        .expect("B present");
    assert_eq!(b["arrivals"], 1);
    assert_eq!(b["departures"], 1);
    assert!(activity["total"].as_i64().unwrap() >= 3);
    assert!(b.get("total_count").is_none());

    let (status, _) = get(&app, "/api/v1/place-activity?limit=0").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = get(&app, "/api/v1/place-activity?year_from=5&year_to=1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // unknown API routes stay JSON 404s
    let (status, body) = get(&app, "/api/v1/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn serves_frontend_with_its_own_csp_when_configured() {
    let pool = common::setup_pool().await;
    let mut cfg = common::test_config();
    cfg.web_dir = Some(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web"));
    let app = kadi_atlas::app::build_router(kadi_atlas::app::AppState::new(pool, &cfg), &cfg);

    let fetch = |uri: &'static str| {
        let app = app.clone();
        async move {
            app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap()
        }
    };

    let page = fetch("/").await;
    assert_eq!(page.status(), StatusCode::OK);
    let csp = page.headers()["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("script-src 'self'"), "page CSP: {csp}");

    let api = fetch("/api/v1/health").await;
    assert_eq!(api.status(), StatusCode::OK);
    let csp = api.headers()["content-security-policy"].to_str().unwrap();
    assert!(csp.starts_with("default-src 'none'"), "api CSP: {csp}");

    assert_eq!(
        fetch("/js/missing.js").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(fetch("/api/v1/nope").await.status(), StatusCode::NOT_FOUND);
}
