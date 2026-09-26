// Copyright 2026 The BoardBridge Authors
// Licensed under the Apache License, Version 2.0.

//! OpenGL ES FFI.
//!
//! Linked against **`libGLESv2.so`**: on Android the ES 3.x entry points are
//! exported by the same library that provides ES 2.0 (`libGLESv3.so` exists as
//! the compatibility stub a program links when it names ES 3 explicitly), and
//! `libGLESv2.so` is present on every Android device that can run a GL app.
//!
//! The diagnostics renderer uses a deliberately small subset — clear, a
//! two-attribute triangle program and a centre-pixel readback — because that was
//! the only part of the old demo worth keeping: it proves pixels reached the
//! framebuffer in a way logcat can verify.

use core::ffi::{c_char, c_void};

/// `GLenum`.
pub type GLenum = u32;
/// `GLuint`.
pub type GLuint = u32;
/// `GLint`.
pub type GLint = i32;
/// `GLsizei`.
pub type GLsizei = i32;
/// `GLboolean`.
pub type GLboolean = u8;
/// `GLfloat`.
pub type GLfloat = f32;
/// `GLbitfield`.
pub type GLbitfield = u32;
/// `GLubyte`.
pub type GLubyte = u8;
/// `GLchar`.
pub type GLchar = c_char;
/// `GLsizeiptr`.
pub type GLsizeiptr = isize;

/// `GL_NO_ERROR`.
pub const GL_NO_ERROR: GLenum = 0;
/// `GL_FALSE`.
pub const GL_FALSE: GLboolean = 0;
/// `GL_TRUE`.
pub const GL_TRUE: GLboolean = 1;

/// `GL_VENDOR`.
pub const GL_VENDOR: GLenum = 0x1F00;
/// `GL_RENDERER`.
pub const GL_RENDERER: GLenum = 0x1F01;
/// `GL_VERSION`.
pub const GL_VERSION: GLenum = 0x1F02;
/// `GL_EXTENSIONS`.
pub const GL_EXTENSIONS: GLenum = 0x1F03;
/// `GL_SHADING_LANGUAGE_VERSION`.
pub const GL_SHADING_LANGUAGE_VERSION: GLenum = 0x8B8C;
/// `GL_MAJOR_VERSION` (ES 3.0+).
pub const GL_MAJOR_VERSION: GLenum = 0x821B;
/// `GL_MINOR_VERSION` (ES 3.0+).
pub const GL_MINOR_VERSION: GLenum = 0x821C;

/// `GL_COLOR_BUFFER_BIT`.
pub const GL_COLOR_BUFFER_BIT: GLbitfield = 0x0000_4000;
/// `GL_DEPTH_BUFFER_BIT`.
pub const GL_DEPTH_BUFFER_BIT: GLbitfield = 0x0000_0100;
/// `GL_STENCIL_BUFFER_BIT`.
pub const GL_STENCIL_BUFFER_BIT: GLbitfield = 0x0000_0400;

/// `GL_RGBA`.
pub const GL_RGBA: GLenum = 0x1908;
/// `GL_RGB`.
pub const GL_RGB: GLenum = 0x1907;
/// `GL_UNSIGNED_BYTE`.
pub const GL_UNSIGNED_BYTE: GLenum = 0x1401;

/// `GL_VERTEX_SHADER`.
pub const GL_VERTEX_SHADER: GLenum = 0x8B31;
/// `GL_FRAGMENT_SHADER`.
pub const GL_FRAGMENT_SHADER: GLenum = 0x8B30;
/// `GL_COMPILE_STATUS`.
pub const GL_COMPILE_STATUS: GLenum = 0x8B81;
/// `GL_LINK_STATUS`.
pub const GL_LINK_STATUS: GLenum = 0x8B82;
/// `GL_INFO_LOG_LENGTH`.
pub const GL_INFO_LOG_LENGTH: GLenum = 0x8B84;

/// `GL_FLOAT`.
pub const GL_FLOAT: GLenum = 0x1406;
/// `GL_ARRAY_BUFFER`.
pub const GL_ARRAY_BUFFER: GLenum = 0x8892;
/// `GL_STATIC_DRAW`.
pub const GL_STATIC_DRAW: GLenum = 0x88E4;
/// `GL_TRIANGLES`.
pub const GL_TRIANGLES: GLenum = 0x0004;
/// `GL_BLEND`.
pub const GL_BLEND: GLenum = 0x0BE2;
/// `GL_DEPTH_TEST`.
pub const GL_DEPTH_TEST: GLenum = 0x0B71;
/// `GL_CULL_FACE`.
pub const GL_CULL_FACE: GLenum = 0x0B44;
/// `GL_SCISSOR_TEST`.
pub const GL_SCISSOR_TEST: GLenum = 0x0C11;
/// `GL_DITHER`.
pub const GL_DITHER: GLenum = 0x0BD0;

#[link(name = "GLESv2")]
extern "C" {
    /// Returns a version/limit string; null on error.
    pub fn glGetString(name: GLenum) -> *const GLubyte;
    /// Returns and clears the last GL error.
    pub fn glGetError() -> GLenum;
    /// Reads an integer state value.
    pub fn glGetIntegerv(pname: GLenum, params: *mut GLint);
    /// Sets the viewport rectangle.
    pub fn glViewport(x: GLint, y: GLint, width: GLsizei, height: GLsizei);
    /// Sets the clear colour.
    pub fn glClearColor(red: GLfloat, green: GLfloat, blue: GLfloat, alpha: GLfloat);
    /// Clears the given buffers.
    pub fn glClear(mask: GLbitfield);
    /// Reads pixels from the framebuffer.
    pub fn glReadPixels(
        x: GLint,
        y: GLint,
        width: GLsizei,
        height: GLsizei,
        format: GLenum,
        type_: GLenum,
        pixels: *mut c_void,
    );
    /// Waits for all queued commands to finish.
    pub fn glFinish();
    /// Enables a capability.
    pub fn glEnable(cap: GLenum);
    /// Disables a capability.
    pub fn glDisable(cap: GLenum);

    /// Creates a shader object.
    pub fn glCreateShader(type_: GLenum) -> GLuint;
    /// Supplies shader source.
    pub fn glShaderSource(
        shader: GLuint,
        count: GLsizei,
        string: *const *const GLchar,
        length: *const GLint,
    );
    /// Compiles a shader.
    pub fn glCompileShader(shader: GLuint);
    /// Reads shader state.
    pub fn glGetShaderiv(shader: GLuint, pname: GLenum, params: *mut GLint);
    /// Reads the shader info log.
    pub fn glGetShaderInfoLog(
        shader: GLuint,
        buf_size: GLsizei,
        length: *mut GLsizei,
        info_log: *mut GLchar,
    );
    /// Deletes a shader.
    pub fn glDeleteShader(shader: GLuint);

    /// Creates a program object.
    pub fn glCreateProgram() -> GLuint;
    /// Attaches a shader to a program.
    pub fn glAttachShader(program: GLuint, shader: GLuint);
    /// Links a program.
    pub fn glLinkProgram(program: GLuint);
    /// Reads program state.
    pub fn glGetProgramiv(program: GLuint, pname: GLenum, params: *mut GLint);
    /// Reads the program info log.
    pub fn glGetProgramInfoLog(
        program: GLuint,
        buf_size: GLsizei,
        length: *mut GLsizei,
        info_log: *mut GLchar,
    );
    /// Deletes a program.
    pub fn glDeleteProgram(program: GLuint);
    /// Installs a program for rendering.
    pub fn glUseProgram(program: GLuint);
    /// Looks up a uniform location.
    pub fn glGetUniformLocation(program: GLuint, name: *const GLchar) -> GLint;
    /// Sets a float uniform.
    pub fn glUniform1f(location: GLint, value: GLfloat);

    /// Generates vertex array objects (ES 3.0+).
    pub fn glGenVertexArrays(n: GLsizei, arrays: *mut GLuint);
    /// Binds a vertex array object.
    pub fn glBindVertexArray(array: GLuint);
    /// Deletes vertex array objects.
    pub fn glDeleteVertexArrays(n: GLsizei, arrays: *const GLuint);
    /// Generates buffer objects.
    pub fn glGenBuffers(n: GLsizei, buffers: *mut GLuint);
    /// Binds a buffer object.
    pub fn glBindBuffer(target: GLenum, buffer: GLuint);
    /// Uploads buffer data.
    pub fn glBufferData(target: GLenum, size: GLsizeiptr, data: *const c_void, usage: GLenum);
    /// Deletes buffer objects.
    pub fn glDeleteBuffers(n: GLsizei, buffers: *const GLuint);
    /// Describes a vertex attribute array.
    pub fn glVertexAttribPointer(
        index: GLuint,
        size: GLint,
        type_: GLenum,
        normalized: GLboolean,
        stride: GLsizei,
        pointer: *const c_void,
    );
    /// Enables a vertex attribute array.
    pub fn glEnableVertexAttribArray(index: GLuint);
    /// Draws primitives from arrays.
    pub fn glDrawArrays(mode: GLenum, first: GLint, count: GLsizei);
}
