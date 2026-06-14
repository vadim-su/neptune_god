shader_type spatial;

uniform sampler2D ground_texture : source_color, repeat_enable;
uniform sampler2D stone_texture : source_color, repeat_enable;
uniform sampler2D water_texture : source_color, repeat_enable;
uniform float terrain_scale = 0.18;
uniform float detail_scale = 0.47;
uniform float detail_strength = 0.28;

varying vec3 terrain_weight;

void vertex() {
	terrain_weight = COLOR.rgb;
}

vec2 rotated_uv(vec2 uv) {
	mat2 rotation = mat2(vec2(0.819152, 0.573576), vec2(-0.573576, 0.819152));
	return rotation * uv;
}

vec3 terrain_sample(sampler2D source, vec2 world_uv) {
	vec3 broad = texture(source, world_uv * terrain_scale).rgb;
	vec3 detail = texture(source, rotated_uv(world_uv) * detail_scale + vec2(19.37, 7.11)).rgb;
	return mix(broad, detail, detail_strength);
}

void fragment() {
	vec3 ground = terrain_sample(ground_texture, UV);
	vec3 stone = terrain_sample(stone_texture, UV);
	vec3 water = terrain_sample(water_texture, UV);
	vec3 weights = max(terrain_weight, vec3(0.0));
	float total = max(weights.r + weights.g + weights.b, 0.001);
	weights /= total;
	ALBEDO = ground * weights.r + stone * weights.g + water * weights.b;
	ROUGHNESS = 0.92;
}
