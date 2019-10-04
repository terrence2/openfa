#version 450

#include <common/build-shaders/include/header.glsl>

layout(set = 0, binding = 0) buffer InverseViewProjection {
    mat4[] inv_view_proj;
};

layout(location = 0) in vec2 position;
layout(location = 0) out vec2 v_ray;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
    v_ray = vec2(compute_something(), 0.0);
}
