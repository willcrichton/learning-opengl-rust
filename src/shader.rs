use std::path::Path;

use glm::Mat3;

use crate::{io, prelude::*};

pub struct Shader {
  id: GlProgram,
}

impl Shader {
  pub async unsafe fn load(
    gl: &Context,
    vertex_path: impl AsRef<Path>,
    fragment_path: impl AsRef<Path>,
  ) -> Result<Self> {
    let (vertex_source, fragment_source) =
      try_join!(io::load_string(vertex_path), io::load_string(fragment_path))?;
    Self::new(gl, vertex_source, fragment_source)
  }

  pub unsafe fn new(
    gl: &Context,
    mut vertex_source: String,
    mut fragment_source: String,
  ) -> Result<Self> {
    // Add directives needed for each platform
    let header = if cfg!(target_arch = "wasm32") {
      "#version 300 es\nprecision highp float;"
    } else {
      "#version 330 core"
    };

    // Add struct definitions for all types in the crate
    let defs = [
      crate::camera::Camera::TYPE_DEF,
      crate::material::Material::TYPE_DEF,
      crate::light::PointLight::TYPE_DEF,
      crate::light::DirLight::TYPE_DEF,
      crate::light::SpotLight::TYPE_DEF,
    ]
    .join("\n");

    let preprocess = |source| format!("{}\n{}\n{}", header, defs, source);

    vertex_source = preprocess(vertex_source);
    fragment_source = preprocess(fragment_source);

    // Compile individual shaders into OpenGL objects
    let vertex_shader = Self::build_shader(&gl, glow::VERTEX_SHADER, &vertex_source)?;
    let fragment_shader = Self::build_shader(&gl, glow::FRAGMENT_SHADER, &fragment_source)?;

    // Link shaders into a single program
    let shader_program = gl.create_program().unwrap();
    gl.attach_shader(shader_program, vertex_shader);
    gl.attach_shader(shader_program, fragment_shader);

    gl.link_program(shader_program);
    if !gl.get_program_link_status(shader_program) {
      bail!(
        "Shader program failed to link with error: {}",
        gl.get_program_info_log(shader_program)
      );
    }

    // Cleanup shaders after linking
    gl.delete_shader(vertex_shader);
    gl.delete_shader(fragment_shader);

    Ok(Shader { id: shader_program })
  }

  unsafe fn build_shader(gl: &Context, shader_type: u32, source: &str) -> Result<GlShader> {
    // Create a new OpenGL shader object
    let shader = gl.create_shader(shader_type).unwrap();

    // Pass source to OpenGL
    gl.shader_source(shader, source);

    // Call the OpenGL shader compiler
    gl.compile_shader(shader);
    if !gl.get_shader_compile_status(shader) {
      bail!(
        "Shader failed to compile with error: {}",
        gl.get_shader_info_log(shader)
      );
    }

    Ok(shader)
  }

  unsafe fn location(&self, gl: &Context, name: &str) -> Option<GlUniformLocation> {
    gl.get_uniform_location(self.id, name)
  }

  // I wanted to call this "use" but that's a Rust keyword :'(
  pub unsafe fn activate(&self, gl: &Context) -> ActiveShader {
    gl.use_program(Some(self.id));
    ActiveShader::new(self)
  }
}

// Trait for custom shader structs that contains a GLSL type definition
pub trait ShaderTypeDef {
  const TYPE_DEF: &'static str;
}

pub struct ActiveShader<'a> {
  shader: &'a Shader,
  num_textures: u32,
}

// TODO: this API still doesn't feel quite right wrt handling texture slots
impl<'a> ActiveShader<'a> {
  pub fn new(shader: &'a Shader) -> Self {
    ActiveShader {
      shader,
      num_textures: 0,
    }
  }

  pub fn new_texture_slot(&mut self) -> u32 {
    let slot = self.num_textures;
    self.num_textures += 1;
    slot
  }

  pub unsafe fn bind_uniform<T: BindUniform>(&mut self, gl: &Context, name: &str, value: &T) {
    value.bind_uniform(gl, self, name);
  }

  pub unsafe fn location(&self, gl: &Context, name: &str) -> Option<GlUniformLocation> {
    self.shader.location(gl, name)
  }

  pub fn reset_textures(&mut self) {
    self.num_textures = 0;
  }
}

// A Rustic way to expose the uniform_* methods is to have a single trait which
// we implement for each type.
pub trait BindUniform {
  unsafe fn bind_uniform(&self, gl: &Context, shader: &mut ActiveShader, name: &str);
}

impl<T: BindUniform> BindUniform for Vec<T> {
  unsafe fn bind_uniform(&self, gl: &Context, shader: &mut ActiveShader, name: &str) {
    shader.bind_uniform(gl, &format!("{}_len", name), &(self.len() as i32));
    for (i, value) in self.iter().enumerate() {
      shader.bind_uniform(gl, &format!("{}[{}]", name, i), value);
    }
  }
}

impl<T: BindUniform> BindUniform for &T {
  unsafe fn bind_uniform(&self, gl: &Context, shader: &mut ActiveShader, name: &str) {
    (*self).bind_uniform(gl, shader, name);
  }
}

impl BindUniform for [f32; 4] {
  unsafe fn bind_uniform(&self, gl: &Context, shader: &mut ActiveShader, name: &str) {
    gl.uniform_4_f32(
      shader.location(gl, name).as_ref(),
      self[0],
      self[1],
      self[2],
      self[3],
    );
  }
}

impl BindUniform for i32 {
  unsafe fn bind_uniform(&self, gl: &Context, shader: &mut ActiveShader, name: &str) {
    gl.uniform_1_i32(shader.location(gl, name).as_ref(), *self);
  }
}

impl BindUniform for f32 {
  unsafe fn bind_uniform(&self, gl: &Context, shader: &mut ActiveShader, name: &str) {
    gl.uniform_1_f32(shader.location(gl, name).as_ref(), *self);
  }
}

impl BindUniform for u32 {
  unsafe fn bind_uniform(&self, gl: &Context, shader: &mut ActiveShader, name: &str) {
    gl.uniform_1_u32(shader.location(gl, name).as_ref(), *self);
  }
}

impl BindUniform for Vec3 {
  unsafe fn bind_uniform(&self, gl: &Context, shader: &mut ActiveShader, name: &str) {
    gl.uniform_3_f32(shader.location(gl, name).as_ref(), self.x, self.y, self.z);
  }
}

impl BindUniform for Mat3 {
  unsafe fn bind_uniform(&self, gl: &Context, shader: &mut ActiveShader, name: &str) {
    gl.uniform_matrix_3_f32_slice(shader.location(gl, name).as_ref(), false, self.as_slice());
  }
}

impl BindUniform for Mat4 {
  unsafe fn bind_uniform(&self, gl: &Context, shader: &mut ActiveShader, name: &str) {
    gl.uniform_matrix_4_f32_slice(shader.location(gl, name).as_ref(), false, self.as_slice());
  }
}
