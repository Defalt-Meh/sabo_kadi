# Free hosting: Render (app) + Neon (database)

1. Neon (neon.com, Free plan): create a project, Postgres 17, region near your
   visitors. Click Connect, turn Connection pooling OFF, copy the connection
   string. No SQL needed; the app enables PostGIS and migrates on first start.
2. Render (render.com, sign in with GitHub): New → Blueprint → this repo, branch
   main. Paste the Neon string as DATABASE_URL. Deploy (first Rust build takes
   several minutes). Open the https://kadi-atlas-….onrender.com URL.
3. Data: on first start against an empty DB it imports KADI_SEED_URL, else
   deploy/seed/*.xlsx, else a 34-row synthetic demo set. It never re-imports
   once the DB has records.

Free-tier notes: Render sleeps after 15 min without traffic (next visit wakes it
in ~1 min — open it before a demo); Neon suspends after 5 min idle and wakes in
under a second.

Admin view / user view: set `KADI_ATLAS__ADMIN_PASSWORD` (Render → the service →
Environment, at least 8 characters) to hide source texts from visitors. Admins
open `https://…onrender.com/admin/login`, enter the password (any user name) and
see everything; `/admin/logout` ends the session. Without the variable, every
visitor gets the full admin view.

Adding data later (e.g. the next delivery of records): the seed only runs on an
empty database, so import new workbooks directly into Neon from your computer:

    DATABASE_URL='<the Neon connection string>' cargo run --release --bin import_xlsx -- data/new_delivery.xlsx

The import is idempotent on `doc_id` (a workbook can be imported again safely)
and the site shows the new records immediately. Step-by-step instructions in
Turkish: `docs/KULLANIM_VE_BAKIM.md`.
