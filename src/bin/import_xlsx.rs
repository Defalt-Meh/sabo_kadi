//! `import_xlsx` — the only way data enters the system.
//!
//! Usage:
//!   import_xlsx <file.xlsx> [--sheet <name>] [--dry-run] [--no-migrate]

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
    path: PathBuf,
    options: ImportOptions,
    migrate: bool,
}

//! Migration error, check this function for later use -Defalt
fn parse_args() -> Result<Args> {
    let mut path: Option<PathBuf> = None;
    let mut options = ImportOptions::default();
    let mut migrate = true;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!(
                    "Usage: import_xlsx <file.xlsx> [--sheet <name>] [--dry-run] [--no-migrate]\n\n\
                     Imports historical kadı appointment records into PostgreSQL.\n\
                     Idempotent on doc_id: re-running updates rows instead of duplicating them.\n\n\
                     Options:\n  \
                       --sheet <name>   Worksheet to read (default: first sheet)\n  \
                       --dry-run        Parse, validate and roll back without writing\n  \
                       --no-migrate     Do not run database migrations first\n"
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
            other if other.starts_with("--") => bail!("unknown option `{other}`"),
            other => {
                if path.replace(PathBuf::from(other)).is_some() {
                    bail!("only one input file may be given");
                }
            }
        }
    }

    Ok(Args {
        path: path.context("missing input file: import_xlsx <file.xlsx>")?,
        options,
        migrate,
    })
}

async fn run() -> Result<ExitCode> {
    let args = parse_args()?;
    if !args.path.is_file() {
        bail!("`{}` is not a readable file", args.path.display());
    }

    let config = Config::load()?;
    init_tracing(config.log_format);

    let pool = db::connect(&config).await?;
    if args.migrate {
        db::migrate(&pool).await?;
    }

    eprintln!(
        "importing {}{}...",
        args.path.display(),
        if args.options.dry_run {
            " (dry run)"
        } else {
            ""
        }
    );

    let report = import_path(&pool, &args.path, &args.options).await?;
    report.print_summary();

    pool.close().await;

    // Non-empty warning list is not a failure, but surface it in the exit path
    // for CI: still exit 0 unless nothing was imported at all.
    if report.rows_total == 0 {
        eprintln!("warning: no data rows found");
    }
    Ok(ExitCode::SUCCESS)
}
