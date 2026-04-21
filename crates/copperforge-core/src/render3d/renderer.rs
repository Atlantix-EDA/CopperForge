// Shader shape (position+color -> MVP transform, flat color out) and the
// single-program `UnlitProgram` organization follow alumina-interface's
// `src/renderer.rs` (Timothy Schmidt, MIT). See `render3d/mod.rs` for the
// full credit note.

use glow::{Context, HasContext as _};
use nalgebra::Matrix4;

const VS_UNLIT: &str = r#"#version 330
uniform mat4 u_mvp;
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_col;
out vec3 v_col;
void main() { v_col = a_col; gl_Position = u_mvp * vec4(a_pos, 1.0); }
"#;

const FS_UNLIT: &str = r#"#version 330
in vec3 v_col;
out vec4 o_col;
void main() { o_col = vec4(v_col, 1.0); }
"#;

pub struct UnlitProgram {
    prog: glow::Program,
    u_mvp: glow::UniformLocation,
}

impl UnlitProgram {
    pub unsafe fn new(gl: &Context) -> Self {
        unsafe {
            let prog = compile(gl, VS_UNLIT, FS_UNLIT);
            let u_mvp = gl
                .get_uniform_location(prog, "u_mvp")
                .expect("unlit shader missing u_mvp uniform");
            Self { prog, u_mvp }
        }
    }

    pub unsafe fn bind(&self, gl: &Context, mvp: &Matrix4<f32>) {
        unsafe {
            gl.use_program(Some(self.prog));
            gl.uniform_matrix_4_f32_slice(Some(&self.u_mvp), false, mvp.as_slice());
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
        let make = |kind: u32, src: &str| {
            let s = gl.create_shader(kind).expect("create_shader");
            gl.shader_source(s, src);
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
