shader_type spatial;
render_mode cull_disabled;

uniform sampler2D belt_texture : source_color, repeat_enable;
uniform float belt_speed_tiles_per_second = 1.0;
uniform vec4 tint : source_color = vec4(1.0);

void fragment() {
	vec2 belt_uv = vec2(fract(UV.x - TIME * belt_speed_tiles_per_second), fract(UV.y));
	vec4 surface = texture(belt_texture, belt_uv) * tint;
	ALBEDO = surface.rgb;
	ALPHA = surface.a;
	ROUGHNESS = 0.88;
	METALLIC = 0.0;
}
