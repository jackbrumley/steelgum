extends SceneTree

var surface: SubViewport
var camera: Camera3D
var geometry: Node3D
var environment: Environment
var sun: DirectionalLight3D
var last_objects: Array
var last_flat := false
var frame: Variant

func _initialize() -> void:
	RenderingServer.render_loop_enabled = false
	# Linux LibGodot needs a native graphics context. The SubViewport is
	# the only preview surface, so minimize the auxiliary context window.
	if DisplayServer.get_name() != "headless":
		DisplayServer.window_set_flag(DisplayServer.WINDOW_FLAG_NO_FOCUS, true)
		DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_MINIMIZED)
	surface = SubViewport.new()
	surface.own_world_3d = true
	surface.render_target_update_mode = SubViewport.UPDATE_DISABLED
	root.add_child(surface)
	var world := WorldEnvironment.new()
	environment = Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color.WHITE
	world.environment = environment
	surface.add_child(world)
	geometry = Node3D.new()
	surface.add_child(geometry)
	sun = DirectionalLight3D.new()
	surface.add_child(sun)
	camera = Camera3D.new()
	surface.add_child(camera)
	camera.make_current()

func apply_snapshot(text: String) -> bool:
	var scene: Variant = JSON.parse_string(text)
	if not scene is Dictionary:
		return false
	surface.size = Vector2i(scene.width, scene.height)
	if last_objects != scene.objects or last_flat != scene.flat:
		for old in geometry.get_children():
			old.free()
		for object in scene.objects:
			var mesh := BoxMesh.new()
			mesh.size = _vec(object.size)
			var material := StandardMaterial3D.new()
			material.albedo_color = _color(object.color)
			if scene.flat:
				material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
			mesh.material = material
			var node := MeshInstance3D.new()
			node.mesh = mesh
			node.position = _vec(object.center)
			geometry.add_child(node)
		last_objects = scene.objects
		last_flat = scene.flat
	var lighting: Dictionary = scene.lighting
	environment.background_color = _color(lighting.background)
	environment.ambient_light_energy = clampf(lighting.ambient_brightness / 340.0, 0.0, 8.0)
	sun.light_color = _color(lighting.get("sun_color", [255, 255, 255]))
	sun.light_energy = lighting.sun_illuminance / 7000.0
	sun.shadow_enabled = lighting.sun_shadows and not scene.flat
	var sun_direction := _vec(lighting.sun_from).normalized()
	var up := Vector3.FORWARD if absf(sun_direction.dot(Vector3.UP)) > 0.999 else Vector3.UP
	sun.look_at(-sun_direction, up)
	camera.near = maxf(scene.near, 0.001)
	camera.far = scene.far
	if scene.orthographic_height != null:
		camera.projection = Camera3D.PROJECTION_ORTHOGONAL
		camera.size = scene.orthographic_height
	else:
		camera.projection = Camera3D.PROJECTION_PERSPECTIVE
		camera.fov = rad_to_deg(scene.vertical_fov)
	camera.position = _vec(scene.position)
	var q: Array = scene.orientation
	camera.quaternion = Quaternion(q[0], q[1], q[2], q[3])
	return true

func draw_requested_frame() -> void:
	frame = null
	surface.render_target_update_mode = SubViewport.UPDATE_ONCE
	RenderingServer.frame_post_draw.connect(_read_frame, CONNECT_ONE_SHOT)
	RenderingServer.force_draw(false)

func _read_frame() -> void:
	var image := surface.get_texture().get_image()
	if image == null or image.is_empty():
		return
	image.convert(Image.FORMAT_RGBA8)
	frame = image.get_data()

func take_frame() -> Variant:
	var result: Variant = frame
	frame = null
	return result

func _vec(data: Array) -> Vector3:
	return Vector3(data[0], data[1], data[2])

func _color(data: Array) -> Color:
	return Color8(data[0], data[1], data[2])
