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
#include "egl_core.h"

#include <GLES3/gl3.h>

#include "log.h"

EglCore::~EglCore() {
    release();
}

bool EglCore::initialize() {
    display_ = eglGetDisplay(EGL_DEFAULT_DISPLAY);
    if (display_ == EGL_NO_DISPLAY) {
        LOGE("eglGetDisplay(EGL_DEFAULT_DISPLAY) failed");
        return false;
    }

    EGLint major = 0, minor = 0;
    if (!eglInitialize(display_, &major, &minor)) {
        LOGE("eglInitialize failed: 0x%x", eglGetError());
        display_ = EGL_NO_DISPLAY;
        return false;
    }
    LOGI("EGL initialized: %d.%d", major, minor);

    const EGLint configAttribs[] = {
            EGL_RENDERABLE_TYPE, EGL_OPENGL_ES3_BIT,
            EGL_SURFACE_TYPE,    EGL_WINDOW_BIT,
            EGL_RED_SIZE,        8,
            EGL_GREEN_SIZE,      8,
            EGL_BLUE_SIZE,       8,
            EGL_ALPHA_SIZE,      8,
            EGL_DEPTH_SIZE,      24,
            EGL_STENCIL_SIZE,    8,
            EGL_NONE
    };
    EGLint numConfigs = 0;
    if (!eglChooseConfig(display_, configAttribs, &config_, 1, &numConfigs) || numConfigs < 1) {
        // Some drivers lack 24/8 depth-stencil; retry with a smaller depth buffer.
        const EGLint fallbackAttribs[] = {
                EGL_RENDERABLE_TYPE, EGL_OPENGL_ES3_BIT,
                EGL_SURFACE_TYPE,    EGL_WINDOW_BIT,
                EGL_RED_SIZE,        8,
                EGL_GREEN_SIZE,      8,
                EGL_BLUE_SIZE,       8,
                EGL_DEPTH_SIZE,      16,
                EGL_NONE
        };
        if (!eglChooseConfig(display_, fallbackAttribs, &config_, 1, &numConfigs) ||
            numConfigs < 1) {
            LOGE("eglChooseConfig failed: 0x%x", eglGetError());
            return false;
        }
    }
    eglGetConfigAttrib(display_, config_, EGL_NATIVE_VISUAL_ID, &visualId_);

    // Prefer ES 3.2 (Mali-G52), then fall back to 3.1 / 3.0.
    const EGLint minors[] = {2, 1, 0};
    for (EGLint m : minors) {
        const EGLint ctxAttribs[] = {
                EGL_CONTEXT_MAJOR_VERSION, 3,
                EGL_CONTEXT_MINOR_VERSION, m,
                EGL_NONE
        };
        context_ = eglCreateContext(display_, config_, EGL_NO_CONTEXT, ctxAttribs);
        if (context_ != EGL_NO_CONTEXT) {
            LOGI("Created OpenGL ES 3.%d context", m);
            break;
        }
    }
    if (context_ == EGL_NO_CONTEXT) {
        // Last resort for EGL 1.4 drivers without KHR_create_context minor support.
        const EGLint legacyAttribs[] = {EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE};
        context_ = eglCreateContext(display_, config_, EGL_NO_CONTEXT, legacyAttribs);
    }
    if (context_ == EGL_NO_CONTEXT) {
        LOGE("eglCreateContext failed: 0x%x", eglGetError());
        return false;
    }
    return true;
}

bool EglCore::createWindowSurface(ANativeWindow* window) {
    if (window == nullptr || display_ == EGL_NO_DISPLAY || context_ == EGL_NO_CONTEXT) {
        return false;
    }
    // Match the window's buffer format to the chosen EGLConfig.
    if (visualId_ != 0) {
        ANativeWindow_setBuffersGeometry(window, 0, 0, visualId_);
    }
    surface_ = eglCreateWindowSurface(display_, config_, window, nullptr);
    if (surface_ == EGL_NO_SURFACE) {
        LOGE("eglCreateWindowSurface failed: 0x%x", eglGetError());
        return false;
    }
    return true;
}

void EglCore::destroyWindowSurface() {
    if (surface_ != EGL_NO_SURFACE) {
        eglMakeCurrent(display_, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
        eglDestroySurface(display_, surface_);
        surface_ = EGL_NO_SURFACE;
    }
}

bool EglCore::makeCurrent() {
    if (surface_ == EGL_NO_SURFACE || context_ == EGL_NO_CONTEXT) {
        return false;
    }
    if (!eglMakeCurrent(display_, surface_, surface_, context_)) {
        LOGE("eglMakeCurrent failed: 0x%x", eglGetError());
        return false;
    }
    return true;
}

void EglCore::makeNothingCurrent() {
    if (display_ != EGL_NO_DISPLAY) {
        eglMakeCurrent(display_, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
    }
}

bool EglCore::swapBuffers() {
    if (surface_ == EGL_NO_SURFACE) {
        return false;
    }
    return eglSwapBuffers(display_, surface_) == EGL_TRUE;
}

void EglCore::setSwapInterval(int interval) {
    if (display_ != EGL_NO_DISPLAY) {
        eglSwapInterval(display_, interval);
    }
}

int EglCore::surfaceWidth() const {
    EGLint value = 0;
    if (surface_ != EGL_NO_SURFACE) {
        eglQuerySurface(display_, surface_, EGL_WIDTH, &value);
    }
    return value;
}

int EglCore::surfaceHeight() const {
    EGLint value = 0;
    if (surface_ != EGL_NO_SURFACE) {
        eglQuerySurface(display_, surface_, EGL_HEIGHT, &value);
    }
    return value;
}

std::string EglCore::queryGlInfo() const {
    const auto* vendor = reinterpret_cast<const char*>(glGetString(GL_VENDOR));
    const auto* renderer = reinterpret_cast<const char*>(glGetString(GL_RENDERER));
    const auto* version = reinterpret_cast<const char*>(glGetString(GL_VERSION));
    std::string info;
    info += "GL_VENDOR=";
    info += vendor ? vendor : "?";
    info += " | GL_RENDERER=";
    info += renderer ? renderer : "?";
    info += " | GL_VERSION=";
    info += version ? version : "?";
    return info;
}

void EglCore::release() {
    if (display_ != EGL_NO_DISPLAY) {
        eglMakeCurrent(display_, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
        if (surface_ != EGL_NO_SURFACE) {
            eglDestroySurface(display_, surface_);
            surface_ = EGL_NO_SURFACE;
        }
        if (context_ != EGL_NO_CONTEXT) {
            eglDestroyContext(display_, context_);
            context_ = EGL_NO_CONTEXT;
        }
        eglTerminate(display_);
        eglReleaseThread();
        display_ = EGL_NO_DISPLAY;
    }
    config_ = nullptr;
    visualId_ = 0;
}
