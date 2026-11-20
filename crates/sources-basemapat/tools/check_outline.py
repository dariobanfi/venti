# Checks the outlines build_outline.py writes: valid, their area and box,
# and that eleven places lie on their side of the border. Not run by the
# build or the tests; the crate's test austrias_outline_puts_the_places_on_
# their_side asks the same of the outline as the crate reads it.

import json,pyproj
from shapely.geometry import shape,Point
from shapely.validation import explain_validity
from shapely.ops import transform
for fn in ['austria-border.geojson','austria-border-simplified.geojson']:
    d=json.load(open(fn)); g=shape(d['features'][0]['geometry'])
    print(fn,len(d['features']),g.geom_type,len(g.geoms),'valid',g.is_valid,explain_validity(g))
    a=pyproj.Geod(ellps='WGS84').geometry_area_perimeter(g)[0]; print(' area km2',abs(a)/1e6, 'bbox',g.bounds)
    for n,p,exp in [('Innsbruck',(11.39,47.27),1),('Vienna',(16.37,48.21),1),('Greifenburg',(13.18,46.75),1),('Bregenz',(9.75,47.50),1),('Jungholz',(10.45,47.57),1),('Munich',(11.57,48.14),0),('Bolzano',(11.35,46.50),0),('Tegernsee',(11.76,47.71),0),('Vaduz',(9.52,47.14),0),('Samnaun',(10.36,46.94),0)]:
        r=g.contains(Point(p)); print('  ',n,r,'OK' if r==bool(exp) else 'FAIL')
