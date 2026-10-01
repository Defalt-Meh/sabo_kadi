import json, sys
# Regenerates web/data/basemap.json from Natural Earth 1:50m GeoJSON:
#   curl -O https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_50m_{land,lakes,rivers_lake_centerlines}.geojson
#   python3 scripts/build_basemap.py <dir with the downloads> web/data/basemap.json
SP = sys.argv[1]; OUT = sys.argv[2]
W, E, S, N = -14.0, 68.0, 8.0, 58.0   # Balkans, N. Africa, Levant, Arabia, Persia, Crimea

def clip(ring):
    # Sutherland–Hodgman against the bbox; fine for filled polygons.
    def edge(pts, inside, inter):
        out = []
        for i, cur in enumerate(pts):
            prev = pts[i - 1]
            if inside(cur):
                if not inside(prev): out.append(inter(prev, cur))
                out.append(cur)
            elif inside(prev):
                out.append(inter(prev, cur))
        return out
    def ix(x):
        return lambda a, b: (x, a[1] + (b[1] - a[1]) * (x - a[0]) / (b[0] - a[0]))
    def iy(y):
        return lambda a, b: (a[0] + (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]), y)
    pts = [tuple(p[:2]) for p in ring]
    for inside, inter in [(lambda p: p[0] >= W, ix(W)), (lambda p: p[0] <= E, ix(E)),
                          (lambda p: p[1] >= S, iy(S)), (lambda p: p[1] <= N, iy(N))]:
        if not pts: break
        pts = edge(pts, inside, inter)
    return pts

def simplify(pts, tol):
    # Douglas–Peucker
    if len(pts) < 3: return pts
    keep = [False] * len(pts); keep[0] = keep[-1] = True
    stack = [(0, len(pts) - 1)]
    while stack:
        a, b = stack.pop()
        ax, ay = pts[a]; bx, by = pts[b]
        dx, dy = bx - ax, by - ay; L = (dx * dx + dy * dy) or 1e-12
        best, idx = 0, None
        for i in range(a + 1, b):
            px, py = pts[i]
            t = max(0, min(1, ((px - ax) * dx + (py - ay) * dy) / L))
            d = (px - ax - t * dx) ** 2 + (py - ay - t * dy) ** 2
            if d > best: best, idx = d, i
        if idx is not None and best > tol * tol:
            keep[idx] = True; stack += [(a, idx), (idx, b)]
    return [p for p, k in zip(pts, keep) if k]

def rnd(pts):
    out = []
    for x, y in pts:
        p = [round(x, 2), round(y, 2)]
        if not out or out[-1] != p: out.append(p)
    return out

def polygons(path):
    for f in json.load(open(path))["features"]:
        g = f["geometry"]
        if not g: continue
        polys = [g["coordinates"]] if g["type"] == "Polygon" else g["coordinates"]
        for poly in polys:
            yield f["properties"], poly

land = []
for _, poly in polygons(f"{SP}/ne_50m_land.geojson"):
    for ring in poly:
        r = rnd(simplify(clip(ring), 0.03))
        if len(r) >= 4: land.append(r)

lakes = []
for props, poly in polygons(f"{SP}/ne_50m_lakes.geojson"):
    if props.get("scalerank", 9) > 3: continue
    r = rnd(simplify(clip(poly[0]), 0.03))
    if len(r) >= 4: lakes.append(r)

rivers = []
for f in json.load(open(f"{SP}/ne_50m_rivers_lake_centerlines.geojson"))["features"]:
    p, g = f["properties"], f["geometry"]
    if not g or p.get("featurecla") != "River" or p.get("scalerank", 9) > 4: continue
    lines = [g["coordinates"]] if g["type"] == "LineString" else g["coordinates"]
    for line in lines:
        pts = [q for q in line if W <= q[0] <= E and S <= q[1] <= N]
        r = rnd(simplify([tuple(q[:2]) for q in pts], 0.03))
        if len(r) >= 2: rivers.append(r)

json.dump({"source": "Natural Earth 1:50m (public domain), clipped to %s..%s E, %s..%s N" % (W, E, S, N),
           "land": land, "lakes": lakes, "rivers": rivers},
          open(OUT, "w"), separators=(",", ":"))
print(len(land), len(lakes), len(rivers))
