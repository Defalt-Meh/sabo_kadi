"""Synthetic kadı-appointment workbooks for testing. Nothing in them is real.

    python3 scripts/make_fixture.py demo  out.xlsx   # 34 hand-made rows (e2e tests)
    python3 scripts/make_fixture.py scale out.xlsx   # ~70k generated rows (load tests)

Writes a minimal .xlsx (inline strings) in the column layout the importer
expects, so no third-party Python packages are needed.
"""
import random
import sys
import zipfile
from xml.sax.saxutils import escape

HEADERS = ["doc_id", "degree", "position type", "period", "salary", "old_salary", "asitane",
           "infisal", "old_kadi", "new_kadi", "old_place", "new_place", "tarih", "varak_no",
           "region", "certificate", "text", "old_latitude", "old_longitude", "new_latitude",
           "new_longitude"]

# real present-day coordinates; everything else in the fixtures is invented
CITIES = {
    "İstanbul": (41.01, 28.98), "Edirne": (41.68, 26.56), "Bursa": (40.19, 29.06),
    "İzmir": (38.42, 27.14), "Konya": (37.87, 32.48), "Ankara": (39.93, 32.86),
    "Kayseri": (38.72, 35.48), "Sivas": (39.75, 37.02), "Halep": (36.20, 37.16),
    "Şam": (33.51, 36.29), "Kudüs": (31.77, 35.23), "Kahire": (30.04, 31.24),
    "Selanik": (40.64, 22.94), "Sofya": (42.70, 23.32), "Belgrad": (44.82, 20.46),
    "Saraybosna": (43.86, 18.41), "Trabzon": (41.00, 39.72), "Erzurum": (39.90, 41.27),
    "Diyarbakır": (37.91, 40.23), "Bağdat": (33.31, 44.36), "Mekke": (21.42, 39.83),
    "Medine": (24.47, 39.61), "Rodos": (36.43, 28.22), "Kandiye": (35.34, 25.13),
    "Filibe": (42.15, 24.75), "Manisa": (38.61, 27.43), "Amasya": (40.65, 35.83),
    "Tokat": (40.31, 36.55), "Kütahya": (39.42, 29.98), "Antalya": (36.89, 30.71),
}

# (doc, old_kadi, new_kadi, old_place, new_place, tarih, degree, type)
DEMO_ROWS = [
    ("E-001", "Ali Efendi", "Ahmed Efendi", "Kütahya", "Bursa", "Receb 1223", "Mevleviyet", "tevcih"),
    ("E-002", "Veli Efendi", "Ahmed Efendi", "Edirne", "Selanik", "Şaban 1224", "Mevleviyet", "nakil"),
    ("E-003", "Rıza Efendi", "Ahmed Efendi", "Bursa", "Edirne", "Muharrem 1224", "Mevleviyet", "nakil"),
    ("E-004", "Nuri Efendi", "Ahmed Efendi", "Selanik", "İstanbul", "1227", "Mevleviyet", "tevcih"),
    ("E-005", "Ahmed Efendi", "Hasan Efendi", "Ankara", "İstanbul", "1229", "Mevleviyet", "tevcih"),
    ("E-010", "Halil Efendi", "Mehmed Said Efendi", "Konya", "Kayseri", "1210", "Kaza", "tevcih"),
    ("E-011", "Osman Efendi", "Mehmed Said Efendi", "Sivas", "Tokat", "1212", "Kaza", "nakil"),
    ("E-012", "Kâmil Efendi", "Mehmed Said Efendi", "Kayseri", "Sivas", "Zilhicce 1212", "Kaza", "nakil"),
    ("E-013", "Sadık Efendi", "Mehmed Said Efendi", "Tokat", "Amasya", "evahir-i Muharrem", "Kaza", "ibka"),
    ("E-014", "Tahir Efendi", "Mehmed Said Efendi", "Amasya", "Trabzon", "1216", "Kaza", "nakil"),
    ("E-015", "Zeki Efendi", "Mehmed Said Efendi", "Erzurum", "Diyarbakır", "1219", "Mevleviyet", "tevcih"),
    ("E-016", "Fehmi Efendi", "Mehmed Said Efendi", "Diyarbakır", "Halep", "1221", "Mevleviyet", "nakil"),
    ("E-020", "Rüşdi Efendi", "es-Seyyid Abdullah Efendi", "Halep", "Şam", "1230", "Mevleviyet", "tevcih"),
    ("E-021", "Arif Efendi", "es-Seyyid Abdullah Efendi", "Şam", "Kudüs", "1231", "Mevleviyet", "nakil"),
    ("E-022", "Şakir Efendi", "es-Seyyid Abdullah Efendi", "Kudüs", "Kahire", "1233", "Mevleviyet", "nakil"),
    ("E-023", "Lütfi Efendi", "es-Seyyid Abdullah Efendi", "Kahire", "Mekke", "1235", "Mevleviyet", "nakil"),
    ("E-024", "Emin Efendi", "es-Seyyid Abdullah Efendi", "Mekke", "Medine", "1236", "Mevleviyet", "nakil"),
    ("E-030", "Refik Efendi", "Mustafa Efendi", "Sofya", "Filibe", "1215", "Kaza", "tevcih"),
    ("E-031", "Naci Efendi", "Mustafa Efendi", "Filibe", "Edirne", "1217", "Kaza", "nakil"),
    ("E-032", "Vehbi Efendi", "Mustafa Efendi", "Edirne", "Belgrad", "1219", "Mevleviyet", "nakil"),
    ("E-033", "Cemil Efendi", "Mustafa Efendi", "Belgrad", "Saraybosna", "1222", "Mevleviyet", "nakil"),
    ("E-040", "Sabri Efendi", "İbrahim Efendi", "İzmir", "Manisa", "1225", "Kasaba", "tevcih"),
    ("E-041", "Hilmi Efendi", "İbrahim Efendi", "Manisa", "Rodos", "1227", "Kaza", "nakil"),
    ("E-042", "Hamdi Efendi", "İbrahim Efendi", "Rodos", "Kandiye", "1229", "Kaza", "nakil"),
    ("E-043", "Asım Efendi", "İbrahim Efendi", "Kandiye", "Nevrekop", "1230", "Kaza", "nakil"),
    ("E-050", "Kemal Efendi", "Hasan Efendi", "Ankara", "Konya", "1240", "Kaza", "tevcih"),
    ("E-051", "Nazım Efendi", "Hasan Efendi", "Konya", "Antalya", "1241", "Kaza", "nakil"),
    ("E-052", "Tevfik Efendi", "Hasan Efendi", "Antalya", "İzmir", "1243", "Kaza", "nakil"),
    ("E-060", "Selim Efendi", "Yusuf Efendi", "Bağdat", "Halep", "1238", "Mevleviyet", "tevcih"),
    ("E-061", "Rauf Efendi", "Yusuf Efendi", "Halep", "Şam", "1239", "Mevleviyet", "nakil"),
    ("E-062", "Bekir Efendi", "Ömer Efendi", "Bursa", "İstanbul", "1226", "Mevleviyet", "tevcih"),
    ("E-063", "Recep Efendi", "Ömer Efendi", "Kütahya", "Bursa", "1218", "Kaza", "tevcih"),
    ("E-064", "Salih Efendi", "Hüseyin Efendi", "Edirne", "İstanbul", "1224", "Mevleviyet", "tevcih"),
    ("E-065", "Faik Efendi", "Hüseyin Efendi", "Selanik", "Edirne", "1220", "Mevleviyet", "tevcih"),
]


def demo_rows():
    for n, (doc, old, new, origin, dest, tarih, degree, kind) in enumerate(DEMO_ROWS, start=2):
        yield row(doc, old, new, origin, dest, tarih, degree, kind, n,
                  CITIES.get(origin), CITIES.get(dest))


def scale_rows(target=70_000, seed=1453):
    rng = random.Random(seed)
    given = ["Ahmed", "Mehmed", "Mustafa", "Ali", "Hasan", "Hüseyin", "İbrahim", "Osman", "Abdullah",
             "Yusuf", "Ömer", "Süleyman", "Halil", "İsmail", "Salih", "Abdurrahman", "Seyyid Ali",
             "Feyzullah", "Esad", "Said", "Zeynelabidin", "Abdülkadir", "Ebubekir", "Murtaza"]
    second = ["", "Said", "Emin", "Arif", "Sadık", "Tahir", "Nuri", "Rıza", "Şakir", "Lütfi", "Fehmi",
              "Hilmi", "Kâmil", "Refik", "Vehbi", "Cemil", "Sabri", "Hamdi", "Asım", "Nazım"]
    title = ["Efendi", "Efendi", "Efendi", "Bey", "Molla", "Çelebi"]
    persons = list({" ".join(filter(None, [rng.choice(["", "", "es-Seyyid", "el-Hac"]), g, s, t]))
                    for g in given for s in second for t in title})
    rng.shuffle(persons)
    persons = persons[:9000]

    # ~1500 places around the real cities; about 60% geocoded
    stems = ["Yeni", "Eski", "Kara", "Ak", "Kızıl", "Büyük", "Küçük", "Orta", "Aşağı", "Yukarı"]
    places = {}
    for city, (lat, lon) in CITIES.items():
        places[city] = (lat, lon)
        for i in range(50):
            name = f"{rng.choice(stems)}{city.lower()} {i + 1}" if i else f"{city} kazası"
            geocoded = rng.random() < 0.6
            places[name] = (round(lat + rng.uniform(-1.6, 1.6), 3),
                            round(lon + rng.uniform(-1.6, 1.6), 3)) if geocoded else None
    names = list(places)
    months = ["Muharrem", "Safer", "Rebiülevvel", "Receb", "Şaban", "Ramazan", "Şevval", "Zilhicce"]
    degrees = ["Mevleviyet", "Kaza", "Kasaba", "Nahiye"]
    kinds = ["tevcih", "nakil", "ibka", "infisal"]

    n = 0
    while n < target:
        for person in persons:
            if n >= target:
                break
            year = rng.randint(1100, 1320)
            here = rng.choice(names)
            for _ in range(rng.randint(1, 15)):
                if n >= target:
                    break
                # mostly chain on; sometimes a gap in the record
                origin = here if rng.random() < 0.85 else rng.choice(names)
                dest = rng.choice(names)
                year += rng.choice([0, 0, 1, 1, 2, 3])
                r = rng.random()
                tarih = (str(year) if r < 0.5 else f"{rng.choice(months)} {year}" if r < 0.95
                         else f"evahir-i {rng.choice(months)}")
                n += 1
                predecessor = rng.choice(persons) if rng.random() < 0.7 else f"Kadı {n}"
                yield row(f"S-{n:06d}", predecessor, person, origin, dest, tarih,
                          rng.choice(degrees), rng.choice(kinds), n + 1,
                          places[origin], places[dest])
                here = dest


def row(doc, old, new, origin, dest, tarih, degree, kind, n, o, d):
    text = (f"{origin} kazası kadısı {old} ma'zul olup yerine {dest} kazasına {new} {kind} olundu. "
            f"Tarih: {tarih}. Derece: {degree.lower()}.")
    region = "Rumeli" if (o and o[1] < 26.5) else "Anadolu"
    return [doc, degree, kind, "1 sene", str(300 + n % 900), str(250 + n % 900), None, None,
            old, new, origin, dest, tarih, str(10 + n % 500), region, None, text,
            o[0] if o else None, o[1] if o else None, d[0] if d else None, d[1] if d else None]


def col(i):
    s = ""
    i += 1
    while i:
        i, r = divmod(i - 1, 26)
        s = chr(65 + r) + s
    return s


def cell(ref, value):
    if value is None:
        return ""
    if isinstance(value, (int, float)):
        return f'<c r="{ref}"><v>{value}</v></c>'
    return f'<c r="{ref}" t="inlineStr"><is><t>{escape(str(value))}</t></is></c>'


def write_xlsx(path, rows):
    def sheet():
        yield ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet '
               'xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>')
        for r, values in enumerate([HEADERS, *rows], start=1):
            yield f'<row r="{r}">' + "".join(cell(f"{col(i)}{r}", v) for i, v in enumerate(values)) + "</row>"
        yield "</sheetData></worksheet>"

    rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    pkg = "http://schemas.openxmlformats.org/package/2006"
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml",
                   f'<?xml version="1.0" encoding="UTF-8"?><Types xmlns="{pkg}/content-types">'
                   '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
                   '<Default Extension="xml" ContentType="application/xml"/>'
                   '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
                   '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>')
        z.writestr("_rels/.rels",
                   f'<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{pkg}/relationships">'
                   f'<Relationship Id="rId1" Type="{rel}/officeDocument" Target="xl/workbook.xml"/></Relationships>')
        z.writestr("xl/workbook.xml",
                   '<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
                   f'xmlns:r="{rel}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>')
        z.writestr("xl/_rels/workbook.xml.rels",
                   f'<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{pkg}/relationships">'
                   f'<Relationship Id="rId1" Type="{rel}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>')
        z.writestr("xl/worksheets/sheet1.xml", "".join(sheet()))


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] not in ("demo", "scale"):
        sys.exit(__doc__)
    rows = list(demo_rows() if sys.argv[1] == "demo" else scale_rows())
    write_xlsx(sys.argv[2], rows)
    print(f"{len(rows)} rows -> {sys.argv[2]}")
