//! `import_xlsx` — the only way data enters the system.
//!
//! Usage:
//!   import_xlsx <file.xlsx> [--sheet <name>] [--dry-run] [--no-migrate]
//!   import_xlsx --migrate-only

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};

use kadi_atlas::app::init_tracing;
use kadi_atlas::config::Config;
use kadi_atlas::db;
use kadi_atlas::importer::{import_path, ImportOptions};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("\nimport failed: {err:#}");
            ExitCode::FAILURE
        }
    }
}

struct Args {
    /// `None` with `migrate_only`.
    path: Option<PathBuf>,
    options: ImportOptions,
    migrate: bool,
}

fn parse_args() -> Result<Args> {
    let mut path: Option<PathBuf> = None;
    let mut options = ImportOptions::default();
    let mut migrate = true;
    let mut migrate_only = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!(
                    "Usage: import_xlsx <file.xlsx> [--sheet <name>] [--dry-run] [--no-migrate]\n       \
                     import_xlsx --migrate-only\n\n\
                     Imports historical kadı appointment records into PostgreSQL.\n\
                     Idempotent on doc_id: re-running updates rows instead of duplicating them.\n\n\
                     Options:\n  \
                       --sheet <name>   Worksheet to read (default: first sheet)\n  \
                       --dry-run        Parse, validate and roll back without writing\n  \
                       --no-migrate     Do not run database migrations first\n  \
                       --migrate-only   Apply pending migrations and exit (no file needed)\n"
                );
                std::process::exit(0);
            }
            "--sheet" => {
                options.sheet = Some(
                    args.next()
                        .context("--sheet requires a worksheet name argument")?,
                );
            }
            "--dry-run" => options.dry_run = true,
            "--no-migrate" => migrate = false,
            "--migrate-only" => migrate_only = true,
            other if other.starts_with("--") => bail!("unknown option `{other}`"),
            other => {
                if path.replace(PathBuf::from(other)).is_some() {
                    bail!("only one input file may be given");
                }
            }
        }
    }

    if migrate_only {
        if path.is_some() || !migrate {
            bail!("--migrate-only takes no input file and cannot be combined with --no-migrate");
        }
    } else if path.is_none() {
        bail!("missing input file: import_xlsx <file.xlsx>");
    }

    Ok(Args {
        path,
        options,
        migrate,
    })
}

async fn run() -> Result<ExitCode> {
    let args = parse_args()?;
    if let Some(path) = &args.path {
        if !path.is_file() {
            bail!("`{}` is not a readable file", path.display());
        }
    }

    let config = Config::load()?;
    init_tracing(config.log_format);

    let pool =
        db::connect_with_slow_threshold(&config, std::time::Duration::from_secs(120)).await?;
    if args.migrate {
        db::migrate(&pool).await?;
    }
    let Some(path) = args.path else {
        eprintln!("migrations up to date");
        pool.close().await;
        return Ok(ExitCode::SUCCESS);
    };

    eprintln!(
        "importing {}{}...",
        path.display(),
        if args.options.dry_run {
            " (dry run)"
        } else {
            ""
        }
    );

    let report = import_path(&pool, &path, &args.options).await?;
    report.print_summary();

    pool.close().await;

    // Non-empty warning list is not a failure, but surface it in the exit path
    // for CI: still exit 0 unless nothing was imported at all.
    if report.rows_total == 0 {
        eprintln!("warning: no data rows found");
    }
    Ok(ExitCode::SUCCESS)
}
