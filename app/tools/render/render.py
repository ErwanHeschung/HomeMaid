"""Studio renders of HomeMaid's device models, for the room viewport.

Run from app/: ~/.local/opt/blender-5.2.2-linux-x64/blender -b -P tools/render/render.py -- [--quick]

For each archetype (see lib/viewport/models.dart) it renders a turntable of FRAMES views:
  assets/models/<name>/base_XX.png   the model, lights off, with its contact shadow, transparent
  assets/models/<name>/glow<j>_XX.png  emission of light part j alone (white; tinted amber in the app)
Camera conventions match lib/viewport/geometry.dart: orthographic, pitch PITCH, yaw of frame k is
k * 2pi / FRAMES (applied as an object rotation of -yaw about Z), our (x, y, z) -> Blender (x, -z, y),
the object's floor origin at the image center, ORTHO_SCALE units across the image for every model.
"""

import math
import os
import sys

import bpy
from mathutils import Vector

FRAMES = 24
PITCH = 0.62
ORTHO_SCALE = 3.2
SIZE = 512
OUT = os.path.join(os.path.dirname(__file__), "..", "..", "assets", "models")
QUICK = "--quick" in sys.argv

# ---------------------------------------------------------------------------------------- scene


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 16 if QUICK else 64
    scene.cycles.use_denoising = True
    scene.render.film_transparent = True
    scene.render.resolution_x = scene.render.resolution_y = SIZE
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.view_settings.view_transform = "AgX"
    scene.view_settings.look = "AgX - Medium High Contrast"
    scene.view_settings.exposure = -0.6

    world = bpy.data.worlds.new("studio")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs[0].default_value = (0.19, 0.2, 0.22, 1)
    world.node_tree.nodes["Background"].inputs[1].default_value = 0.35
    scene.world = world

    cam = bpy.data.cameras.new("cam")
    cam.type = "ORTHO"
    cam.ortho_scale = ORTHO_SCALE
    cam_obj = bpy.data.objects.new("cam", cam)
    distance = 10
    cam_obj.location = (0, -distance * math.cos(PITCH), distance * math.sin(PITCH))
    cam_obj.rotation_euler = (math.pi / 2 - PITCH, 0, 0)
    scene.collection.objects.link(cam_obj)
    scene.camera = cam_obj

    # Soft studio light: a high key from the upper left casts the only (contact) shadow; a cool
    # rim behind and a front fill model the shape without shadows of their own.
    for name, loc, energy, size, color, shadow in [
        ("key", (-2.2, -2.0, 6.5), 420, 3.5, (1.0, 0.97, 0.93), True),
        ("rim", (3.0, 4.0, 3.0), 260, 3.0, (0.82, 0.88, 1.0), False),
        ("fill", (3.5, -4.5, 2.5), 90, 6.0, (1.0, 1.0, 1.0), False),
    ]:
        light = bpy.data.lights.new(name, "AREA")
        light.energy, light.size, light.color = energy, size, color
        light.use_shadow = shadow
        obj = bpy.data.objects.new(name, light)
        obj.location = loc
        obj.rotation_euler = (Vector((0, 0, 0.3)) - Vector(loc)).to_track_quat("-Z", "Y").to_euler()
        scene.collection.objects.link(obj)

    bpy.ops.mesh.primitive_plane_add(size=12, location=(0, 0, 0))
    floor = bpy.context.active_object
    floor.is_shadow_catcher = True
    return scene


def material(name, color, roughness=0.5, metallic=0.0, transmission=0.0, coat=0.0, emission=None):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*color, 1)
    bsdf.inputs["Roughness"].default_value = roughness
    bsdf.inputs["Metallic"].default_value = metallic
    bsdf.inputs["Transmission Weight"].default_value = transmission
    bsdf.inputs["Coat Weight"].default_value = coat
    if emission:
        bsdf.inputs["Emission Color"].default_value = (*emission, 1)
        bsdf.inputs["Emission Strength"].default_value = 6.0
    return mat


def palette():
    return {
        "clay": material("clay", (0.56, 0.57, 0.59), roughness=0.38, coat=0.35),
        "face": material("face", (0.42, 0.43, 0.46), roughness=0.45),
        "dark": material("dark", (0.02, 0.022, 0.025), roughness=0.35),
        "metal": material("metal", (0.72, 0.72, 0.74), roughness=0.28, metallic=1.0),
        "glass": material("glass", (0.95, 0.94, 0.9), roughness=0.45, transmission=0.9),
        "led": material("led", (0.08, 0.08, 0.09), roughness=0.2),
        "screen": material("screen", (0.01, 0.012, 0.014), roughness=0.08, coat=0.6),
        "fabric": material("fabric", (0.3, 0.31, 0.33), roughness=0.92),
        "shade": material("shade", (0.78, 0.76, 0.72), roughness=0.85, transmission=0.15),
        "lens": material("lens", (0.005, 0.006, 0.008), roughness=0.04, coat=1.0),
    }


# ---------------------------------------------------------------------------------------- shapes
# All in our units, converted to Blender axes by place(): our (x, y, z) -> (x, -z, y).


def place(obj, x=0.0, y=0.0, z=0.0):
    obj.location = (x, -z, y)
    return obj


def bevel(obj, width, segments=5):
    mod = obj.modifiers.new("bevel", "BEVEL")
    mod.width, mod.segments, mod.limit_method = width, segments, "ANGLE"
    obj.modifiers.new("weighted", "WEIGHTED_NORMAL")
    return obj


def block(mat, w, h, d, x=0.0, y=0.0, z=0.0, round_=0.04, spin=0.0):
    """A box w (x) by h (up) by d (depth), standing on y; [spin] turns it about the depth axis."""
    bpy.ops.mesh.primitive_cube_add(size=1)
    obj = bpy.context.active_object
    obj.scale = (w, d, h)
    bpy.ops.object.transform_apply(scale=True)
    obj.rotation_euler = (0, math.radians(spin), 0)
    place(obj, x, y + h / 2, z)
    obj.data.materials.append(mat)
    return bevel(obj, round_) if round_ else obj


def disc(mat, r, h, x=0.0, y=0.0, z=0.0, round_=0.02, vertices=64, facing="up"):
    bpy.ops.mesh.primitive_cylinder_add(radius=r, depth=h, vertices=vertices)
    obj = bpy.context.active_object
    if facing == "front":
        obj.rotation_euler = (math.pi / 2, 0, 0)
        place(obj, x, y, z + h / 2)
    else:
        place(obj, x, y + h / 2, z)
    obj.data.materials.append(mat)
    bpy.ops.object.shade_smooth()
    return bevel(obj, round_) if round_ else obj


def ball(mat, r, x=0.0, y=0.0, z=0.0):
    bpy.ops.mesh.primitive_uv_sphere_add(radius=r, segments=64, ring_count=32)
    obj = bpy.context.active_object
    place(obj, x, y, z)
    obj.data.materials.append(mat)
    bpy.ops.object.shade_smooth()
    return obj


def torus(mat, major, minor, x=0.0, y=0.0, z=0.0, facing="up"):
    bpy.ops.mesh.primitive_torus_add(major_radius=major, minor_radius=minor, major_segments=96, minor_segments=16)
    obj = bpy.context.active_object
    if facing == "front":
        obj.rotation_euler = (math.pi / 2, 0, 0)
    place(obj, x, y, z)
    obj.data.materials.append(mat)
    bpy.ops.object.shade_smooth()
    return obj


def cone(mat, r_bottom, r_top, h, x=0.0, y=0.0, z=0.0):
    bpy.ops.mesh.primitive_cone_add(radius1=r_bottom, radius2=r_top, depth=h, vertices=64)
    obj = place(bpy.context.active_object, x, y + h / 2, z)
    obj.data.materials.append(mat)
    bpy.ops.object.shade_smooth()
    return obj


def squash(obj, sx=1.0, sy=1.0, sz=1.0):
    """Scales in our axes (x, up, depth)."""
    obj.scale = (sx, sz, sy)
    return obj


def soft(obj):
    """Marks a large emitter (a screen): its glow pass falls off from the center, not a flat panel."""
    obj["soft"] = True
    return obj


def socket(m, x, y, z, facing="up"):
    """A European outlet: a recessed round cup with two pin holes."""
    parts = [disc(m["face"], 0.19, 0.012, x, y, z, round_=0.006, facing=facing)]
    for dx in (-0.06, 0.06):
        if facing == "up":
            parts.append(disc(m["dark"], 0.022, 0.016, x + dx, y + 0.002, z, round_=0, vertices=24))
        else:
            parts.append(disc(m["dark"], 0.022, 0.016, x + dx, y, z + 0.002, round_=0, vertices=24, facing="front"))
    return parts


# ---------------------------------------------------------------------------------------- models
# Each returns the list of light parts, in the order the app assigns entities to them.


def strip(m, n):
    width = 0.62 * n + 0.5
    block(m["clay"], width, 0.24, 0.72, round_=0.07)
    lights = []
    for i in range(n):
        cx = -width / 2 + 0.45 + i * 0.62
        socket(m, cx, 0.235, 0.0)
        lights.append(disc(m["led"], 0.024, 0.01, cx, 0.24, 0.28, round_=0, vertices=24))
    # USB ports on the end, and the cable leaving along the floor.
    for dz in (-0.1, 0.1):
        block(m["dark"], 0.012, 0.05, 0.12, width / 2 + 0.003, 0.09, dz, round_=0.01)
    bpy.ops.curve.primitive_bezier_curve_add()
    cable = bpy.context.active_object
    pts = cable.data.splines[0].bezier_points
    pts[0].co, pts[0].handle_left, pts[0].handle_right = (-width / 2, 0, 0.1), (-width / 2 + 0.2, 0, 0.1), (-width / 2 - 0.3, 0, 0.08)
    pts[1].co, pts[1].handle_left, pts[1].handle_right = (-width / 2 - 0.9, -0.5, 0.02), (-width / 2 - 0.6, -0.4, 0.02), (-width / 2 - 1.2, -0.6, 0.02)
    cable.data.bevel_depth = 0.028
    cable.data.bevel_resolution = 6
    cable.data.materials.append(m["clay"])
    return lights


def plug(m):
    block(m["clay"], 0.62, 0.72, 0.62, round_=0.12)
    socket(m, 0.0, 0.36, 0.312, facing="front")
    return [disc(m["led"], 0.03, 0.01, 0.0, 0.72, 0.18, round_=0, vertices=32)]


def lamp(m):
    disc(m["clay"], 0.3, 0.06, round_=0.025)
    disc(m["metal"], 0.025, 0.42, y=0.06, round_=0)
    disc(m["metal"], 0.075, 0.12, y=0.46, round_=0.01)
    bulb = ball(m["glass"], 0.27, y=0.86)
    # The neck between socket and globe.
    bpy.ops.mesh.primitive_cone_add(radius1=0.08, radius2=0.16, depth=0.14, vertices=64)
    neck = place(bpy.context.active_object, 0, 0.64, 0)
    neck.data.materials.append(m["glass"])
    bpy.ops.object.shade_smooth()
    return [bulb]


def screen(m):
    block(m["clay"], 0.5, 0.025, 0.3, z=-0.05, round_=0.012)
    block(m["metal"], 0.07, 0.48, 0.035, z=-0.12, y=0.02, round_=0.015)
    block(m["dark"], 1.9, 1.08, 0.05, y=0.42, z=-0.06, round_=0.025)
    panel = block(m["screen"], 1.84, 1.02, 0.004, y=0.45, z=-0.033, round_=0)
    return [soft(panel)]


def module(m):
    disc(m["clay"], 0.34, 0.22, round_=0.07)
    bpy.ops.mesh.primitive_torus_add(major_radius=0.2, minor_radius=0.014, major_segments=96)
    ring = place(bpy.context.active_object, 0, 0.222, 0)
    ring.data.materials.append(m["led"])
    bpy.ops.object.shade_smooth()
    return [ring]


# Everyday connected objects: same iconic scale, one light part each (amber when physically on).


def bulb(m):
    disc(m["metal"], 0.09, 0.16, round_=0.01)
    for y in (0.04, 0.08, 0.12):
        torus(m["metal"], 0.092, 0.008, y=y)
    cone(m["glass"], 0.09, 0.2, 0.14, y=0.16)
    return [ball(m["glass"], 0.3, y=0.55)]


def ceiling_light(m):
    disc(m["clay"], 0.56, 0.06, round_=0.025)
    dome = squash(ball(m["glass"], 0.5, y=0.06), sy=0.32)
    return [dome]


def light_strip(m):
    block(m["clay"], 1.9, 0.04, 0.14, round_=0.015)
    diffuser = block(m["glass"], 1.84, 0.025, 0.09, y=0.04, round_=0.008)
    block(m["clay"], 0.22, 0.08, 0.16, x=-1.08, round_=0.02)
    return [diffuser]


def floor_lamp(m):
    disc(m["clay"], 0.3, 0.05, round_=0.02)
    disc(m["metal"], 0.022, 1.18, y=0.05, round_=0)
    return [cone(m["shade"], 0.32, 0.2, 0.34, y=1.18)]


def thermostat(m):
    block(m["clay"], 0.36, 0.05, 0.24, round_=0.02)
    disc(m["metal"], 0.3, 0.1, y=0.36, z=-0.05, round_=0.03, facing="front")
    return [disc(m["lens"], 0.24, 0.006, y=0.36, z=0.05, round_=0, facing="front")]


def radiator(m):
    for i in range(7):
        block(m["clay"], 0.16, 0.66, 0.12, x=-0.6 + i * 0.2, y=0.08, round_=0.05)
    for y in (0.14, 0.68):
        block(m["clay"], 1.36, 0.05, 0.06, y=y, round_=0.02)
    for x in (-0.55, 0.55):
        block(m["metal"], 0.06, 0.1, 0.1, x=x, round_=0.02)
    disc(m["clay"], 0.07, 0.1, x=0.78, y=0.6, round_=0.02)
    return [disc(m["led"], 0.02, 0.01, x=0.78, y=0.7, round_=0, vertices=24)]


def fan(m):
    disc(m["clay"], 0.3, 0.05, round_=0.02)
    disc(m["metal"], 0.03, 0.7, y=0.05, round_=0)
    disc(m["clay"], 0.1, 0.16, y=0.95, z=-0.2, round_=0.03, facing="front")
    torus(m["metal"], 0.42, 0.014, y=0.95, z=0.0, facing="front")
    disc(m["clay"], 0.07, 0.06, y=0.95, z=-0.02, round_=0.02, facing="front")
    for angle in (0, 120, 240):
        a = math.radians(angle)
        block(m["glass"], 0.13, 0.3, 0.012, x=0.18 * math.sin(a), y=0.95 - 0.15 + 0.18 * math.cos(a), z=-0.01, round_=0.03, spin=angle)
    return [disc(m["led"], 0.02, 0.01, y=0.05, z=0.24, round_=0, vertices=24)]


def air_purifier(m):
    disc(m["clay"], 0.32, 0.96, round_=0.06)
    disc(m["fabric"], 0.325, 0.62, y=0.12, round_=0)
    disc(m["dark"], 0.24, 0.02, y=0.96, round_=0.005)
    return [torus(m["led"], 0.27, 0.012, y=0.975)]


def air_conditioner(m):
    block(m["clay"], 1.6, 0.48, 0.36, y=0.6, round_=0.12)
    block(m["dark"], 1.3, 0.03, 0.02, y=0.66, z=0.18, round_=0.01)
    block(m["metal"], 0.05, 0.6, 0.05, x=-0.6, round_=0.01)
    block(m["metal"], 0.05, 0.6, 0.05, x=0.6, round_=0.01)
    return [block(m["led"], 0.16, 0.04, 0.01, x=0.55, y=0.92, z=0.18, round_=0.01)]


def tv(m):
    for x in (-0.75, 0.75):
        block(m["metal"], 0.06, 0.14, 0.3, x=x, round_=0.02)
    block(m["dark"], 2.1, 1.2, 0.045, y=0.12, round_=0.02)
    return [soft(block(m["screen"], 2.04, 1.14, 0.004, y=0.15, z=0.024, round_=0))]


def speaker(m):
    disc(m["fabric"], 0.26, 0.58, round_=0.06)
    disc(m["clay"], 0.25, 0.04, y=0.58, round_=0.015)
    return [torus(m["led"], 0.17, 0.012, y=0.625)]


def camera(m):
    disc(m["clay"], 0.18, 0.05, round_=0.02)
    disc(m["metal"], 0.035, 0.14, y=0.05, round_=0)
    ball(m["clay"], 0.21, y=0.38)
    disc(m["lens"], 0.085, 0.03, y=0.38, z=0.18, round_=0.01, facing="front")
    return [disc(m["led"], 0.018, 0.01, y=0.5, z=0.17, round_=0, vertices=24, facing="front")]


def door_lock(m):
    block(m["clay"], 0.42, 0.78, 0.13, round_=0.07)
    for row in range(4):
        for col in range(3):
            disc(m["dark"], 0.03, 0.01, x=-0.1 + col * 0.1, y=0.22 + row * 0.1, z=0.065, round_=0, vertices=24, facing="front")
    block(m["metal"], 0.34, 0.05, 0.08, x=0.12, y=0.12, z=0.1, round_=0.02)
    return [block(m["led"], 0.2, 0.025, 0.01, y=0.68, z=0.066, round_=0.008)]


def contact_sensor(m):
    block(m["clay"], 0.16, 0.48, 0.1, x=-0.12, round_=0.04)
    block(m["clay"], 0.1, 0.48, 0.08, x=0.06, round_=0.03)
    return [disc(m["led"], 0.016, 0.01, x=-0.12, y=0.4, z=0.05, round_=0, vertices=24, facing="front")]


def motion_sensor(m):
    disc(m["clay"], 0.2, 0.06, round_=0.02)
    squash(ball(m["clay"], 0.2, y=0.2), sz=0.8)
    lens = squash(ball(m["glass"], 0.11, y=0.24, z=0.12), sz=0.6)
    return [lens]


def smoke_detector(m):
    disc(m["clay"], 0.42, 0.14, round_=0.05)
    torus(m["dark"], 0.3, 0.012, y=0.142)
    disc(m["clay"], 0.12, 0.03, y=0.14, round_=0.01)
    return [disc(m["led"], 0.02, 0.01, x=0.2, y=0.14, z=0.2, round_=0, vertices=24)]


def doorbell(m):
    block(m["clay"], 0.24, 0.64, 0.08, round_=0.06)
    disc(m["lens"], 0.05, 0.01, y=0.52, z=0.04, round_=0.004, facing="front")
    return [torus(m["led"], 0.07, 0.012, y=0.22, z=0.045, facing="front")]


def climate_sensor(m):
    block(m["clay"], 0.36, 0.04, 0.2, round_=0.015)
    block(m["clay"], 0.42, 0.42, 0.1, y=0.04, z=-0.03, round_=0.05)
    return [soft(block(m["screen"], 0.32, 0.26, 0.004, y=0.13, z=0.022, round_=0))]


def kettle(m):
    disc(m["clay"], 0.34, 0.06, round_=0.025)
    disc(m["metal"], 0.29, 0.5, y=0.07, round_=0.08)
    disc(m["clay"], 0.16, 0.04, y=0.57, round_=0.015)
    handle = torus(m["clay"], 0.2, 0.035, x=-0.3, y=0.33, facing="front")
    squash(handle, sx=0.8, sy=1.2)
    cone(m["metal"], 0.07, 0.03, 0.18, x=0.3, y=0.4)
    return [torus(m["led"], 0.3, 0.01, y=0.065)]


def coffee_maker(m):
    block(m["clay"], 0.62, 0.08, 0.5, round_=0.03)
    block(m["clay"], 0.62, 0.82, 0.22, y=0.08, z=-0.14, round_=0.05)
    block(m["clay"], 0.62, 0.16, 0.42, y=0.74, z=-0.04, round_=0.05)
    disc(m["metal"], 0.05, 0.08, y=0.66, z=0.06, round_=0.01)
    disc(m["clay"], 0.1, 0.16, y=0.08, z=0.06, round_=0.02)
    return [disc(m["led"], 0.025, 0.01, x=0.2, y=0.82, z=0.17, round_=0, vertices=24, facing="front")]


def washing_machine(m):
    block(m["clay"], 1.0, 1.16, 0.86, round_=0.06)
    torus(m["metal"], 0.3, 0.035, y=0.5, z=0.44, facing="front")
    disc(m["lens"], 0.28, 0.02, y=0.5, z=0.43, round_=0.01, facing="front")
    block(m["dark"], 0.9, 0.02, 0.01, y=0.96, z=0.432, round_=0)
    return [soft(block(m["screen"], 0.26, 0.09, 0.006, x=0.2, y=1.02, z=0.432, round_=0.01))]


def fridge(m):
    block(m["clay"], 0.9, 1.52, 0.78, round_=0.06)
    block(m["dark"], 0.88, 0.012, 0.01, y=0.98, z=0.392, round_=0)
    for y, h in ((0.55, 0.36), (1.06, 0.3)):
        block(m["metal"], 0.04, h, 0.05, x=0.36, y=y, z=0.42, round_=0.015)
    return [soft(block(m["screen"], 0.18, 0.12, 0.006, x=-0.2, y=1.2, z=0.392, round_=0.01))]


def robot_vacuum(m):
    disc(m["clay"], 0.44, 0.13, round_=0.05)
    torus(m["dark"], 0.44, 0.02, y=0.06)
    disc(m["clay"], 0.12, 0.03, y=0.13, z=0.15, round_=0.01)
    return [torus(m["led"], 0.09, 0.01, y=0.165, z=0.15)]


def shutter(m):
    for x in (-0.66, 0.66):
        block(m["clay"], 0.07, 1.3, 0.12, x=x, round_=0.02)
    block(m["clay"], 1.4, 0.22, 0.24, y=1.3, round_=0.05)
    for i in range(10):
        block(m["fabric"], 1.24, 0.07, 0.04, y=0.6 + i * 0.07, round_=0.012)
    return [disc(m["led"], 0.02, 0.01, x=0.55, y=1.52, z=0.06, round_=0, vertices=24)]


CATALOG = {
    f.__name__: f
    for f in (
        bulb, ceiling_light, light_strip, floor_lamp, thermostat, radiator, fan, air_purifier,
        air_conditioner, tv, speaker, camera, door_lock, contact_sensor, motion_sensor,
        smoke_detector, doorbell, climate_sensor, kettle, coffee_maker, washing_machine, fridge,
        robot_vacuum, shutter,
    )
}

MODELS = {
    **{f"strip_{n}": (lambda m, n=n: strip(m, n)) for n in range(2, 7)},
    "plug": plug,
    "lamp": lamp,
    "screen": screen,
    "module": module,
    **CATALOG,
    # Pendants for the furnished rooms: the same model upside down, hanging from its origin.
    "ceiling_light_hung": ceiling_light,
    "bulb_hung": bulb,
}

# ---------------------------------------------------------------------------------------- render


def render_model(name, build):
    scene = reset()
    m = palette()
    lights = build(m)
    pivot = bpy.data.objects.new("pivot", None)
    scene.collection.objects.link(pivot)
    hung = name.endswith("_hung")
    for obj in list(scene.objects):
        if obj.type == "MESH" and obj.is_shadow_catcher and hung:
            bpy.data.objects.remove(obj)  # nothing to cast a contact shadow on
        elif obj.type in ("MESH", "CURVE") and not obj.is_shadow_catcher:
            obj.parent = pivot
    flip = math.pi if hung else 0
    out = os.path.join(OUT, name)
    os.makedirs(out, exist_ok=True)

    holdout = bpy.data.materials.new("holdout")
    holdout.use_nodes = True
    nodes = holdout.node_tree.nodes
    nodes.remove(nodes["Principled BSDF"])
    hold = nodes.new("ShaderNodeHoldout")
    holdout.node_tree.links.new(hold.outputs[0], nodes["Material Output"].inputs[0])
    glow = material("glow", (0, 0, 0), emission=(1.0, 1.0, 1.0))
    # Soft glow: emission strongest at the center of the part, fading towards its edges.
    glow_soft = bpy.data.materials.new("glow_soft")
    glow_soft.use_nodes = True
    tree = glow_soft.node_tree
    tree.nodes.remove(tree.nodes["Principled BSDF"])
    coords = tree.nodes.new("ShaderNodeTexCoord")
    mapping = tree.nodes.new("ShaderNodeMapping")
    mapping.inputs["Location"].default_value = (-0.5, -0.5, -0.5)
    mapping.inputs["Scale"].default_value = (1.5, 1.5, 1.5)
    gradient = tree.nodes.new("ShaderNodeTexGradient")
    gradient.gradient_type = "SPHERICAL"
    ramp = tree.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.elements[0].color = (0.12, 0.12, 0.12, 1)
    ramp.color_ramp.elements[1].color = (0.85, 0.85, 0.85, 1)
    emit = tree.nodes.new("ShaderNodeEmission")
    emit.inputs["Strength"].default_value = 5.0
    links = tree.links
    links.new(coords.outputs["Generated"], mapping.inputs["Vector"])
    links.new(mapping.outputs["Vector"], gradient.inputs["Vector"])
    links.new(gradient.outputs["Color"], ramp.inputs["Fac"])
    links.new(ramp.outputs["Color"], emit.inputs["Color"])
    links.new(emit.outputs["Emission"], tree.nodes["Material Output"].inputs["Surface"])

    frames = range(0, FRAMES, 6) if QUICK else range(FRAMES)
    for k in frames:
        pivot.rotation_euler = (flip, 0, -2 * math.pi * k / FRAMES)
        scene.render.filepath = os.path.join(out, f"base_{k:02}.png")
        bpy.ops.render.render(write_still=True)

    # Glow passes: everything holds out (and still occludes), one light part emits.
    meshes = [o for o in scene.objects if o.type in ("MESH", "CURVE") and not o.is_shadow_catcher]
    originals = {o.name: [s.material for s in o.material_slots] for o in meshes}
    samples = scene.cycles.samples
    scene.cycles.samples = 8 if QUICK else 24
    scene.world.node_tree.nodes["Background"].inputs[1].default_value = 0
    for j, part in enumerate(lights):
        for o in meshes:
            for slot in o.material_slots:
                slot.material = (glow_soft if part.get("soft") else glow) if o == part else holdout
        for k in frames:
            pivot.rotation_euler = (flip, 0, -2 * math.pi * k / FRAMES)
            scene.render.filepath = os.path.join(out, f"glow{j}_{k:02}.png")
            bpy.ops.render.render(write_still=True)
    for o in meshes:
        for slot, mat in zip(o.material_slots, originals[o.name]):
            slot.material = mat
    scene.cycles.samples = samples


if __name__ == "__main__":
    only = [a for a in sys.argv[sys.argv.index("--") + 1 :] if not a.startswith("--")] if "--" in sys.argv else []
    for name, build in MODELS.items():
        if not only or name in only:
            print(f"== {name}", flush=True)
            render_model(name, build)
