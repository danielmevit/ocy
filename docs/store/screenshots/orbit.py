import json, math, sys
p=sys.argv[1]; d=json.load(open(p)); comps=d['heroRight']['compositions']
CH="repo/assets/illustrations/ocy-v1f/svg/blink-light-f1.svg"
K={"short":("caps/light/short-break.png",784,[34,34,716,192]),"warn":("caps/light/warning.png",784,[34,34,716,156]),
   "game":("caps/games/light/gentle-short.png",784,[34,34,716,256]),"walk":("caps/games/light/gentle-long.png",784,[34,34,716,256]),
   "long":("caps/light/long-break.png",1280,[160,156,960,728]),
   "roll":("caps/light/short-roll.png",784,[34,34,716,222])}
CX,CY=1400,510
def place(kind,ang,rx,ry,w,rot=0,op=1,z=2):
  src,sw,cr=K[kind]; h=cr[3]*w/cr[2]; a=math.radians(ang)
  cx=CX+rx*math.cos(a); cy=CY+ry*math.sin(a)
  return {"src":src,"srcW":sw,"crop":cr,"x":round(cx-w/2),"y":round(cy-h/2),"width":w,"radius":10,"card":True,"rotate":rot,"z":z,"opacity":op}
def ring(r,a): return {"x":CX-r,"y":CY-r,"w":2*r,"h":2*r,"css":f"border-radius:50%;border:2px solid rgba(127,196,166,{a});"}
char=lambda w:[{"src":CH,"x":CX-w/2,"y":CY-w/2-10,"width":w}]
comps['R4b']={"label":"Orbit, one ring","hires":True,"art":char(260),
  "decor":[ring(200,.40),ring(320,.26),ring(440,.14)],
  "shots":[place("short",-90,0,300,360,0),place("warn",-20,320,270,300,3),place("walk",50,320,270,330,2),
           place("long",130,320,270,250,-2),place("game",200,330,270,330,-3)]}
comps['R4c']={"label":"Orbit, two rings","hires":True,"art":char(240),
  "decor":[ring(190,.40),ring(300,.26),ring(430,.14)],
  "shots":[place("short",-90,0,215,380,0,1,3),place("warn",30,300,215,340,2,1,3),place("game",150,300,215,360,-2,1,3),
           place("walk",-30,400,330,270,3,.82,2),place("long",210,400,330,170,-3,.82,2),place("short",90,0,0,1,0,0,0)][:5]}
# V12i: R4c with a much bigger Ocy (520px); cards keep their size, placed by explicit centres.
def at(kind,cx,cy,w,rot=0,op=1,z=3):
  src,sw,cr=K[kind]; h=cr[3]*w/cr[2]
  return {"src":src,"srcW":sw,"crop":cr,"x":round(cx-w/2),"y":round(cy-h/2),"width":w,"radius":10,"card":True,"rotate":rot,"z":z,"opacity":op}
BX,BY=1360,500
def ringb(r,a): return {"x":BX-r,"y":BY-r,"w":2*r,"h":2*r,"css":f"border-radius:50%;border:2px solid rgba(127,196,166,{a});"}
CHD="repo/docs/store/screenshots/ocy-char-deep.svg"  # slide-only recolour (deep jade outline)
def blob(cx,cy,w,h,rot,shape): return {"x":cx-w/2,"y":cy-h/2,"w":w,"h":h,"z":0,"css":f"background:radial-gradient(circle at 40% 35%, #E4EFEA, #B9D6C9);opacity:.92;border-radius:{shape};transform:rotate({rot}deg);"}
comps['R4c']={"label":"Orbit, two rings","hires":True,"art":[{"src":CHD,"x":BX-260,"y":BY-270,"width":520}],
  "decor":[ringb(330,.36),ringb(430,.24),ringb(530,.13)],
  "shots":[at("short",BX,175,380),at("warn",1700,720,340,2),at("game",1060,720,280,-2),
           at("roll",BX,870,380,1),
           at("walk",1725,330,270,3,.82,2),at("long",995,320,200,-3,.82,2)]}
# V12s: every hero card uses the outer cards' soft 0.82 opacity (owner).
for sh in comps['R4c']['shots']: sh['opacity']=0.82
# The featured slide 01-hero = R4c on G.
G=d['heroChar']['backgrounds']['G']
d['slides'][0]={"id":"01-hero","layout":"hero","bg":G,"center":True,"brand":"repo/docs/store/screenshots/sprout-light-dark-stem.svg","brandBig":True,"copyClass":"char",
  "headline":"Gentle breaks for your [eyes]","sub":"A small reminder to look away, and a real break every hour.","facts":["Offline, no account","Never steals focus","40 languages"],"badge":"15-day free trial",
  **{k:v for k,v in comps['R4c'].items() if k in ("art","decor","shots")}}
json.dump(d,open(p,'w'),indent=2,ensure_ascii=False); open(p,'a').write('\n')
