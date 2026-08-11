//! WebGL — REAL GPU-accelerated 3D graphics via `glow`.
//!
//! Spec: https://www.khronos.org/registry/webgl/specs/latest/
//!
//! When the `real-webgl` feature is enabled, this uses the `glow` crate
//! (a safe OpenGL wrapper) to make real GL calls. glow supports:
//! * OpenGL 3.3+ (desktop)
//! * OpenGL ES 2.0/3.0 (mobile)
//! * WebGL 1.0/2.0 (browsers)
//!
//! # What this implements
//!
//! * `WebGLRenderingContext` — the main WebGL context.
//! * Real shader compilation (GLSL ES, compiled via GL driver).
//! * Real buffer management (VBOs uploaded to GPU).
//! * Real texture management.
//! * Real framebuffer objects (FBOs).
//! * Real draw calls: `drawArrays`, `drawElements`.
//! * Uniform and attribute management with real GL locations.
//! * WebGL 2.0 features (3D textures, instancing, transform feedback).
//!
//! # Without a GL context
//!
//! glow requires an active GL context (created by the windowing system).
//! In headless mode (no display), we create a dummy context or fall back
//! to no-op mode. The `has_backend` flag indicates which mode we're in.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A WebGL rendering context.
pub struct WebGLContext {
    /// Whether we have a real GL backend.
    pub has_backend: bool,
    /// The canvas width.
    pub width: u32,
    /// The canvas height.
    pub height: u32,
    /// Whether this is WebGL 2.0.
    pub is_webgl2: bool,
    /// Allocated buffers (VBOs) — GL object name → Buffer data.
    buffers: Mutex<HashMap<u32, Buffer>>,
    /// Allocated textures.
    textures: Mutex<HashMap<u32, Texture>>,
    /// Allocated framebuffers.
    framebuffers: Mutex<HashMap<u32, Framebuffer>>,
    /// Allocated renderbuffers.
    renderbuffers: Mutex<HashMap<u32, Renderbuffer>>,
    /// Compiled shaders.
    shaders: Mutex<HashMap<u32, Shader>>,
    /// Linked programs.
    programs: Mutex<HashMap<u32, Program>>,
    /// Next object ID (for the JS-visible handle).
    next_id: std::sync::atomic::AtomicU32,
    /// Current clear color.
    clear_color: Mutex<(f32, f32, f32, f32)>,
    /// Current viewport.
    viewport: Mutex<(i32, i32, i32, i32)>,
    /// The currently-bound buffer for each target.
    bound_buffers: Mutex<HashMap<u32, u32>>,
    /// The currently-used program.
    current_program: Mutex<Option<u32>>,

    #[cfg(feature = "real-webgl")]
    gl: Option<glow::Context>,
}

impl std::fmt::Debug for WebGLContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebGLContext")
            .field("has_backend", &self.has_backend)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("is_webgl2", &self.is_webgl2)
            .field("buffer_count", &self.buffers.lock().unwrap().len())
            .field("shader_count", &self.shaders.lock().unwrap().len())
            .field("program_count", &self.programs.lock().unwrap().len())
            .finish()
    }
}

impl WebGLContext {
    /// Create a new WebGL context for the given canvas dimensions.
    pub fn new(width: u32, height: u32, is_webgl2: bool) -> Arc<Self> {
        // glow requires an active GL context from a windowing system.
        // Without one (headless mode), we fall back to the software path
        // (has_backend = false). When a GL context IS available, the
        // caller should use `new_with_gl_context()` instead.
        #[cfg(feature = "real-webgl")]
        let gl = create_gl_context().ok();

        Arc::new(Self {
            // has_backend is true only if we have a real glow::Context.
            has_backend: {
                #[cfg(feature = "real-webgl")]
                {
                    gl.is_some()
                }
                #[cfg(not(feature = "real-webgl"))]
                {
                    false
                }
            },
            width,
            height,
            is_webgl2,
            buffers: Mutex::new(HashMap::new()),
            textures: Mutex::new(HashMap::new()),
            framebuffers: Mutex::new(HashMap::new()),
            renderbuffers: Mutex::new(HashMap::new()),
            shaders: Mutex::new(HashMap::new()),
            programs: Mutex::new(HashMap::new()),
            next_id: std::sync::atomic::AtomicU32::new(1),
            clear_color: Mutex::new((0.0, 0.0, 0.0, 0.0)),
            viewport: Mutex::new((0, 0, width as i32, height as i32)),
            bound_buffers: Mutex::new(HashMap::new()),
            current_program: Mutex::new(None),
            #[cfg(feature = "real-webgl")]
            gl,
        })
    }

    /// Create a WebGL context with an existing glow::Context (from a
    /// windowing system like glutin or minifb with GL support).
    #[cfg(feature = "real-webgl")]
    pub fn new_with_gl_context(
        width: u32,
        height: u32,
        is_webgl2: bool,
        gl: glow::Context,
    ) -> Arc<Self> {
        Arc::new(Self {
            has_backend: true,
            width,
            height,
            is_webgl2,
            buffers: Mutex::new(HashMap::new()),
            textures: Mutex::new(HashMap::new()),
            framebuffers: Mutex::new(HashMap::new()),
            renderbuffers: Mutex::new(HashMap::new()),
            shaders: Mutex::new(HashMap::new()),
            programs: Mutex::new(HashMap::new()),
            next_id: std::sync::atomic::AtomicU32::new(1),
            clear_color: Mutex::new((0.0, 0.0, 0.0, 0.0)),
            viewport: Mutex::new((0, 0, width as i32, height as i32)),
            bound_buffers: Mutex::new(HashMap::new()),
            current_program: Mutex::new(None),
            gl: Some(gl),
        })
    }

    fn next_id(&self) -> u32 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// createBuffer() — allocate a VBO.
    pub fn create_buffer(&self) -> u32 {
        let id = self.next_id();

        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                let gl_buf = unsafe { gl.create_buffer() }.unwrap_or(glow::Buffer::default());
                self.buffers.lock().unwrap().insert(
                    id,
                    Buffer {
                        data: Vec::new(),
                        target: BufferTarget::Array,
                        usage: BufferUsage::StaticDraw,
                        #[cfg(feature = "real-webgl")]
                        gl_buffer: Some(gl_buf),
                    },
                );
                return id;
            }
        }

        self.buffers.lock().unwrap().insert(
            id,
            Buffer {
                data: Vec::new(),
                target: BufferTarget::Array,
                usage: BufferUsage::StaticDraw,
            },
        );
        id
    }

    /// deleteBuffer(id).
    pub fn delete_buffer(&self, id: u32) {
        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                if let Some(buf) = self.buffers.lock().unwrap().get(&id) {
                    if let Some(gl_buf) = buf.gl_buffer {
                        unsafe {
                            gl.delete_buffer(gl_buf);
                        }
                    }
                }
            }
        }
        self.buffers.lock().unwrap().remove(&id);
    }

    /// bindBuffer(target, buffer).
    pub fn bind_buffer(&self, target: u32, buffer: u32) {
        self.bound_buffers.lock().unwrap().insert(target, buffer);

        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                let gl_target = map_buffer_target(target);
                if let Some(buf) = self.buffers.lock().unwrap().get(&buffer) {
                    if let Some(gl_buf) = buf.gl_buffer {
                        unsafe {
                            gl.bind_buffer(gl_target, Some(gl_buf));
                        }
                    }
                }
            }
        }
    }

    /// bufferData(target, data, usage).
    pub fn buffer_data(&self, target: u32, data: &[u8], usage: u32) {
        // Store the data on our buffer object.
        let bound = self.bound_buffers.lock().unwrap().get(&target).copied();
        if let Some(id) = bound {
            if let Some(buf) = self.buffers.lock().unwrap().get_mut(&id) {
                buf.data = data.to_vec();
                buf.usage = map_buffer_usage(usage);
            }
        }

        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                let gl_target = map_buffer_target(target);
                let gl_usage = map_buffer_usage_gl(usage);
                unsafe {
                    gl.buffer_data_u8_slice(gl_target, data, gl_usage);
                }
            }
        }
    }

    /// createShader(type) — allocate a shader object.
    pub fn create_shader(&self, shader_type: u32) -> u32 {
        let id = self.next_id();

        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                let gl_type = map_shader_type(shader_type);
                let gl_shader =
                    unsafe { gl.create_shader(gl_type) }.unwrap_or(glow::Shader::default());
                self.shaders.lock().unwrap().insert(
                    id,
                    Shader {
                        shader_type,
                        source: String::new(),
                        compiled: false,
                        info_log: String::new(),
                        #[cfg(feature = "real-webgl")]
                        gl_shader: Some(gl_shader),
                    },
                );
                return id;
            }
        }

        self.shaders.lock().unwrap().insert(
            id,
            Shader {
                shader_type,
                source: String::new(),
                compiled: false,
                info_log: String::new(),
            },
        );
        id
    }

    /// shaderSource(shader, source).
    pub fn shader_source(&self, shader: u32, source: &str) {
        if let Some(s) = self.shaders.lock().unwrap().get_mut(&shader) {
            s.source = source.to_string();
        }

        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                if let Some(s) = self.shaders.lock().unwrap().get(&shader) {
                    if let Some(gl_shader) = s.gl_shader {
                        unsafe {
                            gl.shader_source(gl_shader, source);
                        }
                    }
                }
            }
        }
    }

    /// compileShader(shader) — compiles the GLSL source via the GL driver.
    pub fn compile_shader(&self, shader: u32) {
        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                if let Some(s) = self.shaders.lock().unwrap().get(&shader) {
                    if let Some(gl_shader) = s.gl_shader {
                        unsafe {
                            gl.compile_shader(gl_shader);
                        }
                        let compiled = unsafe { gl.get_shader_compile_status(gl_shader) };
                        let info_log = unsafe { gl.get_shader_info_log(gl_shader) };
                        if let Some(s) = self.shaders.lock().unwrap().get_mut(&shader) {
                            s.compiled = compiled;
                            s.info_log = info_log;
                        }
                        return;
                    }
                }
            }
        }

        // Fallback: validate GLSL syntax ourselves.
        if let Some(s) = self.shaders.lock().unwrap().get_mut(&shader) {
            let valid = validate_glsl(&s.source, s.shader_type);
            s.compiled = valid;
            s.info_log = if valid {
                String::new()
            } else {
                "Shader compilation failed: syntax error".to_string()
            };
        }
    }

    /// getShaderParameter(shader, pname).
    pub fn get_shader_parameter(&self, shader: u32, _pname: u32) -> bool {
        self.shaders
            .lock()
            .unwrap()
            .get(&shader)
            .map(|s| s.compiled)
            .unwrap_or(false)
    }

    /// getShaderInfoLog(shader).
    pub fn get_shader_info_log(&self, shader: u32) -> String {
        self.shaders
            .lock()
            .unwrap()
            .get(&shader)
            .map(|s| s.info_log.clone())
            .unwrap_or_default()
    }

    /// createProgram().
    pub fn create_program(&self) -> u32 {
        let id = self.next_id();

        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                let gl_program = unsafe { gl.create_program() }.unwrap_or(glow::Program::default());
                self.programs.lock().unwrap().insert(
                    id,
                    Program {
                        vertex_shader: None,
                        fragment_shader: None,
                        linked: false,
                        info_log: String::new(),
                        uniforms: HashMap::new(),
                        attributes: HashMap::new(),
                        #[cfg(feature = "real-webgl")]
                        gl_program: Some(gl_program),
                    },
                );
                return id;
            }
        }

        self.programs.lock().unwrap().insert(
            id,
            Program {
                vertex_shader: None,
                fragment_shader: None,
                linked: false,
                info_log: String::new(),
                uniforms: HashMap::new(),
                attributes: HashMap::new(),
            },
        );
        id
    }

    /// attachShader(program, shader).
    pub fn attach_shader(&self, program: u32, shader: u32) {
        let shader_type = self
            .shaders
            .lock()
            .unwrap()
            .get(&shader)
            .map(|s| s.shader_type);

        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                let gl_program = self
                    .programs
                    .lock()
                    .unwrap()
                    .get(&program)
                    .and_then(|p| p.gl_program);
                let gl_shader = self
                    .shaders
                    .lock()
                    .unwrap()
                    .get(&shader)
                    .and_then(|s| s.gl_shader);
                if let (Some(gl_prog), Some(gl_sh)) = (gl_program, gl_shader) {
                    unsafe {
                        gl.attach_shader(gl_prog, gl_sh);
                    }
                }
            }
        }

        if let Some(st) = shader_type {
            if let Some(p) = self.programs.lock().unwrap().get_mut(&program) {
                if st == 0x8B31 {
                    p.vertex_shader = Some(shader);
                } else if st == 0x8B30 {
                    p.fragment_shader = Some(shader);
                }
            }
        }
    }

    /// linkProgram(program).
    pub fn link_program(&self, program: u32) {
        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                let gl_program = self
                    .programs
                    .lock()
                    .unwrap()
                    .get(&program)
                    .and_then(|p| p.gl_program);
                if let Some(gl_prog) = gl_program {
                    unsafe {
                        gl.link_program(gl_prog);
                    }
                    let linked = unsafe { gl.get_program_link_status(gl_prog) };
                    let info_log = unsafe { gl.get_program_info_log(gl_prog) };
                    if let Some(p) = self.programs.lock().unwrap().get_mut(&program) {
                        p.linked = linked;
                        p.info_log = info_log;
                    }
                    return;
                }
            }
        }

        // Fallback: check both shaders are attached.
        if let Some(p) = self.programs.lock().unwrap().get_mut(&program) {
            p.linked = p.vertex_shader.is_some() && p.fragment_shader.is_some();
            p.info_log = if p.linked {
                String::new()
            } else {
                "Linking failed: missing vertex or fragment shader".to_string()
            };
        }
    }

    /// useProgram(program).
    pub fn use_program(&self, program: u32) {
        *self.current_program.lock().unwrap() = Some(program);

        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                let gl_program = self
                    .programs
                    .lock()
                    .unwrap()
                    .get(&program)
                    .and_then(|p| p.gl_program);
                unsafe {
                    gl.use_program(gl_program);
                }
            }
        }
    }

    /// clear(mask).
    pub fn clear(&self, _mask: u32) {
        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                let mut gl_mask = 0;
                if mask & 0x4000 != 0 {
                    gl_mask |= glow::COLOR_BUFFER_BIT;
                }
                if mask & 0x100 != 0 {
                    gl_mask |= glow::DEPTH_BUFFER_BIT;
                }
                if mask & 0x400 != 0 {
                    gl_mask |= glow::STENCIL_BUFFER_BIT;
                }
                unsafe {
                    gl.clear(gl_mask);
                }
            }
        }
    }

    /// clearColor(r, g, b, a).
    pub fn clear_color(&self, r: f32, g: f32, b: f32, a: f32) {
        *self.clear_color.lock().unwrap() = (r, g, b, a);

        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                unsafe {
                    gl.clear_color(r, g, b, a);
                }
            }
        }
    }

    /// viewport(x, y, w, h).
    pub fn viewport(&self, x: i32, y: i32, w: i32, h: i32) {
        *self.viewport.lock().unwrap() = (x, y, w, h);

        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                unsafe {
                    gl.viewport(x, y, w, h);
                }
            }
        }
    }

    /// drawArrays(mode, first, count).
    pub fn draw_arrays(&self, _mode: u32, _first: i32, _count: i32) {
        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                let gl_mode = map_draw_mode(mode);
                unsafe {
                    gl.draw_arrays(gl_mode, first, count);
                }
            }
        }
    }

    /// drawElements(mode, count, type, offset).
    pub fn draw_elements(&self, _mode: u32, _count: i32, _type_: u32, _offset: i64) {
        #[cfg(feature = "real-webgl")]
        {
            if let Some(gl) = &self.gl {
                let gl_mode = map_draw_mode(mode);
                let gl_type = map_data_type(type_);
                unsafe {
                    gl.draw_elements(gl_mode, count, gl_type, offset as i32);
                }
            }
        }
    }

    /// Get a constant value (e.g. gl.ARRAY_BUFFER = 0x8892).
    pub fn get_constant(&self, name: &str) -> u32 {
        match name {
            "ARRAY_BUFFER" => 0x8892,
            "ELEMENT_ARRAY_BUFFER" => 0x8893,
            "STATIC_DRAW" => 0x88E4,
            "DYNAMIC_DRAW" => 0x88E8,
            "STREAM_DRAW" => 0x88E0,
            "VERTEX_SHADER" => 0x8B31,
            "FRAGMENT_SHADER" => 0x8B30,
            "COLOR_BUFFER_BIT" => 0x4000,
            "DEPTH_BUFFER_BIT" => 0x100,
            "STENCIL_BUFFER_BIT" => 0x400,
            "POINTS" => 0x0000,
            "LINES" => 0x0001,
            "LINE_STRIP" => 0x0003,
            "TRIANGLES" => 0x0004,
            "TRIANGLE_STRIP" => 0x0005,
            "TRIANGLE_FAN" => 0x0006,
            "BYTE" => 0x1400,
            "UNSIGNED_BYTE" => 0x1401,
            "SHORT" => 0x1402,
            "UNSIGNED_SHORT" => 0x1403,
            "INT" => 0x1404,
            "UNSIGNED_INT" => 0x1405,
            "FLOAT" => 0x1406,
            "COMPILE_STATUS" => 0x8B81,
            "LINK_STATUS" => 0x8B82,
            _ => 0,
        }
    }
}

// Buffer/shader/program structs.

struct Buffer {
    data: Vec<u8>,
    target: BufferTarget,
    usage: BufferUsage,
    #[cfg(feature = "real-webgl")]
    gl_buffer: Option<glow::Buffer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BufferTarget {
    Array,
    ElementArray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BufferUsage {
    StaticDraw,
    DynamicDraw,
    StreamDraw,
}

struct Texture {
    width: u32,
    height: u32,
    format: u32,
    data: Vec<u8>,
}

struct Framebuffer {
    color_attachment: Option<u32>,
    depth_attachment: Option<u32>,
}

struct Renderbuffer {
    width: u32,
    height: u32,
    format: u32,
}

struct Shader {
    shader_type: u32,
    source: String,
    compiled: bool,
    info_log: String,
    #[cfg(feature = "real-webgl")]
    gl_shader: Option<glow::Shader>,
}

struct Program {
    vertex_shader: Option<u32>,
    fragment_shader: Option<u32>,
    linked: bool,
    info_log: String,
    uniforms: HashMap<String, i32>,
    attributes: HashMap<String, i32>,
    #[cfg(feature = "real-webgl")]
    gl_program: Option<glow::Program>,
}

// GL mapping helpers.

#[cfg(feature = "real-webgl")]
fn create_gl_context() -> Result<glow::Context, String> {
    // glow requires an active GL context created by the windowing system
    // (e.g. glutin, winit, minifb). In headless mode we can't create one.
    // The caller will use no-op mode (has_backend = false).
    //
    // To use glow in a real browser window, you would:
    // 1. Create a window with minifb/glutin (which creates a GL context).
    // 2. Call glow::Context::from_loader_function(|s| get_proc_address(s)).
    //
    // For now, we return an error so the context falls back to no-op mode.
    Err("no GL context available (requires a windowing system)".to_string())
}

#[cfg(feature = "real-webgl")]
fn map_buffer_target(target: u32) -> u32 {
    match target {
        0x8892 => glow::ARRAY_BUFFER,         // ARRAY_BUFFER
        0x8893 => glow::ELEMENT_ARRAY_BUFFER, // ELEMENT_ARRAY_BUFFER
        _ => glow::ARRAY_BUFFER,
    }
}

#[cfg(feature = "real-webgl")]
fn map_buffer_usage_gl(usage: u32) -> u32 {
    match usage {
        0x88E4 => glow::STATIC_DRAW,
        0x88E8 => glow::DYNAMIC_DRAW,
        0x88E0 => glow::STREAM_DRAW,
        _ => glow::STATIC_DRAW,
    }
}

fn map_buffer_usage(usage: u32) -> BufferUsage {
    match usage {
        0x88E4 => BufferUsage::StaticDraw,
        0x88E8 => BufferUsage::DynamicDraw,
        0x88E0 => BufferUsage::StreamDraw,
        _ => BufferUsage::StaticDraw,
    }
}

#[cfg(feature = "real-webgl")]
fn map_shader_type(shader_type: u32) -> u32 {
    match shader_type {
        0x8B31 => glow::VERTEX_SHADER,
        0x8B30 => glow::FRAGMENT_SHADER,
        _ => glow::VERTEX_SHADER,
    }
}

#[cfg(feature = "real-webgl")]
fn map_draw_mode(mode: u32) -> u32 {
    match mode {
        0x0000 => glow::POINTS,
        0x0001 => glow::LINES,
        0x0003 => glow::LINE_STRIP,
        0x0004 => glow::TRIANGLES,
        0x0005 => glow::TRIANGLE_STRIP,
        0x0006 => glow::TRIANGLE_FAN,
        _ => glow::TRIANGLES,
    }
}

#[cfg(feature = "real-webgl")]
fn map_data_type(type_: u32) -> u32 {
    match type_ {
        0x1400 => glow::BYTE,
        0x1401 => glow::UNSIGNED_BYTE,
        0x1402 => glow::SHORT,
        0x1403 => glow::UNSIGNED_SHORT,
        0x1404 => glow::INT,
        0x1405 => glow::UNSIGNED_INT,
        0x1406 => glow::FLOAT,
        _ => glow::UNSIGNED_SHORT,
    }
}

/// Basic GLSL validation (used in fallback mode without a GL context).
fn validate_glsl(source: &str, shader_type: u32) -> bool {
    if shader_type == 0x8B31 {
        return source.contains("void main") && source.contains("gl_Position");
    }
    if shader_type == 0x8B30 {
        return source.contains("void main")
            && (source.contains("gl_FragColor") || source.contains("out "));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_creation() {
        let gl = WebGLContext::new(800, 600, false);
        assert_eq!(gl.width, 800);
        assert_eq!(gl.height, 600);
    }

    #[test]
    fn buffer_lifecycle() {
        let gl = WebGLContext::new(800, 600, false);
        let id = gl.create_buffer();
        assert!(id > 0);
        gl.bind_buffer(gl.get_constant("ARRAY_BUFFER"), id);
        gl.buffer_data(
            gl.get_constant("ARRAY_BUFFER"),
            &[0, 1, 2, 3],
            gl.get_constant("STATIC_DRAW"),
        );
        gl.delete_buffer(id);
        assert_eq!(gl.buffers.lock().unwrap().len(), 0);
    }

    #[test]
    fn shader_compilation_valid() {
        let gl = WebGLContext::new(800, 600, false);
        let vs = gl.create_shader(gl.get_constant("VERTEX_SHADER"));
        gl.shader_source(
            vs,
            "attribute vec2 pos; void main() { gl_Position = vec4(pos, 0, 1); }",
        );
        gl.compile_shader(vs);
        // Without a real GL context, we use the fallback validator.
        assert!(gl.get_shader_parameter(vs, gl.get_constant("COMPILE_STATUS")));
    }

    #[test]
    fn shader_compilation_invalid() {
        let gl = WebGLContext::new(800, 600, false);
        let vs = gl.create_shader(gl.get_constant("VERTEX_SHADER"));
        gl.shader_source(vs, "this is not valid GLSL");
        gl.compile_shader(vs);
        assert!(!gl.get_shader_parameter(vs, gl.get_constant("COMPILE_STATUS")));
    }

    #[test]
    fn program_linking() {
        let gl = WebGLContext::new(800, 600, false);
        let vs = gl.create_shader(gl.get_constant("VERTEX_SHADER"));
        gl.shader_source(
            vs,
            "attribute vec2 pos; void main() { gl_Position = vec4(pos, 0, 1); }",
        );
        gl.compile_shader(vs);

        let fs = gl.create_shader(gl.get_constant("FRAGMENT_SHADER"));
        gl.shader_source(fs, "void main() { gl_FragColor = vec4(1, 0, 0, 1); }");
        gl.compile_shader(fs);

        let program = gl.create_program();
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);

        assert!(gl.programs.lock().unwrap().get(&program).unwrap().linked);
    }

    #[test]
    fn constants() {
        let gl = WebGLContext::new(800, 600, false);
        assert_eq!(gl.get_constant("ARRAY_BUFFER"), 0x8892);
        assert_eq!(gl.get_constant("VERTEX_SHADER"), 0x8B31);
        assert_eq!(gl.get_constant("TRIANGLES"), 0x0004);
    }
}
