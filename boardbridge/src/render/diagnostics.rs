// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! Diagnostic renderer.
//!
//! Three modes, all optional:
//!
//! * [`DiagnosticMode::None`] — draw nothing (a black clear). Used when the
//!   launcher wants the bridge to own the surface but not paint on it while the
//!   JVM starts.
//! * [`DiagnosticMode::Solid`] — clear to a fixed colour. Cheap, and the centre
//!   pixel can be read back and compared against the expected value, which is
//!   how CI proves that rendering really happened on a device where
//!   `screencap` is not always available.
//! * [`DiagnosticMode::Triangle`] — the GLES 3.00 shader program from the
//!   original demo, kept as a pipeline check.
//!
//! The two log lines this module produces are intentionally unchanged from the
//! C++ implementation, because the CI render test greps for them:
//!
//! ```text
//! First frame rendered (1080x2400, mode=SOLID)
//! frames=61 fps=59.8 mode=SOLID center_pixel_RGBA=(0,158,166,255)
//! ```
//!
//! Input is consumed from the same SDL-shaped queue the game will use, so the
//! touch/key path is exercised end to end even in diagnostic mode: a primary
//! DOWN toggles the mode, exactly as before, but now it travels through
//! `Android → translation → queue → renderer`.

use std::time::Instant;

use crate::android::surface::SurfaceSize;
use crate::bb_debug;
use crate::bb_info;
use crate::bb_warn;
use crate::graphics::ffi as gl;
use crate::input::event::{InputEvent, TouchPhase};
use crate::input::sdl_tables;

/// Fixed solid test colour: RGB ≈ (0, 158, 166), the value CI verifies.
const SOLID_R: gl::GLfloat = 0.00;
const SOLID_G: gl::GLfloat = 0.62;
const SOLID_B: gl::GLfloat = 0.65;

/// How often the frame statistics line is emitted.
const STATS_INTERVAL_SECS: f32 = 1.0;

/// Diagnostic render mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum DiagnosticMode {
    /// Clear to black, nothing else.
    None = 0,
    /// Clear to a fixed colour.
    #[default]
    Solid = 1,
    /// Spinning shaded triangle.
    Triangle = 2,
}

impl DiagnosticMode {
    /// Decodes the value sent from Kotlin.
    pub fn from_jni(value: i32) -> DiagnosticMode {
        match value {
            1 => DiagnosticMode::Solid,
            2 => DiagnosticMode::Triangle,
            _ => DiagnosticMode::None,
        }
    }

    /// Name used in the log lines (`SOLID`, `TRIANGLE`, `NONE`).
    pub fn name(self) -> &'static str {
        match self {
            DiagnosticMode::None => "NONE",
            DiagnosticMode::Solid => "SOLID",
            DiagnosticMode::Triangle => "TRIANGLE",
        }
    }

    /// Next mode in the toggle cycle.
    pub fn next(self) -> DiagnosticMode {
        match self {
            DiagnosticMode::None => DiagnosticMode::Solid,
            DiagnosticMode::Solid => DiagnosticMode::Triangle,
            DiagnosticMode::Triangle => DiagnosticMode::Solid,
        }
    }
}

/// Draws the bridge's own diagnostic content.
pub struct DiagnosticRenderer {
    mode: DiagnosticMode,
    program: u32,
    vao: u32,
    vbo: u32,
    angle_uniform: i32,
    program_ready: bool,
    program_attempted: bool,
    frame_count: u64,
    frames_since_stats: u64,
    started: Instant,
    last_stats: Instant,
    current_fps: f32,
    first_frame_logged: bool,
    center_pixel: [u8; 4],
    consumed_events: u64,
}

impl DiagnosticRenderer {
    /// Creates a renderer in the given mode.
    pub fn new(mode: DiagnosticMode) -> DiagnosticRenderer {
        let now = Instant::now();
        DiagnosticRenderer {
            mode,
            program: 0,
            vao: 0,
            vbo: 0,
            angle_uniform: -1,
            program_ready: false,
            program_attempted: false,
            frame_count: 0,
            frames_since_stats: 0,
            started: now,
            last_stats: now,
            current_fps: 0.0,
            first_frame_logged: false,
            center_pixel: [0, 0, 0, 0],
            consumed_events: 0,
        }
    }

    /// Current mode.
    pub fn mode(&self) -> DiagnosticMode {
        self.mode
    }

    /// Sets the mode.
    pub fn set_mode(&mut self, mode: DiagnosticMode) {
        if self.mode != mode {
            bb_info!("diagnostic mode: {} -> {}", self.mode.name(), mode.name());
            self.mode = mode;
        }
    }

    /// Advances to the next mode and returns it.
    pub fn toggle_mode(&mut self) -> DiagnosticMode {
        let next = self.mode.next();
        self.set_mode(next);
        next
    }

    /// Frames drawn since creation.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Frames per second over the last statistics window.
    pub fn fps(&self) -> f32 {
        self.current_fps
    }

    /// Last centre pixel readback (RGBA).
    pub fn center_pixel(&self) -> [u8; 4] {
        self.center_pixel
    }

    /// Input events consumed by the diagnostics.
    pub fn consumed_events(&self) -> u64 {
        self.consumed_events
    }

    /// Consumes queued events, driving the diagnostic mode from real input.
    ///
    /// Returns the first mode change caused by input, if any, so the caller can
    /// log it at the same place it logs other lifecycle facts.
    pub fn consume_input(&mut self, events: &[InputEvent]) -> Option<DiagnosticMode> {
        let mut changed = None;
        for event in events {
            self.consumed_events += 1;
            match event {
                InputEvent::Touch { phase, x, y, .. } if *phase == TouchPhase::Down => {
                    let mode = self.toggle_mode();
                    bb_info!("touch DOWN at ({x:.0}, {y:.0}) -> render mode = {}", mode.name());
                    changed = Some(mode);
                }
                InputEvent::Key {
                    pressed: true,
                    repeat: false,
                    scancode,
                    android_keycode,
                    ..
                } => {
                    let mode = self.toggle_mode();
                    bb_info!(
                        "key DOWN code={android_keycode} scancode={} -> render mode = {}",
                        sdl_tables::scancode_name(*scancode),
                        mode.name()
                    );
                    changed = Some(mode);
                }
                InputEvent::Text { text, .. } => {
                    bb_debug!("text input: {text:?} ({} chars)", text.chars().count());
                }
                other => {
                    bb_debug!("diagnostic input: {}", other.kind_name());
                }
            }
        }
        changed
    }

    /// Draws one frame. Requires a current context and a bound surface.
    pub fn draw(&mut self, size: SurfaceSize) {
        if size.is_valid() {
            unsafe { gl::glViewport(0, 0, size.width, size.height) };
        }
        match self.mode {
            DiagnosticMode::None => unsafe {
                gl::glClearColor(0.0, 0.0, 0.0, 1.0);
                gl::glClear(gl::GL_COLOR_BUFFER_BIT);
            },
            DiagnosticMode::Solid => unsafe {
                gl::glClearColor(SOLID_R, SOLID_G, SOLID_B, 1.0);
                gl::glClear(gl::GL_COLOR_BUFFER_BIT | gl::GL_DEPTH_BUFFER_BIT);
            },
            DiagnosticMode::Triangle => {
                self.ensure_program();
                unsafe {
                    gl::glClearColor(0.05, 0.06, 0.09, 1.0);
                    gl::glClear(gl::GL_COLOR_BUFFER_BIT | gl::GL_DEPTH_BUFFER_BIT);
                }
                if self.program_ready {
                    let angle = self.started.elapsed().as_secs_f32();
                    unsafe {
                        gl::glUseProgram(self.program);
                        gl::glUniform1f(self.angle_uniform, angle);
                        gl::glBindVertexArray(self.vao);
                        gl::glDrawArrays(gl::GL_TRIANGLES, 0, 3);
                        gl::glBindVertexArray(0);
                    }
                }
            }
        }
        self.frame_count += 1;
        self.frames_since_stats += 1;
    }

    /// Emits the "first frame" line once, and returns it when it was emitted.
    pub fn take_first_frame_log(&mut self, size: SurfaceSize) -> Option<String> {
        if self.first_frame_logged {
            return None;
        }
        self.first_frame_logged = true;
        Some(format!(
            "First frame rendered ({}x{}, mode={})",
            size.width,
            size.height,
            self.mode.name()
        ))
    }

    /// Produces the per-second statistics line when a window has elapsed.
    ///
    /// Reading the centre pixel is part of this: it is the cheapest possible
    /// proof that the driver actually wrote the pixels we asked for.
    pub fn take_stats_log(&mut self, size: SurfaceSize) -> Option<String> {
        let elapsed = self.last_stats.elapsed().as_secs_f32();
        if elapsed < STATS_INTERVAL_SECS {
            return None;
        }
        self.center_pixel = self.read_center_pixel(size);
        self.current_fps = self.frames_since_stats as f32 / elapsed;
        self.last_stats = Instant::now();
        self.frames_since_stats = 0;
        Some(format!(
            "frames={} fps={:.1} mode={} center_pixel_RGBA=({},{},{},{})",
            self.frame_count,
            self.current_fps,
            self.mode.name(),
            self.center_pixel[0],
            self.center_pixel[1],
            self.center_pixel[2],
            self.center_pixel[3]
        ))
    }

    /// Reads the centre pixel of the current draw surface.
    fn read_center_pixel(&mut self, size: SurfaceSize) -> [u8; 4] {
        let mut pixel = [0u8; 4];
        if !size.is_valid() {
            return pixel;
        }
        unsafe {
            gl::glReadPixels(
                size.width / 2,
                size.height / 2,
                1,
                1,
                gl::GL_RGBA,
                gl::GL_UNSIGNED_BYTE,
                pixel.as_mut_ptr() as *mut core::ffi::c_void,
            );
        }
        pixel
    }

    /// Releases GL objects; requires a current context.
    pub fn release_gl(&mut self) {
        if self.program_ready {
            unsafe {
                if self.vao != 0 {
                    gl::glDeleteVertexArrays(1, &self.vao);
                }
                if self.vbo != 0 {
                    gl::glDeleteBuffers(1, &self.vbo);
                }
                if self.program != 0 {
                    gl::glDeleteProgram(self.program);
                }
            }
            self.program = 0;
            self.vao = 0;
            self.vbo = 0;
            self.angle_uniform = -1;
            self.program_ready = false;
            // Allow `ensure_program` to build a fresh program against the next
            // context (the objects above belonged to the old one).
            self.program_attempted = false;
        }
    }

    /// Compiles the triangle program once.
    fn ensure_program(&mut self) {
        if self.program_attempted {
            return;
        }
        self.program_attempted = true;

        let vertex = compile_shader(gl::GL_VERTEX_SHADER, VERTEX_SOURCE);
        let fragment = compile_shader(gl::GL_FRAGMENT_SHADER, FRAGMENT_SOURCE);
        let (vertex, fragment) = match (vertex, fragment) {
            (Some(vertex), Some(fragment)) => (vertex, fragment),
            _ => {
                bb_warn!("triangle diagnostics unavailable: shader compilation failed");
                if let Some(shader) = vertex {
                    unsafe { gl::glDeleteShader(shader) };
                }
                if let Some(shader) = fragment {
                    unsafe { gl::glDeleteShader(shader) };
                }
                return;
            }
        };

        let program = unsafe { gl::glCreateProgram() };
        unsafe {
            gl::glAttachShader(program, vertex);
            gl::glAttachShader(program, fragment);
            gl::glLinkProgram(program);
            gl::glDeleteShader(vertex);
            gl::glDeleteShader(fragment);
        }
        let mut linked: gl::GLint = 0;
        unsafe { gl::glGetProgramiv(program, gl::GL_LINK_STATUS, &mut linked) };
        if linked != gl::GL_TRUE as gl::GLint {
            bb_warn!("triangle diagnostics unavailable: {}", program_log(program));
            unsafe { gl::glDeleteProgram(program) };
            return;
        }

        let mut vao: gl::GLuint = 0;
        let mut vbo: gl::GLuint = 0;
        unsafe {
            gl::glGenVertexArrays(1, &mut vao);
            gl::glGenBuffers(1, &mut vbo);
            gl::glBindVertexArray(vao);
            gl::glBindBuffer(gl::GL_ARRAY_BUFFER, vbo);
            gl::glBufferData(
                gl::GL_ARRAY_BUFFER,
                (VERTICES.len() * core::mem::size_of::<gl::GLfloat>()) as gl::GLsizeiptr,
                VERTICES.as_ptr() as *const core::ffi::c_void,
                gl::GL_STATIC_DRAW,
            );
            let stride = (5 * core::mem::size_of::<gl::GLfloat>()) as gl::GLsizei;
            gl::glVertexAttribPointer(0, 2, gl::GL_FLOAT, gl::GL_FALSE, stride, core::ptr::null());
            gl::glEnableVertexAttribArray(0);
            gl::glVertexAttribPointer(
                1,
                3,
                gl::GL_FLOAT,
                gl::GL_FALSE,
                stride,
                (2 * core::mem::size_of::<gl::GLfloat>()) as *const core::ffi::c_void,
            );
            gl::glEnableVertexAttribArray(1);
            gl::glBindVertexArray(0);
        }

        self.program = program;
        self.vao = vao;
        self.vbo = vbo;
        self.angle_uniform =
            unsafe { gl::glGetUniformLocation(program, UNIFORM_ANGLE.as_ptr() as *const gl::GLchar) };
        self.program_ready = true;
        bb_info!("triangle diagnostics: program linked");
    }
}

impl Drop for DiagnosticRenderer {
    fn drop(&mut self) {
        // GL objects are freed by the caller through `release_gl` while the
        // context is current; dropping here would need a current context, which
        // is not guaranteed at this point.
        if self.program_ready {
            bb_debug!(
                "diagnostic GL objects were not released before drop (context likely gone)"
            );
        }
    }
}

/// `"uAngle"` with a NUL terminator, as GL expects.
const UNIFORM_ANGLE: &[u8] = b"uAngle\0";

/// Vertex layout: `x, y, r, g, b` per vertex.
const VERTICES: [gl::GLfloat; 15] = [
    0.0, 0.6, 1.0, 0.25, 0.25, //
    -0.6, -0.5, 0.25, 1.0, 0.25, //
    0.6, -0.5, 0.25, 0.25, 1.0,
];

const VERTEX_SOURCE: &[u8] = b"#version 300 es\n\
layout(location = 0) in vec2 aPos;\n\
layout(location = 1) in vec3 aColor;\n\
uniform float uAngle;\n\
out vec3 vColor;\n\
void main() {\n\
  float c = cos(uAngle);\n\
  float s = sin(uAngle);\n\
  mat2 rot = mat2(c, -s, s, c);\n\
  gl_Position = vec4(rot * aPos, 0.0, 1.0);\n\
  vColor = aColor;\n\
}\n\0";

const FRAGMENT_SOURCE: &[u8] = b"#version 300 es\n\
precision mediump float;\n\
in vec3 vColor;\n\
out vec4 fragColor;\n\
void main() {\n\
  fragColor = vec4(vColor, 1.0);\n\
}\n\0";

fn compile_shader(kind: gl::GLenum, source: &[u8]) -> Option<gl::GLuint> {
    let shader = unsafe { gl::glCreateShader(kind) };
    if shader == 0 {
        return None;
    }
    let source_ptr = source.as_ptr() as *const gl::GLchar;
    unsafe { gl::glShaderSource(shader, 1, &source_ptr, core::ptr::null()) };
    unsafe { gl::glCompileShader(shader) };
    let mut compiled: gl::GLint = 0;
    unsafe { gl::glGetShaderiv(shader, gl::GL_COMPILE_STATUS, &mut compiled) };
    if compiled != gl::GL_TRUE as gl::GLint {
        bb_warn!("shader compile failed: {}", shader_log(shader));
        unsafe { gl::glDeleteShader(shader) };
        return None;
    }
    Some(shader)
}

fn shader_log(shader: gl::GLuint) -> String {
    let mut length: gl::GLint = 0;
    unsafe { gl::glGetShaderiv(shader, gl::GL_INFO_LOG_LENGTH, &mut length) };
    read_info_log(length, |buffer, size, written| unsafe {
        gl::glGetShaderInfoLog(shader, size, written, buffer)
    })
}

fn program_log(program: gl::GLuint) -> String {
    let mut length: gl::GLint = 0;
    unsafe { gl::glGetProgramiv(program, gl::GL_INFO_LOG_LENGTH, &mut length) };
    read_info_log(length, |buffer, size, written| unsafe {
        gl::glGetProgramInfoLog(program, size, written, buffer)
    })
}

fn read_info_log<F>(length: gl::GLint, fetch: F) -> String
where
    F: Fn(*mut gl::GLchar, gl::GLsizei, *mut gl::GLsizei),
{
    if length <= 1 {
        return "(no log)".to_string();
    }
    let mut buffer = vec![0u8; length as usize];
    let mut written: gl::GLsizei = 0;
    fetch(
        buffer.as_mut_ptr() as *mut gl::GLchar,
        buffer.len() as gl::GLsizei,
        &mut written,
    );
    if written > 0 && (written as usize) <= buffer.len() {
        buffer.truncate(written as usize);
    }
    while buffer.last() == Some(&0) {
        buffer.pop();
    }
    String::from_utf8_lossy(&buffer).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_cycle_and_names() {
        assert_eq!(DiagnosticMode::from_jni(1), DiagnosticMode::Solid);
        assert_eq!(DiagnosticMode::from_jni(2), DiagnosticMode::Triangle);
        assert_eq!(DiagnosticMode::from_jni(7), DiagnosticMode::None);
        assert_eq!(DiagnosticMode::Solid.name(), "SOLID");
        assert_eq!(DiagnosticMode::Solid.next(), DiagnosticMode::Triangle);
        assert_eq!(DiagnosticMode::Triangle.next(), DiagnosticMode::Solid);
        assert_eq!(DiagnosticMode::None.next(), DiagnosticMode::Solid);
    }

    #[test]
    fn shader_sources_are_nul_terminated_and_version_300() {
        assert_eq!(VERTEX_SOURCE.last(), Some(&0));
        assert_eq!(FRAGMENT_SOURCE.last(), Some(&0));
        assert_eq!(&FRAGMENT_SOURCE[..12], b"#version 300");
        assert_eq!(UNIFORM_ANGLE.last(), Some(&0));
    }

    #[test]
    fn solid_colour_matches_the_documented_centre_pixel() {
        // 0.62 * 255 = 158.1 and 0.65 * 255 = 165.75, i.e. the (0,158,166)
        // value the CI render test asserts on.
        assert_eq!((SOLID_R * 255.0) as u8, 0);
        assert_eq!((SOLID_G * 255.0) as u8, 158);
        assert_eq!((SOLID_B * 255.0) as u8, 165);
    }

    #[test]
    fn first_frame_log_is_emitted_exactly_once() {
        let mut renderer = DiagnosticRenderer::new(DiagnosticMode::Solid);
        let size = SurfaceSize::new(1080, 2400);
        let line = renderer.take_first_frame_log(size).expect("first frame log");
        assert_eq!(line, "First frame rendered (1080x2400, mode=SOLID)");
        assert!(renderer.take_first_frame_log(size).is_none());
    }

    #[test]
    fn stats_log_respects_the_one_second_window() {
        let mut renderer = DiagnosticRenderer::new(DiagnosticMode::Solid);
        let size = SurfaceSize::new(1080, 2400);
        assert!(renderer.take_stats_log(size).is_none(), "too early for stats");
        // Pretend a window has passed.
        renderer.last_stats = Instant::now() - std::time::Duration::from_millis(1500);
        renderer.frames_since_stats = 60;
        let line = renderer.take_stats_log(size).expect("stats after a window");
        assert!(line.contains("frames=60"), "unexpected line: {line}");
        assert!(line.contains("mode=SOLID"));
        assert!(line.contains("center_pixel_RGBA=("));
    }
}
