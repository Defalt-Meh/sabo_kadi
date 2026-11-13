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
    assert_eq!(geocoded["last_year"], 1142, "latest year on the flow");

    // last_year respects the year window (the map ages arrows against it)
    let (_s, body) = get(&app, "/api/v1/flows?year_from=1100&year_to=1141&limit=200").await;
    let windowed = find_flow(&body, origin_id, dest_id).expect("windowed flow present");
    assert_eq!(windowed["count"], 2);
    assert_eq!(windowed["last_year"], 1141);
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

/// One appointment row in the delivered column layout.
fn delivered_row(
    doc_id: &'static str,
    date: Cell,
    position: &'static str,
    old_place: &'static str,
    new_place: &'static str,
    text: &'static str,
) -> Vec<Cell> {
    vec![
        Cell::S(doc_id),
        date,
        Cell::S("1b"),
        Cell::Empty,
        Cell::Empty,
        Cell::S(position),
        Cell::S("müddet-i örfiyye"),
        Cell::N(150.0),
        Cell::N(40.0),
        Cell::N(0.0),
        Cell::N(12.0),
        Cell::S("Gzperson Eski "),
        Cell::S("Gzperson Yeni"),
        Cell::S(old_place),
        Cell::S(new_place),
        Cell::S(text),
    ]
}

/// One gazetteer row; the 8th cell (longitude) sits under no header.
fn gaz_row(
    name: &'static str,
    matched: &'static str,
    qid: &'static str,
    country: &'static str,
    lat: &'static str,
    lon: &'static str,
) -> Vec<Cell> {
    vec![
        Cell::S(name),
        Cell::S(matched),
        Cell::S(qid),
        Cell::Empty,
        Cell::S(country),
        Cell::Empty,
        Cell::S(lat),
        Cell::S(lon),
    ]
}

#[tokio::test]
async fn delivered_workbook_with_untrusted_gazetteer() {
    let (app, pool) = common::setup_app().await;

    let gazetteer = vec![
        gaz_row(
            "Gztown Alfa",
            "Gztown Alfa",
            "Q4242001",
            "Turkey",
            "38.4192",
            "27.1287",
        ),
        // wrong match on the other side of the world
        gaz_row(
            "Gztown Kudüs",
            "Kudus",
            "Q4242002",
            "Indonesia",
            "-6.8",
            "110.84",
        ),
        // a whole country's centroid
        gaz_row("Gztown Mısır", "Egypt", "Q4242003", "Egypt", "27.", "29."),
    ];
    const TEXT_A: &str = "Gztown Alfa kadısı Gzperson Eski sene-i mezbure gurresinden ref ve \
                          yeri yevmi kırk akçe ile tevcih olunub sadaka buyuruldu (A)";
    let appointments = vec![
        delivered_row(
            "### Gzdoc.docx_1",
            Cell::N(1161.0),
            "te’bid",
            "Gztown Kudüs ",
            "Gztown Alfa ",
            TEXT_A,
        ),
        // the same register entry, transcribed again in a copied document
        delivered_row(
            "### Gzdoc (1).docx_1",
            Cell::N(1161.0),
            "te'bid",
            "Gztown Kudüs ",
            "Gztown Alfa ",
            TEXT_A,
        ),
        delivered_row(
            "### Gzdoc.docx_2",
            Cell::N(1556.0),
            "maʻişet-i",
            "Gztown Alfa ",
            "ERROR",
            "Gztown Beta kadısı ... a different entry whose date was mis-transcribed (B)",
        ),
        delivered_row(
            "### Gzdoc.docx_3",
            Cell::S("1167]"),
            "tebid",
            "Gztown Alfa ",
            "Gztown Mısır ",
            "Gztown Mısır kadılığı ... yet another distinct register entry for the test (C)",
        ),
    ];
    let (_d, path) = common::write_workbook(&[
        ("atamalar", common::DELIVERED_HEADERS, &appointments),
        ("kazalar", common::GAZETTEER_HEADERS, &gazetteer),
    ]);

    let report = import_path(&pool, &path, &ImportOptions::default())
        .await
        .unwrap();
    // gazetteer first, whatever the sheet order
    assert_eq!(report.sheets[0].0, "kazalar");
    assert_eq!(report.sheets[1].0, "atamalar");
    let g = &report.gazetteer;
    assert_eq!((g.accepted, g.needs_review, g.rejected), (1, 1, 1));
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("no longitude header")));
    assert_eq!(report.implausible_years.get("1556"), Some(&1));
    assert_eq!(report.place_cells_error, 1);

    // only the accepted point reaches places
    let place = |name: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_as::<
                _,
                (
                    i64,
                    Option<f64>,
                    Option<f64>,
                    Option<String>,
                    Option<String>,
                ),
            >(
                "SELECT id, latitude, longitude, coordinate_source, wikidata_qid \
                 FROM places WHERE normalized_name = $1",
            )
            .bind(name)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let alfa = place("gztown alfa").await;
    assert_eq!((alfa.1, alfa.2), (Some(38.4192), Some(27.1287)));
    assert_eq!(alfa.3.as_deref(), Some("gazetteer"));
    assert_eq!(alfa.4.as_deref(), Some("Q4242001"));
    let kudus = place("gztown kudus").await;
    assert_eq!(
        (kudus.1, kudus.3.as_deref(), kudus.4.as_deref()),
        (None, None, None)
    );
    let misir = place("gztown misir").await;
    assert_eq!(misir.1, None);
    // `ERROR` never became a place
    let error_places: i64 =
        sqlx::query_scalar("SELECT count(*) FROM places WHERE normalized_name = 'error'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(error_places, 0);

    // duplicate transcription: kept as evidence, hidden from the event list
    let (all, visible): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM appointment_records a JOIN source_records s ON s.id = a.source_record_id \
                  WHERE s.doc_id LIKE '### Gzdoc%'), \
                (SELECT count(*) FROM appointments a JOIN source_records s ON s.id = a.source_record_id \
                  WHERE s.doc_id LIKE '### Gzdoc%')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((all, visible), (4, 3));
    let kept_doc: String = sqlx::query_scalar(
        "SELECT s.doc_id FROM appointment_records d \
         JOIN appointment_records k ON k.id = d.duplicate_of \
         JOIN source_records s ON s.id = k.source_record_id \
         JOIN source_records ds ON ds.id = d.source_record_id \
         WHERE ds.doc_id = '### Gzdoc (1).docx_1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(kept_doc, "### Gzdoc.docx_1");

    // dates: implausible year kept as text only; `1167]` still parses
    let years: Vec<(String, Option<String>, Option<i32>)> = sqlx::query_as(
        "SELECT s.doc_id, a.year_original, a.year_numeric FROM appointments a \
         JOIN source_records s ON s.id = a.source_record_id \
         WHERE s.doc_id LIKE '### Gzdoc%' ORDER BY s.doc_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        years[1],
        ("### Gzdoc.docx_2".into(), Some("1556".into()), None)
    );
    assert_eq!(years[2].2, Some(1167));

    // vocabulary variants folded, and API filters fold their input too
    let (s, body) = get(
        &app,
        &format!(
            "/api/v1/appointments?position_type=te%E2%80%99bid&place={}",
            alfa.0
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["total"], 2, "{body}");
    assert_eq!(body["items"][0]["position_type"], "tebid");

    // review surface
    let (_, body) = get(&app, "/api/v1/gazetteer?status=rejected&q=gztown").await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["original_name"], "Gztown Kudüs");
    assert_eq!(body["items"][0]["issues"][0], "outside_study_region");
    assert_eq!(body["items"][0]["place_id"], kudus.0);
    let (_, body) = get(&app, &format!("/api/v1/places/{}", misir.0)).await;
    assert_eq!(body["latitude"], Value::Null);
    assert_eq!(body["gazetteer"]["coordinate_status"], "needs_review");

    // re-import is idempotent
    let again = import_path(&pool, &path, &ImportOptions::default())
        .await
        .unwrap();
    assert_eq!(again.source_records_inserted, 0);
    assert_eq!(again.gazetteer.inserted, 0);
    assert_eq!(again.gazetteer.coordinates_applied, 0);

    // a corrected delivery that moves Alfa out of the study region withdraws
    // its point instead of keeping a stale one
    let corrected = vec![gaz_row(
        "Gztown Alfa",
        "Alfa, Chile",
        "Q4242001",
        "Chile",
        "-33.4",
        "-70.6",
    )];
    let (_d2, path2) =
        common::write_workbook(&[("kazalar", common::GAZETTEER_HEADERS, &corrected)]);
    let r = import_path(&pool, &path2, &ImportOptions::default())
        .await
        .unwrap();
    let alfa = place("gztown alfa").await;
    assert_eq!(
        (alfa.1, alfa.3.as_deref()),
        (None, None),
        "{:?}",
        r.gazetteer
    );
    assert!(
        r.gazetteer.coordinates_withdrawn >= 1,
        "withdrawal not reported: {:?}",
        r.gazetteer
    );
}

#[tokio::test]
async fn query_page_charts_and_export() {
    let (app, pool) = common::setup_app().await;

    let rows = vec![
        row(
            "qry-1",
            Cell::S("Qkadi Old One"),
            Cell::S("Qkadi New One"),
            Cell::S("Qadana"),
            Cell::S("Qmisis"),
            Cell::S("1150"),
            Cell::Empty,
            Some((37.0, 35.3, 36.95, 35.6)),
        ),
        row(
            "qry-2",
            Cell::S("Qkadi Old Two"),
            Cell::S("Qkadi New Two"),
            Cell::S("Qmisis"),
            Cell::S("Qadana"),
            Cell::S("1151"),
            Cell::Empty,
            Some((36.95, 35.6, 37.0, 35.3)),
        ),
        // a longer name that starts with "Qmisis", never geocoded
        row(
            "qry-3",
            Cell::S("Qkadi Old Three"),
            Cell::S("Qkadi New One"),
            Cell::S("Qadana"),
            Cell::S("Qmisis maa Ayas"),
            Cell::S("1160"),
            Cell::Empty,
            None,
        ),
        row(
            "qry-4",
            Cell::S("Qkadi Old Four"),
            Cell::S("Qkadi New Four"),
            Cell::S("Qadana"),
            Cell::S("Qtarsus"),
            Cell::S("-"),
            Cell::Empty,
            None,
        ),
    ];
    let (_d, path) = write_fixture(STD_HEADERS, &rows);
    import_path(&pool, &path, &ImportOptions::default())
        .await
        .unwrap();

    // prefix group + direction
    let (s, body) = get(
        &app,
        "/api/v1/appointments?place_prefix=Qmisis&direction=arrival",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        body["total"], 2,
        "qry-1 and qry-3 arrive at a Qmisis* place"
    );
    let (_s, body) = get(
        &app,
        "/api/v1/appointments?place_prefix=qmisis&direction=departure",
    )
    .await;
    assert_eq!(body["total"], 1);
    let (_s, body) = get(
        &app,
        "/api/v1/appointments?place_prefix=QMISIS&sort=destination",
    )
    .await;
    assert_eq!(body["total"], 3);
    assert_eq!(body["items"][0]["raw_new_place"], "Qadana");

    for bad in [
        "/api/v1/appointments?direction=sideways",
        "/api/v1/appointments?sort=nope",
        "/api/v1/appointments?persons=1,x",
        "/api/v1/stats?by=colour",
        "/api/v1/places?q=a&match=fuzzy",
    ] {
        let (s, _) = get(&app, bad).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }

    // ids for highlighting a route
    let (_s, body) = get(
        &app,
        "/api/v1/appointments/ids?place_prefix=qmisis&direction=arrival",
    )
    .await;
    assert_eq!(body["total"], 2);
    assert_eq!(body["ids"].as_array().unwrap().len(), 2);

    // charts
    let (s, body) = get(&app, "/api/v1/stats?by=destination&place_prefix=qmisis").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["records"], 3);
    let labels: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"Qmisis") && labels.contains(&"Qmisis maa Ayas"));

    let (_s, body) = get(&app, "/api/v1/stats?by=decade&place_prefix=qmisis").await;
    let decades: Vec<(i64, i64)> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["value"].as_i64().unwrap(), i["count"].as_i64().unwrap()))
        .collect();
    assert_eq!(decades, [(1150, 2), (1160, 1)]);

    let (_s, body) = get(&app, "/api/v1/stats?by=new_kadi&place=0").await;
    assert_eq!(body["records"], 0);

    let new_one: i64 =
        sqlx::query_scalar("SELECT id FROM persons WHERE normalized_name = 'qkadi new one'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let (_s, body) = get(&app, &format!("/api/v1/stats?by=person&persons={new_one}")).await;
    assert_eq!(body["records"], 2);

    // summary: how much of a selection can be drawn
    let (_s, body) = get(&app, "/api/v1/stats/summary?place_prefix=qmisis").await;
    assert_eq!(body["records"], 3);
    assert_eq!(body["mapped"], 2);
    let (_s, body) = get(&app, "/api/v1/stats/summary?place_prefix=qadana").await;
    assert_eq!(body["records"], 4);
    assert_eq!(body["undated"], 1);

    // prefix search for the place picker
    let (_s, body) = get(&app, "/api/v1/places?q=qmisis&match=prefix").await;
    assert_eq!(body["total"], 2);
    let (_s, body) = get(&app, "/api/v1/places?q=ayas&match=prefix").await;
    assert_eq!(body["total"], 0);
    let (_s, body) = get(&app, "/api/v1/places?q=ayas").await;
    assert!(body["total"].as_i64().unwrap() >= 1);

    // CSV export
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/export/appointments.csv?place_prefix=qmisis")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/csv"));
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let csv = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(csv.starts_with("\u{feff}appointment_id,doc_id"));
    assert_eq!(csv.lines().count(), 4, "header + 3 records");
    assert!(csv.contains("Qmisis maa Ayas"));
    // 1150 H began in 1737
    assert!(csv.contains(",1150,1737"), "{csv}");

    // open deployment: everybody is admin, no login
    let (_s, body) = get(&app, "/api/v1/session").await;
    assert_eq!(body["role"], "admin");
    assert_eq!(body["login_enabled"], false);
}

#[tokio::test]
async fn user_view_hides_source_texts_until_admin_login() {
    use base64::Engine;
    use kadi_atlas::app::{build_router, AppState};

    let pool = common::setup_pool().await;
    let mut cfg = common::test_config();
    cfg.admin_password = Some("test-parola-123".into());
    let app = build_router(AppState::new(pool, &cfg), &cfg);

    let send = |uri: &str, header: Option<(&'static str, String)>| {
        let mut request = Request::builder().uri(uri);
        if let Some((name, value)) = header {
            request = request.header(name, value);
        }
        app.clone().oneshot(request.body(Body::empty()).unwrap())
    };

    let (_s, body) = get(&app, "/api/v1/session").await;
    assert_eq!(body["role"], "user");
    assert_eq!(body["login_enabled"], true);

    let (s, body) = get(&app, "/api/v1/sources").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");
    let (s, _) = get(&app, "/api/v1/sources/1").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // the rest of the API is public
    let (s, _) = get(&app, "/api/v1/appointments?limit=1").await;
    assert_eq!(s, StatusCode::OK);

    let resp = send("/admin/login", None).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert!(resp.headers().contains_key("www-authenticate"));

    let basic = |pw: &str| {
        let token = base64::engine::general_purpose::STANDARD.encode(format!("admin:{pw}"));
        Some(("authorization", format!("Basic {token}")))
    };
    let resp = send("/admin/login", basic("wrong-password")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let resp = send("/admin/login", basic("test-parola-123"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let cookie = resp.headers()["set-cookie"].to_str().unwrap().to_string();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
    let pair = cookie.split(';').next().unwrap().to_string();

    let resp = send("/api/v1/sources?limit=1", Some(("cookie", pair.clone())))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = send("/api/v1/session", Some(("cookie", pair)))
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["role"], "admin");

    let resp = send("/admin/logout", None).await.unwrap();
    assert!(resp.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .contains("Max-Age=0"));
}
