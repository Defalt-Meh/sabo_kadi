# Seed data for hosted demos

Put `.xlsx` workbooks here to have the container import them automatically the
first time it starts against an empty database (alphabetical order, so name the
appointments workbook so it sorts first). Anything committed here is public if
the repo is public; to keep a workbook out of git, host it with a direct
download link and set `KADI_SEED_URL` instead. With neither, a small SYNTHETIC
demo set is loaded. To re-import, empty the database and restart the service.
