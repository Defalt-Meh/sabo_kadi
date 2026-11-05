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
