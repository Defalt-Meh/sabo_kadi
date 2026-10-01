#![allow(dead_code)]
//! Shared helpers for integration tests.
//!
//! These tests need a live PostgreSQL + PostGIS database. Point them at a
//! throwaway database with `TEST_DATABASE_URL` (defaults to the local dev
//! cluster used in the README).

use std::path::PathBuf;

use kadi_atlas::app::{build_router, AppState};
use kadi_atlas::config::{Config, LogFormat};
use kadi_atlas::db;
use rust_xlsxwriter::Workbook;
use sqlx::PgPool;
use std::time::Duration;

pub fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgres://kadi:kadi@127.0.0.1:5433/kadi_atlas_test".to_string())
}

pub fn test_config() -> Config {
    Config {
        database_url: test_database_url(),
        database_max_connections: 5,
        database_acquire_timeout: Duration::from_secs(10),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        default_page_size: 50,
        max_page_size: 200,
        request_timeout: Duration::from_secs(30),
        auto_migrate: true,
        log_format: LogFormat::Pretty,
        cors_allow_origin: None,
        web_dir: None,
    }
}

/// A connection pool with migrations applied.
pub async fn setup_pool() -> PgPool {
    let cfg = test_config();
    let pool = db::connect(&cfg).await.expect("connect to test database");
    db::migrate(&pool).await.expect("run migrations");
    pool
}

pub async fn setup_app() -> (axum::Router, PgPool) {
    let pool = setup_pool().await;
    let cfg = test_config();
    let state = AppState::new(pool.clone(), &cfg);
    (build_router(state, &cfg), pool)
}

#[derive(Clone)]
pub enum Cell {
    S(&'static str),
    N(f64),
    Empty,
}

/// Write a `.xlsx` fixture with the given header row and data rows into a fresh
/// temp file; returns the path (kept alive by the returned `TempDir`).
pub fn write_fixture(headers: &[&str], rows: &[Vec<Cell>]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("fixture.xlsx");

    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Kayitlar").unwrap();

    for (c, h) in headers.iter().enumerate() {
        sheet.write(0, c as u16, *h).unwrap();
    }
    for (r, row) in rows.iter().enumerate() {
        let excel_row = (r + 1) as u32;
        for (c, cell) in row.iter().enumerate() {
            let col = c as u16;
            match cell {
                Cell::S(s) => {
                    sheet.write(excel_row, col, *s).unwrap();
                }
                Cell::N(n) => {
                    sheet.write(excel_row, col, *n).unwrap();
                }
                Cell::Empty => {}
            }
        }
    }

    workbook.save(&path).expect("save xlsx fixture");
    (dir, path)
}

/// The full standard column set, in the order the project documents.
pub const STD_HEADERS: &[&str] = &[
    "doc_id",
    "degree",
    "position type",
    "period",
    "salary",
    "old_salary",
    "asitane",
    "infisal",
    "old_kadi",
    "new_kadi",
    "old_place",
    "new_place",
    "tarih",
    "varak_no",
    "region",
    "certificate",
    "text",
    "old_latitude",
    "old_longitude",
    "new_latitude",
    "new_longitude",
];
