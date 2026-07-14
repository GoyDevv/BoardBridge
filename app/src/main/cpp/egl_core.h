/*
 * Copyright 2026 The BoardBridge Authors
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
#pragma once

#include <EGL/egl.h>
#include <android/native_window.h>
#include <string>

/**
 * Owns the EGLDisplay / EGLConfig / EGLContext and the current window
 * EGLSurface. This is the native equivalent of the Surface -> EGLSurface
 * binding that Android's GLSurfaceView performs internally via its EglHelper,
 * but it targets OpenGL ES 3.x (Mali-G52 supports ES 3.2).
 *
 * Not thread-safe by itself: it is expected to be used from a single render
 * thread. The GLFW shim / game thread provides that guarantee.
 */
class EglCore {
public:
    EglCore() = default;
    ~EglCore();

    EglCore(const EglCore&) = delete;
    EglCore& operator=(const EglCore&) = delete;

    // Create display + config + an ES 3.x context (no surface yet).
    bool initialize();
    // Destroy everything (context, surface, display).
    void release();

    // Bind/unbind a window surface against the given ANativeWindow.
    bool createWindowSurface(ANativeWindow* window);
    void destroyWindowSurface();

    bool makeCurrent();
    void makeNothingCurrent();
    bool swapBuffers();
    void setSwapInterval(int interval);

    bool hasContext() const { return context_ != EGL_NO_CONTEXT; }
    bool hasSurface() const { return surface_ != EGL_NO_SURFACE; }

    int surfaceWidth() const;
    int surfaceHeight() const;

    // GL_VENDOR / GL_RENDERER / GL_VERSION; valid only after makeCurrent().
    std::string queryGlInfo() const;

private:
    EGLDisplay display_ = EGL_NO_DISPLAY;
    EGLConfig  config_  = nullptr;
    EGLContext context_ = EGL_NO_CONTEXT;
    EGLSurface surface_ = EGL_NO_SURFACE;
    EGLint     visualId_ = 0;
};
