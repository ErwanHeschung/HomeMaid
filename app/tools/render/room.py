"""Furnished rooms for the room viewport: dollhouse cut-aways (two walls, two open sides) rendered
over a 90° arc, with where devices can go.

Run from app/: ~/.local/opt/blender-5.2.2-linux-x64/blender -b -P tools/render/room.py -- [--quick] [--json] [<kind>_<size> ...]
Writes:
  assets/rooms/<kind>_<size>/view_<i>.png  the empty room turned by i * STEP degrees (i < VIEWS),
                                            i.e. seen at the app's yaw -i * STEP
  assets/rooms/rooms.json                   per room: framing and mounts; per model: its bounds
--json only rewrites rooms.json (no rendering).

Conventions are render.py's (ortho camera at PITCH, our (x, y, z) -> Blender (x, -z, y)). The back
wall stands at z = -D/2 and faces +z, the right wall at x = +W/2 faces -x; the open sides (+z, -x)
face the camera all along the arc. A facing of 0 looks towards +z, 90 towards +x.
Mounts, in our units:
  wall    {a, b, minY, facing}  a run along a wall's inner face; devices need y >= minY there
  surface {a, b, y, facing}     a furniture top
  window  {a, b, y, facing}     a window (y = sill), for shutters and contact sensors
  door    {a, b, facing}        a door leaf
  floor   {at}                  a free spot on the floor, best first
  pendant {at}                  under the ceiling (the app also hangs pendants over floor spots)
"""

import json
import math
import os
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(__file__))
import render as r  # noqa: E402

H = 2.2  # wall height
WALL = 0.12
VIEWS, STEP = 7, 15  # 0..90 degrees, on the device turntables' 15 degree frames
RES = (1200, 1000)
SIZES = {"s": (4.5, 3.6), "m": (6.0, 4.5), "l": (7.5, 5.4)}
OUT = os.path.join(os.path.dirname(__file__), "..", "..", "assets", "rooms")
ARGS = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
QUICK = "--quick" in ARGS
ONLY = [a for a in ARGS if not a.startswith("--")]


def room_palette():
    def mat(name, color, roughness=0.8, **kw):
        return r.material(name, color, roughness=roughness, **kw)

    return dict(
        wall=mat("wall", (0.16, 0.165, 0.175), roughness=0.9),
        cut=mat("cut", (0.42, 0.44, 0.47), roughness=0.6),
        frame=mat("frame", (0.06, 0.065, 0.07), roughness=0.4),
        pane=mat("pane", (0.55, 0.62, 0.72), roughness=0.05, transmission=0.6),
        fabric=mat("sofa", (0.09, 0.095, 0.105), roughness=0.95),
        cushion=mat("cushion", (0.12, 0.125, 0.135), roughness=0.95),
        linen=mat("linen", (0.24, 0.245, 0.25), roughness=0.95),
        wood=mat("wood", (0.13, 0.095, 0.07), roughness=0.55),
        door=mat("door", (0.1, 0.075, 0.055), roughness=0.5),
        cabinet=mat("cabinet", (0.07, 0.072, 0.078), roughness=0.5),
        stone=mat("stone", (0.3, 0.3, 0.31), roughness=0.35),
        porcelain=mat("porcelain", (0.62, 0.63, 0.64), roughness=0.2, coat=0.5),
        mirror=mat("mirror", (0.6, 0.62, 0.65), roughness=0.05, metallic=1.0),
        groove=mat("groove", (0.02, 0.02, 0.02)),
        metal_dark=mat("metal_dark", (0.05, 0.05, 0.055), roughness=0.3, metallic=1.0),
        steel=mat("steel", (0.5, 0.5, 0.52), roughness=0.25, metallic=1.0),
        rug=mat("rug", (0.075, 0.08, 0.09), roughness=1.0),
        pot=mat("pot", (0.2, 0.2, 0.21), roughness=0.6),
        leaf=mat("leaf", (0.05, 0.09, 0.06), roughness=0.6),
        books=mat("books", (0.17, 0.15, 0.13), roughness=0.8),
    )


def group(build, x=0.0, y=0.0, z=0.0, facing=0.0):
    """Runs [build] at the origin, then sets what it made in the room."""
    before = set(bpy.data.objects)
    build()
    pivot = bpy.data.objects.new("group", None)
    bpy.context.scene.collection.objects.link(pivot)
    for obj in set(bpy.data.objects) - before - {pivot}:
        if obj.parent is None:
            obj.parent = pivot
    pivot.rotation_euler = (0, 0, math.radians(facing))
    r.place(pivot, x, y, z)
    return pivot


def turn(dx, dz, facing):
    """A local offset (x along the furniture, z out of its front) turned by [facing]."""
    f = math.radians(facing)
    return dx * math.cos(f) + dz * math.sin(f), -dx * math.sin(f) + dz * math.cos(f)


# ---------------------------------------------------------------------------------------- furniture
# Built at the origin, front towards +z, standing on y = 0.


def sofa(m, length=2.4):
    r.block(m["fabric"], length, 0.42, 0.95, round_=0.06)
    r.block(m["fabric"], length, 0.42, 0.24, y=0.36, z=-0.36, round_=0.08)
    for dx in (-length / 2 + 0.11, length / 2 - 0.11):
        r.block(m["fabric"], 0.22, 0.6, 0.95, x=dx, round_=0.08)
    seat = (length - 0.44) / 2
    for dx in (-seat / 2 - 0.01, seat / 2 + 0.01):
        r.block(m["cushion"], seat - 0.02, 0.12, 0.7, x=dx, y=0.4, z=0.08, round_=0.05)


def low_cabinet(m, length, depth=0.5, height=0.48, mat="wood"):
    r.block(m[mat], length, height - 0.06, depth, y=0.06, round_=0.02)
    for dx in (-length / 2 + 0.1, length / 2 - 0.1):
        r.block(m["metal_dark"], 0.05, 0.06, depth - 0.1, x=dx, round_=0.01)
    r.block(m["groove"], length - 0.1, 0.01, 0.002, y=height * 0.55, z=depth / 2 + 0.001, round_=0)


def side_table(m):
    r.disc(m["wood"], 0.3, 0.04, y=0.48, round_=0.01)
    r.disc(m["metal_dark"], 0.025, 0.48, round_=0)
    r.disc(m["metal_dark"], 0.18, 0.02, round_=0.005)


def rug(m, w, d):
    r.block(m["rug"], w, 0.015, d, round_=0.006)


def table(m, w, d, h=0.74, mat="wood"):
    r.block(m[mat], w, 0.05, d, y=h - 0.05, round_=0.02)
    for dx in (-w / 2 + 0.08, w / 2 - 0.08):
        for dz in (-d / 2 + 0.08, d / 2 - 0.08):
            r.disc(m["metal_dark"], 0.022, h - 0.05, x=dx, z=dz, round_=0)


def chair(m):
    r.block(m["cushion"], 0.44, 0.05, 0.44, y=0.44, round_=0.02)
    r.block(m["cushion"], 0.44, 0.42, 0.05, y=0.48, z=-0.2, round_=0.02)
    for dx in (-0.19, 0.19):
        for dz in (-0.19, 0.19):
            r.disc(m["metal_dark"], 0.015, 0.44, x=dx, z=dz, round_=0)


def plant(m):
    r.cone(m["pot"], 0.18, 0.22, 0.36)
    for i in range(7):
        a = i * 2 * math.pi / 7
        leaf = r.ball(m["leaf"], 0.16, x=0.12 * math.cos(a), y=0.62 + 0.08 * (i % 3), z=0.12 * math.sin(a))
        r.squash(leaf, sx=0.7, sy=1.6, sz=0.35)
        leaf.rotation_euler = (0.4 * math.sin(a), 0.4 * math.cos(a), a)


def counter(m, length, sink_at=None, hob_at=None):
    r.block(m["cabinet"], length, 0.84, 0.62, y=0.06, z=-0.01, round_=0.01)
    r.block(m["groove"], length - 0.04, 0.06, 0.56, z=-0.04, round_=0)
    r.block(m["stone"], length + 0.02, 0.04, 0.66, y=0.9, round_=0.008)
    doors = max(1, round(length / 0.6))
    for i in range(1, doors):
        r.block(m["groove"], 0.006, 0.78, 0.002, x=-length / 2 + i * length / doors, y=0.09, z=0.302, round_=0)
    if sink_at is not None:
        r.block(m["steel"], 0.5, 0.012, 0.4, x=sink_at, y=0.935, round_=0.02)
        r.disc(m["steel"], 0.018, 0.3, x=sink_at, y=0.94, z=-0.26, round_=0)
        r.block(m["steel"], 0.03, 0.03, 0.18, x=sink_at, y=1.2, z=-0.18, round_=0.01)
    if hob_at is not None:
        r.block(m["screen"], 0.56, 0.006, 0.5, x=hob_at, y=0.94, round_=0.01)
        for dx, dz, rad in ((-0.13, -0.11, 0.09), (0.13, -0.11, 0.07), (-0.13, 0.12, 0.07), (0.13, 0.12, 0.09)):
            r.torus(m["groove"], rad, 0.004, x=hob_at + dx, y=0.947, z=dz)


def bed(m, w=1.6, length=2.1):
    r.block(m["wood"], w + 0.1, 0.3, length, y=0.06, round_=0.03)
    for dx in (-w / 2, w / 2):
        for dz in (-length / 2 + 0.1, length / 2 - 0.1):
            r.block(m["metal_dark"], 0.05, 0.06, 0.05, x=dx, z=dz, round_=0.01)
    r.block(m["linen"], w, 0.2, length - 0.1, y=0.36, z=0.04, round_=0.08)
    r.block(m["fabric"], w + 0.02, 0.18, length * 0.55, y=0.4, z=length * 0.2, round_=0.08)
    r.block(m["wood"], w + 0.14, 0.9, 0.08, y=0.06, z=-length / 2 - 0.02, round_=0.02)
    for dx in (-w / 4, w / 4):
        r.block(m["porcelain"], w / 2 - 0.12, 0.12, 0.36, x=dx, y=0.56, z=-length / 2 + 0.3, round_=0.06)


def nightstand(m):
    r.block(m["wood"], 0.48, 0.44, 0.42, y=0.06, round_=0.02)
    for dx in (-0.2, 0.2):
        r.block(m["metal_dark"], 0.03, 0.06, 0.3, x=dx, round_=0.01)
    r.block(m["groove"], 0.4, 0.008, 0.002, y=0.3, z=0.211, round_=0)


def wardrobe(m, length):
    r.block(m["cabinet"], length, 2.0, 0.6, round_=0.015)
    doors = max(2, round(length / 0.5))
    for i in range(1, doors):
        r.block(m["groove"], 0.006, 1.94, 0.002, x=-length / 2 + i * length / doors, y=0.03, z=0.301, round_=0)
    for i in range(doors):
        r.block(m["steel"], 0.02, 0.3, 0.03, x=-length / 2 + (i + 0.5) * length / doors + (0.1 if i % 2 else -0.1), y=0.9, z=0.31, round_=0.008)


def bookcase(m, length, height=1.9):
    r.block(m["wood"], length, height, 0.38, round_=0.012)
    shelves = 5
    gap = (height - 0.12) / shelves
    for i in range(shelves):
        y = 0.08 + i * gap
        r.block(m["groove"], length - 0.06, gap - 0.04, 0.3, y=y + 0.03, z=0.05, round_=0)
        x = -length / 2 + 0.1
        k = 0
        while x < length / 2 - 0.2:
            k += 1
            w = 0.04 + 0.03 * ((k * 7 + i * 3) % 5) / 4
            r.block(m["books"], w, gap - 0.1 - 0.04 * (k % 3), 0.24, x=x + w / 2, y=y + 0.04, z=0.04, round_=0.004)
            x += w + 0.008 + (0.22 if (k + i) % 9 == 0 else 0)


def desk(m, w=1.7, d=0.72):
    table(m, w, d, h=0.75)
    r.block(m["cabinet"], 0.45, 0.6, d - 0.08, x=w / 2 - 0.3, y=0.06, round_=0.01)


def office_chair(m):
    r.disc(m["metal_dark"], 0.3, 0.04, round_=0.01)
    r.disc(m["metal_dark"], 0.03, 0.42, y=0.04, round_=0)
    r.block(m["cushion"], 0.5, 0.08, 0.48, y=0.46, round_=0.04)
    r.block(m["cushion"], 0.48, 0.55, 0.07, y=0.58, z=-0.22, round_=0.04)


def bathtub(m, length=1.7, w=0.75):
    r.block(m["porcelain"], length, 0.56, w, round_=0.06)
    r.block(m["pane"], length - 0.16, 0.02, w - 0.16, y=0.5, round_=0.05)
    r.disc(m["steel"], 0.02, 0.22, x=-length / 2 + 0.14, y=0.56, z=-w / 2 + 0.1, round_=0)


def vanity(m, length=1.0):
    r.block(m["wood"], length, 0.5, 0.48, y=0.3, round_=0.015)
    r.block(m["stone"], length + 0.02, 0.04, 0.5, y=0.8, round_=0.008)
    r.block(m["porcelain"], 0.46, 0.1, 0.36, y=0.84, round_=0.05)
    r.disc(m["steel"], 0.016, 0.2, y=0.84, z=-0.2, round_=0)


def mirror(m, w, h):
    r.block(m["frame"], w + 0.04, h + 0.04, 0.025, round_=0.01)
    r.block(m["mirror"], w, h, 0.03, y=0.02, round_=0.004)


def toilet(m):
    r.block(m["porcelain"], 0.38, 0.4, 0.2, y=0.4, z=-0.22, round_=0.04)
    r.disc(m["porcelain"], 0.2, 0.4, z=0.08, round_=0.06)
    r.disc(m["porcelain"], 0.21, 0.03, y=0.4, z=0.08, round_=0.01)


def coat_rack(m):
    r.disc(m["metal_dark"], 0.2, 0.03, round_=0.01)
    r.disc(m["metal_dark"], 0.02, 1.7, y=0.03, round_=0)
    for i in range(4):
        a = i * math.pi / 2
        hook = r.block(m["metal_dark"], 0.18, 0.02, 0.02, x=0.08 * math.cos(a), y=1.6, z=0.08 * math.sin(a), round_=0.005)
        hook.rotation_euler = (0, 0, -a)
    coat = r.block(m["fabric"], 0.32, 0.8, 0.2, x=0.12, y=0.75, z=0.04, round_=0.08)
    coat.rotation_euler = (0, 0.05, 0.4)


def bench(m, length=1.0):
    r.block(m["wood"], length, 0.06, 0.38, y=0.42, round_=0.015)
    for dx in (-length / 2 + 0.08, length / 2 - 0.08):
        r.block(m["metal_dark"], 0.04, 0.42, 0.32, x=dx, round_=0.008)


# ---------------------------------------------------------------------------------------- room


class Room:
    def __init__(self, kind, size):
        self.kind, self.size = kind, size
        self.W, self.D = SIZES[size]
        self.m = {**r.palette(), **room_palette()}
        self.foot = []  # (x0, z0, x1, z1, top) of everything standing in the room
        self.openings = {"back": [], "right": []}  # (u0, u1) along each wall
        self.mounts = []
        self.spots = []  # floor spots chosen by the layout, before the generated ones

    # Walls: the back one runs along x at z = -D/2, the right one along z at x = W/2.
    def wall_point(self, wall, u, inset=0.0):
        if wall == "back":
            return u, -self.D / 2 + inset, 0.0
        return self.W / 2 - inset, u, -90.0

    def put(self, build, x, z, w, d, top, y=0.0, facing=0.0):
        """Sets furniture w (along its front) by d at (x, z); its footprint blocks mounts."""
        group(build, x, y, z, facing)
        if round(facing) % 180:
            w, d = d, w
        self.foot.append((x - w / 2, z - d / 2, x + w / 2, z + d / 2, top))

    def against(self, wall, u, build, w, d, top, y=0.0, inset=0.0):
        """Furniture with its back to [wall], centered at [u] along it."""
        x, z, facing = self.wall_point(wall, u, inset + d / 2)
        self.put(build, x, z, w, d, top, y=y, facing=facing)
        return x, z, facing

    def surface(self, x, y, z, length, facing, along=0.0):
        ox, oz = turn(along, 0, facing)
        dx, dz = turn(length / 2, 0, facing)
        x, z = x + ox, z + oz
        self.mounts.append({"mount": "surface", "a": [x - dx, z - dz], "b": [x + dx, z + dz], "y": y, "facing": facing})

    def pendant(self, x, z):
        self.mounts.append({"mount": "pendant", "at": [x, z]})

    def spot(self, x, z):
        self.spots.append((x, z))

    def _segment(self, wall, u0, u1, **extra):
        x0, z0, facing = self.wall_point(wall, u0)
        x1, z1, _ = self.wall_point(wall, u1)
        return {"a": [x0, z0], "b": [x1, z1], "facing": facing, **extra}

    def _hole(self, wall, u, w, y0, h):
        x, z, facing = self.wall_point(wall, u, -WALL / 2)
        cutter = r.block(self.m["wall"], w, h, WALL * 3, round_=0)
        cutter.hide_render = True
        cutter.rotation_euler = (0, 0, math.radians(facing))
        r.place(cutter, x, y0 + h / 2, z)
        mod = self.walls[wall].modifiers.new("hole", "BOOLEAN")
        mod.object, mod.operation = cutter, "DIFFERENCE"
        self.openings[wall].append((u - w / 2, u + w / 2))
        return self.wall_point(wall, u)

    def window(self, wall, u, w=1.3, h=1.1, sill=0.85):
        m = self.m
        x, z, facing = self._hole(wall, u, w, sill, h)

        def build():
            for dx in (-w / 2, 0, w / 2):
                r.block(m["frame"], 0.04, h, 0.06, x=dx, y=sill, z=-WALL / 2, round_=0.005)
            for dy in (0, h):
                r.block(m["frame"], w, 0.04, 0.06, y=sill + dy - 0.02, z=-WALL / 2, round_=0.005)
            r.block(m["pane"], w, h, 0.01, y=sill, z=-WALL / 2, round_=0)
            r.block(m["cut"], w + 0.04, 0.03, WALL + 0.06, y=sill - 0.03, z=-WALL / 2 + 0.03, round_=0.005)

        group(build, x, 0, z, facing)
        self.mounts.append({"mount": "window", **self._segment(wall, u - w / 2, u + w / 2), "y": sill})

    def door(self, wall, u, w=0.95, h=2.0):
        m = self.m
        x, z, facing = self._hole(wall, u, w, 0, h)

        def build():
            for dx in (-w / 2 - 0.02, w / 2 + 0.02):
                r.block(m["frame"], 0.05, h + 0.03, WALL + 0.04, x=dx, z=-WALL / 2, round_=0.005)
            r.block(m["frame"], w + 0.09, 0.05, WALL + 0.04, y=h, z=-WALL / 2, round_=0.005)
            r.block(m["door"], w, h - 0.01, 0.05, z=-0.03, round_=0.008)
            for dy in (0.45, 1.3):
                r.block(m["groove"], w - 0.2, 0.004, 0.002, y=dy, z=-0.004, round_=0)
            r.block(m["steel"], 0.14, 0.025, 0.04, x=w / 2 - 0.16, y=1.0, z=0.02, round_=0.008)
            r.block(m["rug"], w + 0.1, 0.012, 0.6, z=0.35, round_=0.004)

        group(build, x, 0, z, facing)
        self.mounts.append({"mount": "door", **self._segment(wall, u - w / 2 + 0.1, u + w / 2 - 0.1)})

    def shell(self):
        m, W, D = self.m, self.W, self.D
        floor = bpy.data.materials.new("floor")
        floor.use_nodes = True
        tree = floor.node_tree
        bsdf = tree.nodes["Principled BSDF"]
        bsdf.inputs["Roughness"].default_value = 0.7
        coords = tree.nodes.new("ShaderNodeTexCoord")
        # The construction grid is the real floor: hairlines every half unit.
        brick = tree.nodes.new("ShaderNodeTexBrick")
        brick.offset = 0.0
        brick.inputs["Scale"].default_value = 1.0
        brick.inputs["Mortar Size"].default_value = 0.004
        brick.inputs["Mortar Smooth"].default_value = 0.0
        brick.inputs["Brick Width"].default_value = 0.5
        brick.inputs["Row Height"].default_value = 0.5
        for socket in ("Color1", "Color2"):
            brick.inputs[socket].default_value = (0.03, 0.032, 0.036, 1)
        brick.inputs["Mortar"].default_value = (0.05, 0.054, 0.06, 1)
        tree.links.new(coords.outputs["Object"], brick.inputs["Vector"])
        tree.links.new(brick.outputs["Color"], bsdf.inputs["Base Color"])
        # Under the walls too: no seam of light where the floor would meet them.
        r.block(floor, W + WALL, 0.18, D + WALL, x=WALL / 2, y=-0.18, z=-WALL / 2, round_=0.01)

        self.walls = {}
        for name, w, d, x, z in (
            ("back", W + WALL, WALL, WALL / 2, -D / 2 - WALL / 2),
            ("right", WALL, D, W / 2 + WALL / 2, 0),
        ):
            self.walls[name] = r.block(m["wall"], w, H, d, x=x, z=z, round_=0.008)
            # The cut: a lighter section on top, as on an architect's model.
            r.block(m["cut"], w, 0.004, d, x=x, y=H, z=z, round_=0)

    # ------------------------------------------------------------------------------------ mounts

    def _top_in(self, x0, z0, x1, z1, floor=0.05):
        """Height of the tallest footprint over the rectangle (rugs and the like ignored)."""
        top = 0.0
        for fx0, fz0, fx1, fz1, t in self.foot:
            if t >= floor and fx0 < x1 and fx1 > x0 and fz0 < z1 and fz1 > z0:
                top = max(top, t)
        return top

    def wall_runs(self):
        """Free runs along both walls, split where furniture stands in front (low furniture
        leaves the wall above it free, from its top up)."""
        W, D = self.W, self.D
        reach = 0.9
        for wall, (u_start, u_end) in (("back", (-W / 2 + 0.25, W / 2 - 0.5)), ("right", (-D / 2 + 0.5, D / 2 - 0.25))):
            runs, current = [], None
            u = u_start
            while u <= u_end + 1e-6:
                if any(a - 0.1 < u < b + 0.1 for a, b in self.openings[wall]):
                    level = None
                else:
                    if wall == "back":
                        top = self._top_in(u - 0.3, -D / 2, u + 0.3, -D / 2 + reach)
                    else:
                        top = self._top_in(W / 2 - reach, u - 0.3, W / 2, u + 0.3)
                    level = 0.0 if top < 0.05 else (round(top + 0.05, 2) if top <= 1.2 else None)
                if current and current[2] == level:
                    current[1] = u
                else:
                    if current:
                        runs.append(current)
                    current = [u, u, level]
                u += 0.05
            runs.append(current)
            for u0, u1, level in runs:
                if level is not None and u1 - u0 >= 0.45:
                    self.mounts.append({"mount": "wall", **self._segment(wall, u0, u1), "minY": level})

    def floor_spots(self):
        """Free spots on the floor, out of the wall band, not hidden behind furniture from the
        open corner; the layout's own spots first, then the nearest to the center."""
        W, D = self.W, self.D
        found = []
        x = -W / 2 + 0.6
        while x <= W / 2 - 1.0:
            z = -D / 2 + 1.1
            while z <= D / 2 - 0.5:
                free = self._top_in(x - 0.45, z - 0.45, x + 0.45, z + 0.45) == 0
                # The camera looks from the open corner (-x, +z): nothing tall on that side.
                seen = self._top_in(x - 1.0, z, x, z + 1.0, floor=0.5) == 0
                near = any(math.dist((x, z), s) < 0.9 for s in self.spots)
                if free and seen and not near:
                    found.append((x, z))
                z += 0.9
            x += 0.9
        found.sort(key=lambda p: math.dist(p, (-0.2, 0.2)))
        for x, z in self.spots + found:
            self.mounts.append({"mount": "floor", "at": [x, z]})


# ---------------------------------------------------------------------------------------- layouts
# Furniture positions follow the walls, so each kind works at every size.


def living(R):
    m, W, D = R.m, R.W, R.D
    length = min(2.4, D - 1.5)
    zc = -0.1
    sofa_x = -W / 2 + (1.5 if R.size != "s" else 1.15)
    R.window("back", -W / 2 + W * 0.28)
    R.put(lambda: rug(m, W * 0.5, length + 0.4), x=sofa_x + W * 0.25 - 0.2, z=zc, w=W * 0.5, d=length + 0.4, top=0.015)
    stand = min(2.3, D - 1.3)
    x, z, f = R.against("right", zc, lambda: low_cabinet(m, stand), w=stand, d=0.5, top=0.48, inset=0.05)
    R.surface(x, 0.48, z, stand - 0.2, f)
    R.put(lambda: sofa(m, length), x=sofa_x, z=zc, w=length, d=0.95, top=0.6, facing=90)
    table_x = (sofa_x + 0.5 + W / 2 - 0.55) / 2
    R.put(lambda: table(m, 1.1, 0.55, h=0.36), x=table_x, z=zc, w=1.1, d=0.55, top=0.36, facing=90)
    R.surface(table_x, 0.36, zc, 0.6, 90)
    R.pendant(table_x, zc)
    side_z = zc - length / 2 - 0.4
    R.put(lambda: side_table(m), x=sofa_x, z=side_z, w=0.6, d=0.6, top=0.52)
    R.surface(sofa_x, 0.52, side_z, 0.3, 0)
    R.spot(sofa_x, zc + length / 2 + 0.45)
    R.put(lambda: plant(m), x=W / 2 - 0.4, z=-D / 2 + 0.4, w=0.5, d=0.5, top=0.9)
    if R.size == "l":
        R.against("back", W * 0.12, lambda: bookcase(m, 1.4), w=1.4, d=0.38, top=1.9)


def kitchen(R):
    m, W, D = R.m, R.W, R.D
    run = W - 2.4  # leaves a free stretch by the right wall: the fridge's place
    cx = -W / 2 + 0.5 + run / 2
    x, z, f = R.against("back", cx, lambda: counter(m, run, sink_at=-run * 0.2, hob_at=run * 0.25), w=run, d=0.66, top=0.92)
    R.surface(x, 0.92, z, run - 0.3, f)
    R.window("right", D * 0.1, w=1.2)
    tw, td = (1.4, 0.8) if R.size != "s" else (1.0, 0.7)
    tx, tz = -W * 0.1, D / 2 - 1.1
    R.put(lambda: table(m, tw, td), x=tx, z=tz, w=tw, d=td, top=0.74)
    R.surface(tx, 0.74, tz, tw - 0.5, 0)
    for dx in ((-tw / 4, tw / 4) if R.size != "s" else (0,)):
        R.put(lambda: chair(m), x=tx + dx, z=tz - td / 2 - 0.2, w=0.45, d=0.45, top=0.9, facing=0)
        R.put(lambda: chair(m), x=tx + dx, z=tz + td / 2 + 0.2, w=0.45, d=0.45, top=0.9, facing=180)
    R.pendant(tx, tz)


def bedroom(R):
    m, W, D = R.m, R.W, R.D
    bw, bl = (1.6, 2.1) if R.size != "s" else (1.4, 2.0)
    bx = -W / 2 + 1.0 + bw / 2 + (0.3 if R.size != "s" else 0)
    R.against("back", bx, lambda: bed(m, bw, bl), w=bw + 0.14, d=bl + 0.1, top=0.96)
    for side in (-1, 1):
        x, z, f = R.against("back", bx + side * (bw / 2 + 0.38), lambda: nightstand(m), w=0.48, d=0.42, top=0.5)
        R.surface(x, 0.5, z, 0.2, f)
    R.put(lambda: rug(m, bw + 0.8, 1.2), x=bx, z=-D / 2 + bl + 0.55, w=bw + 0.8, d=1.2, top=0.015)
    R.pendant(bx, -D / 2 + bl * 0.75)
    if R.size == "s":
        R.window("right", 0.0, w=1.2)
    else:
        R.against("right", -D / 2 + 0.95, lambda: wardrobe(m, 1.5), w=1.5, d=0.6, top=2.0)
        R.window("right", D / 2 - 1.2, w=1.2)
    if R.size == "l":
        R.put(lambda: bench(m, bw), x=bx, z=-D / 2 + bl + 0.35, w=bw, d=0.38, top=0.48)


def office(R):
    m, W, D = R.m, R.W, R.D
    dw = 1.7 if R.size != "s" else 1.4
    dx = -W / 2 + 0.8 + dw / 2
    x, z, f = R.against("back", dx, lambda: desk(m, dw), w=dw, d=0.72, top=0.75)
    R.surface(x, 0.75, z, dw - 0.6, f, along=-0.2)
    R.put(lambda: office_chair(m), x=dx - 0.15, z=-D / 2 + 1.15, w=0.6, d=0.6, top=1.1, facing=180)
    R.pendant(dx, -D / 2 + 1.0)
    if R.size == "s":
        x, z, f = R.against("right", -D / 2 + 0.9, lambda: low_cabinet(m, 1.0, depth=0.4, height=0.7), w=1.0, d=0.4, top=0.7)
        R.surface(x, 0.7, z, 0.7, f)
        R.window("right", D / 2 - 0.9, w=1.0)
    else:
        R.against("right", -D / 2 + 1.1, lambda: bookcase(m, 1.4), w=1.4, d=0.38, top=1.9)
        R.window("right", D / 2 - 1.3, w=1.2)
        R.put(lambda: plant(m), x=W / 2 - 0.4, z=D / 2 - 0.4, w=0.5, d=0.5, top=0.9)


def bathroom(R):
    m, W, D = R.m, R.W, R.D
    tub = 1.7 if R.size != "s" else 1.5
    tx = -W / 2 + 0.05 + tub / 2
    R.against("back", tx, lambda: bathtub(m, tub), w=tub, d=0.75, top=0.56)
    R.window("back", tx, w=0.8, h=0.6, sill=1.3)
    vz = 0.2 if R.size != "s" else 0.4
    x, z, f = R.against("right", vz, lambda: vanity(m), w=1.0, d=0.5, top=0.85)
    R.surface(x, 0.84, z, 0.15, f, along=0.36)
    R.against("right", vz, lambda: mirror(m, 0.8, 0.8), w=0.84, d=0.03, top=1.95, y=1.15)
    R.against("right", -D / 2 + 0.55, lambda: toilet(m), w=0.4, d=0.65, top=0.8)
    R.put(lambda: rug(m, 0.9, 0.55), x=tx, z=-D / 2 + 1.15, w=0.9, d=0.55, top=0.012)
    R.pendant(-0.2, 0.0)


def entrance(R):
    m, W, D = R.m, R.W, R.D
    door_x = -W * 0.12
    R.door("back", door_x)
    x, z, f = R.against("right", -0.1, lambda: low_cabinet(m, 1.2, depth=0.36, height=0.8), w=1.2, d=0.36, top=0.8)
    R.surface(x, 0.8, z, 0.9, f)
    R.against("right", -0.1, lambda: mirror(m, 0.7, 0.9), w=0.74, d=0.03, top=2.0, y=1.1)
    R.put(lambda: coat_rack(m), x=-W / 2 + 0.45, z=-D / 2 + 0.45, w=0.5, d=0.5, top=1.75)
    if R.size != "s":
        R.against("back", door_x + 1.35, lambda: bench(m, 1.0), w=1.0, d=0.38, top=0.48)
    R.pendant(door_x, -D / 2 + 1.3)


def generic(R):
    m, W, D = R.m, R.W, R.D
    x, z, f = R.against("back", -W * 0.1, lambda: low_cabinet(m, 1.6, depth=0.45, height=0.6), w=1.6, d=0.45, top=0.6)
    R.surface(x, 0.6, z, 1.3, f)
    R.window("right", 0.0, w=1.2)
    R.put(lambda: rug(m, W * 0.45, D * 0.4), x=-0.2, z=0.3, w=W * 0.45, d=D * 0.4, top=0.015)
    R.put(lambda: plant(m), x=W / 2 - 0.4, z=-D / 2 + 0.4, w=0.5, d=0.5, top=0.9)
    R.pendant(-0.2, 0.3)


LAYOUTS = {f.__name__: f for f in (living, kitchen, bedroom, office, bathroom, entrance, generic)}


# ---------------------------------------------------------------------------------------- output


def framing(W, D):
    """Ortho scale and the height of the point the camera aims at, so the room fits the image
    at every view of the arc (one scale for all views: the room never zooms while it turns)."""
    corners = [(x, y, z) for x in (-W / 2, W / 2 + WALL) for z in (-D / 2 - WALL, D / 2) for y in (-0.18, H)]
    xs, ys = [], []
    for i in range(VIEWS):
        yaw = -math.radians(i * STEP)
        for x, y, z in corners:
            xs.append(x * math.cos(yaw) - z * math.sin(yaw))
            z1 = -(x * math.sin(yaw) + z * math.cos(yaw))
            ys.append(y * math.cos(r.PITCH) + z1 * math.sin(r.PITCH))
    width = 2 * max(abs(v) for v in xs)
    height = max(ys) - min(ys)
    scale = max(width, height * RES[0] / RES[1]) * 1.03
    return round(scale, 4), round((max(ys) + min(ys)) / 2 / math.cos(r.PITCH), 4)


def rounded(value):
    if isinstance(value, float):
        return round(value, 3)
    if isinstance(value, (list, tuple)):
        return [rounded(v) for v in value]
    if isinstance(value, dict):
        return {k: rounded(v) for k, v in value.items()}
    return value


def render_room(kind, size, write_images=True):
    scene = r.reset()
    scene.render.resolution_x, scene.render.resolution_y = RES
    scene.cycles.samples = 16 if QUICK else 48
    for o in list(scene.objects):
        if o.type == "LIGHT":
            o.data.energy *= 4
            o.data.size *= 2.5
            o.location *= 1.6
        if o.type == "MESH" and o.is_shadow_catcher:
            bpy.data.objects.remove(o)

    R = Room(kind, size)
    room = bpy.data.objects.new("room", None)
    scene.collection.objects.link(room)
    before = set(bpy.data.objects)
    R.shell()
    LAYOUTS[kind](R)
    R.wall_runs()
    R.floor_spots()
    for obj in set(bpy.data.objects) - before - {room}:
        if obj.parent is None:
            obj.parent = room

    scale, focus = framing(R.W, R.D)
    scene.camera.data.ortho_scale = scale
    # The camera's line of sight goes through (0, focus, 0): that point is the image center.
    scene.camera.location.z += focus

    if write_images:
        out = os.path.join(OUT, f"{kind}_{size}")
        os.makedirs(out, exist_ok=True)
        for i in [0, 3, 6] if QUICK else range(VIEWS):
            room.rotation_euler = (0, 0, math.radians(i * STEP))
            scene.render.filepath = os.path.join(out, f"view_{i}.png")
            bpy.ops.render.render(write_still=True)

    return rounded({
        "w": R.W,
        "d": R.D,
        "h": H,
        "scale": scale,
        "focus": focus,
        "aspect": RES[0] / RES[1],
        "mounts": R.mounts,
    })


def model_bounds():
    """Every model's box in its own frame (x along its front, y up, z out of its front):
    [x0, x1, y0, y1, z0, z1]."""
    bounds = {}
    for name, build in r.MODELS.items():
        r.reset()
        build(r.palette())
        bpy.context.view_layer.update()
        pts = [o.matrix_world @ Vector(c) for o in bpy.data.objects if o.type in ("MESH", "CURVE") and not o.is_shadow_catcher for c in o.bound_box]
        flip = name.endswith("_hung")  # turned upside down about x
        ours = [(p.x, -p.z if flip else p.z, p.y if flip else -p.y) for p in pts]
        bounds[name] = rounded([f(c[i] for c in ours) for i in range(3) for f in (min, max)])
    return bounds


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    rooms = {}
    for kind in LAYOUTS:
        for size in SIZES:
            name = f"{kind}_{size}"
            print(f"== {name}", flush=True)
            rooms[name] = render_room(kind, size, write_images="--json" not in ARGS and (not ONLY or name in ONLY))
    data = {"views": VIEWS, "step": STEP, "pitch": r.PITCH, "rooms": rooms, "models": model_bounds()}
    with open(os.path.join(OUT, "rooms.json"), "w") as f:
        json.dump(data, f, separators=(",", ":"))
