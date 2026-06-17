// Shader shape (position+color -> MVP transform, flat color out) and the
// single-program `UnlitProgram` organization follow alumina-interface's
// `src/renderer.rs` (Timothy Schmidt, MIT). See `render3d/mod.rs` for the
// full credit note.

use glow::{Context, HasContext as _};
use nalgebra::Matrix4;

// Shader bodies are written *without* a `#version` directive — `compile()`
// prepends the right one per backend at runtime. Desktop GL gets
// `#version 330`; WebGL2 (wasm) gets `#version 300 es` + a float precision
// qualifier, which GLSL ES requires. Hardcoding `#version 330` here would
// fail to compile under WebGL2 and panic the browser app. The body syntax
// (`layout(location=…) in`, explicit `out vec4`) is valid in both 330 and
// ES 3.00, so only the header differs.
const VS_UNLIT: &str = r#"
uniform mat4 u_mvp;
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_col;
out vec3 v_col;
void main() { v_col = a_col; gl_Position = u_mvp * vec4(a_pos, 1.0); }
"#;

const FS_UNLIT: &str = r#"
uniform float u_alpha;
in vec3 v_col;
out vec4 o_col;
void main() { o_col = vec4(v_col, u_alpha); }
"#;

pub struct UnlitProgram {
    prog: glow::Program,
    u_mvp: glow::UniformLocation,
    u_alpha: glow::UniformLocation,
}

impl UnlitProgram {
    pub unsafe fn new(gl: &Context) -> Self {
        unsafe {
            let prog = compile(gl, VS_UNLIT, FS_UNLIT);
            let u_mvp = gl
                .get_uniform_location(prog, "u_mvp")
                .expect("unlit shader missing u_mvp uniform");
            let u_alpha = gl
                .get_uniform_location(prog, "u_alpha")
                .expect("unlit shader missing u_alpha uniform");
            Self { prog, u_mvp, u_alpha }
        }
    }

    pub unsafe fn bind(&self, gl: &Context, mvp: &Matrix4<f32>) {
        unsafe {
            gl.use_program(Some(self.prog));
            gl.uniform_matrix_4_f32_slice(Some(&self.u_mvp), false, mvp.as_slice());
            // Default to fully opaque each bind so the opaque-first / blended-
            // last callers don't need to reset alpha between frames.
            gl.uniform_1_f32(Some(&self.u_alpha), 1.0);
        }
    }

    /// Override the per-fragment alpha until the next `bind()` (or another
    /// `set_alpha()` call). Used by the mask layer to render as a tinted
    /// translucent sheet over copper without occluding it.
    pub unsafe fn set_alpha(&self, gl: &Context, alpha: f32) {
        unsafe {
            gl.uniform_1_f32(Some(&self.u_alpha), alpha);
        }
    }
}

// The egui_glow paint callback closure must be Send + Sync. glow's handles
// are single-threaded in practice (egui_glow runs everything on the UI
// thread), so asserting Send/Sync here is safe for this use.
unsafe impl Send for UnlitProgram {}
unsafe impl Sync for UnlitProgram {}

unsafe fn compile(gl: &Context, vs_src: &str, fs_src: &str) -> glow::Program {
    unsafe {
        // Pick the GLSL header for the active backend. WebGL2 reports as an
        // embedded (GLES) context and needs `#version 300 es` plus an
        // explicit default float precision; desktop GL uses `#version 330`.
        let header: &str = if gl.version().is_embedded {
            "#version 300 es\nprecision highp float;\n"
        } else {
            "#version 330\n"
        };
        let make = |kind: u32, src: &str| {
            let full = format!("{header}{src}");
            let s = gl.create_shader(kind).expect("create_shader");
            gl.shader_source(s, &full);
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                panic!("shader compile error: {}", gl.get_shader_info_log(s));
            }
            s
        };
        let vs = make(glow::VERTEX_SHADER, vs_src);
        let fs = make(glow::FRAGMENT_SHADER, fs_src);
        let prog = gl.create_program().expect("create_program");
        gl.attach_shader(prog, vs);
        gl.attach_shader(prog, fs);
        gl.link_program(prog);
        if !gl.get_program_link_status(prog) {
            panic!("shader link error: {}", gl.get_program_info_log(prog));
        }
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        prog
    }
}
