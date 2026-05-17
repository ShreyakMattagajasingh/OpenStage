// Phase 3c mesh shader.
// Lambert diffuse + ambient with base-color texture sampling and optional GPU skinning.

struct FrameUniforms {
    view_proj:   mat4x4<f32>,
    camera_pos:  vec4<f32>,
    light_dir:   vec4<f32>,   // direction TO the light, world space, normalized
    light_color: vec4<f32>,   // rgb premultiplied by intensity
    ambient:     vec4<f32>,
};
@group(0) @binding(0) var<uniform> frame: FrameUniforms;

struct InstanceUniforms {
    model:      mat4x4<f32>,
    base_color: vec4<f32>,
    skinning:   vec4<f32>, // x = enabled
};
@group(1) @binding(0) var<uniform> inst: InstanceUniforms;

@group(2) @binding(0) var base_color_tex:  texture_2d<f32>;
@group(2) @binding(1) var base_color_samp: sampler;

struct SkinUniforms {
    joints: array<mat4x4<f32>, 64>,
};
@group(3) @binding(0) var<uniform> skin: SkinUniforms;

struct VsIn {
    @location(0) pos:    vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv:     vec2<f32>,
    @location(3) joints: vec4<u32>,
    @location(4) weights: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip:         vec4<f32>,
    @location(0)       world_pos:    vec3<f32>,
    @location(1)       world_normal: vec3<f32>,
    @location(2)       uv:           vec2<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var local_pos = vec4<f32>(in.pos, 1.0);
    var local_normal = vec4<f32>(in.normal, 0.0);
    if (inst.skinning.x > 0.5) {
        let m0 = skin.joints[in.joints.x];
        let m1 = skin.joints[in.joints.y];
        let m2 = skin.joints[in.joints.z];
        let m3 = skin.joints[in.joints.w];
        local_pos =
            (m0 * vec4<f32>(in.pos, 1.0)) * in.weights.x +
            (m1 * vec4<f32>(in.pos, 1.0)) * in.weights.y +
            (m2 * vec4<f32>(in.pos, 1.0)) * in.weights.z +
            (m3 * vec4<f32>(in.pos, 1.0)) * in.weights.w;
        local_normal =
            (m0 * vec4<f32>(in.normal, 0.0)) * in.weights.x +
            (m1 * vec4<f32>(in.normal, 0.0)) * in.weights.y +
            (m2 * vec4<f32>(in.normal, 0.0)) * in.weights.z +
            (m3 * vec4<f32>(in.normal, 0.0)) * in.weights.w;
    }

    let world = inst.model * local_pos;
    var out: VsOut;
    out.clip = frame.view_proj * world;
    out.world_pos = world.xyz;
    // Assumes uniform scale. TODO: inverse-transpose for non-uniform.
    out.world_normal = (inst.model * local_normal).xyz;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let l = normalize(frame.light_dir.xyz);
    let lambert = max(dot(n, l), 0.0);
    let tex = textureSample(base_color_tex, base_color_samp, in.uv);
    let albedo = inst.base_color.rgb * tex.rgb;
    let lit = albedo * (frame.ambient.rgb + frame.light_color.rgb * lambert);
    return vec4<f32>(lit, inst.base_color.a * tex.a);
}
