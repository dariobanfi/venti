# Makes assets/austria.geojson, Austria's outline, from the Federal Office
# of Metrology and Surveying's administrative boundaries at 1:50 000 (BEV,
# "Verwaltungsgrenzen (VGD) - Stichtagsdaten 1:50 000", CC BY 4.0): see the
# manifest's [[asset]] for the release and where it is downloaded.
#
# Not run by the build or the tests. Needs Python 3 with pyshp, shapely and
# pyproj, and the release unpacked into dl/vgd50/ beside where it is run.
# Writes austria-border.geojson (as dissolved) and
# austria-border-simplified.geojson (simplified at 5 m), which is the one
# copied to assets/austria.geojson. check_outline.py checks both.

import shapefile, json, sys
from shapely.geometry import shape, Point, mapping, Polygon, MultiPolygon
from shapely.ops import unary_union, transform
from shapely import set_precision
import pyproj
r=shapefile.Reader('dl/vgd50/VGD_50_generalisiert')
geoms=[shape(s.__geo_interface__).buffer(0) for s in r.shapes()]
u=unary_union(geoms)
def desc(g):
    ps=list(g.geoms) if g.geom_type=='MultiPolygon' else [g]
    return len(ps), sum(len(p.interiors) for p in ps), [round(p.area/1e6,3) for p in ps if p.area<1e8]
print('raw union',u.geom_type,desc(u),'area km2',u.area/1e6)
# close hairline gaps (1 cm) and remove again
u2=u.buffer(0.01).buffer(-0.01)
print('after 1cm close',desc(u2),u2.area/1e6)
sumarea=sum(g.area for g in geoms)/1e6; print('sum of parts',sumarea)
u=u
t=pyproj.Transformer.from_crs(31287,4326,always_xy=True)
print(t.description if hasattr(t,'description') else '', t.definition)
print([ (o.name, o.accuracy) for o in t.operations] if t.operations else '')
ll=transform(t.transform,u)
def fc(g,dec=None):
    return {"type":"FeatureCollection","features":[{"type":"Feature","properties":{"name":"Austria","source":"BEV Verwaltungsgrenzen (VGD) 1:50 000 generalised, 2022-10-02, CC BY 4.0"},"geometry":mapping(g)}]}
json.dump(fc(ll),open('austria-border.geojson','w'),separators=(',',':'))
# simplify in metric proj (MGI Lambert metric)
s=u.simplify(5,preserve_topology=True)
sl=transform(t.transform,s)
def rnd(c):
    if isinstance(c[0],(int,float)): return [round(c[0],6),round(c[1],6)]
    return [rnd(x) for x in c]
m=mapping(sl); m={"type":m["type"],"coordinates":rnd(m["coordinates"])}
f=fc(sl); f["features"][0]["geometry"]=m
json.dump(f,open('austria-border-simplified.geojson','w'),separators=(',',':'))
def npts(g):
    ps=list(g.geoms) if g.geom_type=='MultiPolygon' else [g]
    return sum(len(p.exterior.coords)+sum(len(i.coords) for i in p.interiors) for p in ps)
print('points full',npts(ll),'simpl',npts(sl),desc(sl))
