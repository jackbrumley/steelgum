extends SceneTree

var surface: SubViewport
var camera: Camera3D
var box: MeshInstance3D
var frame: Variant
var sequence := 0

func _initialize() -> void:
	RenderingServer.render_loop_enabled = false
	surface = SubViewport.new()
	surface.size = Vector2i(256, 160)
	surface.own_world_3d = true
	surface.render_target_update_mode = SubViewport.UPDATE_DISABLED
	root.add_child(surface)

	var sky := WorldEnvironment.new()
	sky.environment = Environment.new()
	sky.environment.background_mode = Environment.BG_COLOR
	sky.environment.background_color = Color("#172c44")
	surface.add_child(sky)

	box = MeshInstance3D.new()
	box.mesh = BoxMesh.new()
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.albedo_color = Color("#ee681c")
	box.material_override = material
	surface.add_child(box)

	camera = Camera3D.new()
	surface.add_child(camera)
	camera.make_current()

func fixture_ready() -> bool:
	return is_instance_valid(camera) and is_instance_valid(surface)

func request_frame() -> void:
	frame = null
	camera.position = Vector3(2.5, 1.8, 3.6) if sequence == 0 else Vector3(-3.1, 1.0, 2.4)
	camera.look_at(Vector3.ZERO)
	sequence += 1
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
