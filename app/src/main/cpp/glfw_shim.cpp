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
 *
 * A minimal GLFW 3.x ABI shim for Android, backed by the passive BoardBridge
 * core (ANativeWindow + input) and EGL. This implements the subset of GLFW that
 * LWJGL's org.lwjgl.glfw binding calls to create a context, swap buffers, load
 * GL entry points, and pump input — so LWJGL (and thus Minecraft) can drive
 * rendering onto an Android Surface. Compiled into libglfw.so.
 */
#define GLFW_INCLUDE_NONE
#include <GLFW/glfw3.h>

#include <EGL/egl.h>

#include <dlfcn.h>

#include <chrono>

#include "bridge_core.h"
#include "egl_core.h"
#include "log.h"

namespace {

void* g_glesLib = nullptr;  // lazy dlopen handle for core GL fallback

struct ShimWindow {
    bool shouldClose = false;
    int fbWidth = 0;
    int fbHeight = 0;

    GLFWkeyfun keyCb = nullptr;
    GLFWcharfun charCb = nullptr;
    GLFWcursorposfun cursorPosCb = nullptr;
    GLFWmousebuttonfun mouseButtonCb = nullptr;
    GLFWscrollfun scrollCb = nullptr;
    GLFWframebuffersizefun fbSizeCb = nullptr;
    GLFWwindowsizefun winSizeCb = nullptr;
};

ShimWindow g_window;
EglCore g_egl;
bool g_eglInitialized = false;
int64_t g_boundGeneration = -1;
GLFWerrorfun g_errorCb = nullptr;
GLFWvidmode g_vidmode = {0, 0, 8, 8, 8, 60};
std::chrono::steady_clock::time_point g_timeBase;
double g_cursorX = 0.0;
double g_cursorY = 0.0;
int g_mouseLeftState = GLFW_RELEASE;

GLFWwindow* asHandle() { return reinterpret_cast<GLFWwindow*>(&g_window); }

void reportError(int code, const char* desc) {
    LOGE("glfw shim error 0x%x: %s", code, desc);
    if (g_errorCb) g_errorCb(code, desc);
}

void updateFramebufferSize() {
    int w = g_egl.surfaceWidth();
    int h = g_egl.surfaceHeight();
    if (w <= 0 || h <= 0) {
        bridge::getSize(&w, &h);
    }
    if (w != g_window.fbWidth || h != g_window.fbHeight) {
        g_window.fbWidth = w;
        g_window.fbHeight = h;
        g_vidmode.width = w;
        g_vidmode.height = h;
        if (g_window.fbSizeCb) g_window.fbSizeCb(asHandle(), w, h);
        if (g_window.winSizeCb) g_window.winSizeCb(asHandle(), w, h);
    }
}

}  // namespace

extern "C" {

GLFWAPI int glfwInit(void) {
    g_timeBase = std::chrono::steady_clock::now();
    g_window.shouldClose = false;
    LOGI("glfw shim: glfwInit");
    return GLFW_TRUE;
}

GLFWAPI void glfwTerminate(void) {
    g_egl.destroyWindowSurface();
    bridge::consumerSurfaceAlive(false);
    g_egl.release();
    g_eglInitialized = false;
    LOGI("glfw shim: glfwTerminate");
}

GLFWAPI int glfwGetError(const char** description) {
    if (description) *description = nullptr;
    return GLFW_NO_ERROR;
}

GLFWAPI void glfwInitHint(int, int) {}
GLFWAPI void glfwDefaultWindowHints(void) {}
GLFWAPI void glfwWindowHint(int, int) {}
GLFWAPI void glfwWindowHintString(int, const char*) {}

GLFWAPI void glfwGetVersion(int* major, int* minor, int* rev) {
    if (major) *major = 3;
    if (minor) *minor = 4;
    if (rev) *rev = 0;
}

GLFWAPI const char* glfwGetVersionString(void) {
    return "3.4.0 BoardBridge-shim EGL/Android";
}

GLFWAPI GLFWwindow* glfwCreateWindow(int width, int height, const char* title,
                                     GLFWmonitor* /*monitor*/, GLFWwindow* /*share*/) {
    LOGI("glfw shim: glfwCreateWindow(%d, %d, \"%s\")", width, height, title ? title : "");
    if (!g_eglInitialized) {
        if (!g_egl.initialize()) {
            reportError(GLFW_PLATFORM_ERROR, "EGL initialize failed");
            return nullptr;
        }
        g_eglInitialized = true;
    }
    ANativeWindow* nativeWindow = bridge::consumerWaitWindow();
    if (nativeWindow == nullptr) {
        return nullptr;  // closing
    }
    if (!g_egl.createWindowSurface(nativeWindow)) {
        reportError(GLFW_PLATFORM_ERROR, "eglCreateWindowSurface failed");
        return nullptr;
    }
    bridge::consumerSurfaceAlive(true);
    g_boundGeneration = bridge::windowGeneration();
    g_window.shouldClose = false;
    g_egl.makeCurrent();
    updateFramebufferSize();
    return asHandle();
}

GLFWAPI void glfwDestroyWindow(GLFWwindow* /*window*/) {
    g_egl.destroyWindowSurface();
    bridge::consumerSurfaceAlive(false);
    g_window.shouldClose = true;
}

GLFWAPI int glfwWindowShouldClose(GLFWwindow* /*window*/) {
    return (g_window.shouldClose || bridge::shouldClose()) ? GLFW_TRUE : GLFW_FALSE;
}

GLFWAPI void glfwSetWindowShouldClose(GLFWwindow* /*window*/, int value) {
    g_window.shouldClose = (value != 0);
}

GLFWAPI void glfwMakeContextCurrent(GLFWwindow* window) {
    if (window == nullptr) {
        g_egl.makeNothingCurrent();
    } else {
        g_egl.makeCurrent();
    }
}

GLFWAPI GLFWwindow* glfwGetCurrentContext(void) {
    return g_egl.hasSurface() ? asHandle() : nullptr;
}

GLFWAPI void glfwSwapBuffers(GLFWwindow* /*window*/) {
    if (bridge::consumerShouldRelease(g_boundGeneration)) {
        // The surface changed (destroyed / paused / new window). Release ours.
        g_egl.destroyWindowSurface();
        bridge::consumerSurfaceAlive(false);
        if (bridge::shouldClose()) {
            g_window.shouldClose = true;
            return;
        }
        ANativeWindow* nativeWindow = bridge::consumerWaitWindow();
        if (nativeWindow == nullptr) {
            g_window.shouldClose = true;
            return;
        }
        if (g_egl.createWindowSurface(nativeWindow) && g_egl.makeCurrent()) {
            bridge::consumerSurfaceAlive(true);
            g_boundGeneration = bridge::windowGeneration();
            updateFramebufferSize();
            LOGI("glfw shim: rebound surface after generation change");
        } else {
            reportError(GLFW_PLATFORM_ERROR, "surface rebind failed");
            return;
        }
    }
    g_egl.swapBuffers();
}

GLFWAPI void glfwSwapInterval(int interval) {
    g_egl.setSwapInterval(interval);
}

GLFWAPI void glfwPollEvents(void) {
    bridge::Event e;
    GLFWwindow* handle = asHandle();
    while (bridge::pollEvent(&e)) {
        if (e.type == bridge::EventType::Touch) {
            g_cursorX = static_cast<double>(e.x);
            g_cursorY = static_cast<double>(e.y);
            if (g_window.cursorPosCb) g_window.cursorPosCb(handle, g_cursorX, g_cursorY);
            if (e.action == 0 /*ACTION_DOWN*/) {
                g_mouseLeftState = GLFW_PRESS;
                if (g_window.mouseButtonCb)
                    g_window.mouseButtonCb(handle, GLFW_MOUSE_BUTTON_LEFT, GLFW_PRESS, 0);
            } else if (e.action == 1 /*UP*/ || e.action == 3 /*CANCEL*/) {
                g_mouseLeftState = GLFW_RELEASE;
                if (g_window.mouseButtonCb)
                    g_window.mouseButtonCb(handle, GLFW_MOUSE_BUTTON_LEFT, GLFW_RELEASE, 0);
            }
        } else {  // Key
            int action = (e.action == 0) ? GLFW_PRESS : GLFW_RELEASE;
            if (g_window.keyCb) g_window.keyCb(handle, e.code, e.code, action, 0);
            if (action == GLFW_PRESS && e.unicode > 0 && g_window.charCb) {
                g_window.charCb(handle, static_cast<unsigned int>(e.unicode));
            }
        }
    }
    if (bridge::shouldClose()) g_window.shouldClose = true;
}

GLFWAPI void glfwWaitEvents(void) { glfwPollEvents(); }
GLFWAPI void glfwWaitEventsTimeout(double) { glfwPollEvents(); }
GLFWAPI void glfwPostEmptyEvent(void) {}

GLFWAPI GLFWglproc glfwGetProcAddress(const char* procname) {
    GLFWglproc proc = reinterpret_cast<GLFWglproc>(eglGetProcAddress(procname));
    if (proc == nullptr) {
        // Some drivers do not return core GL entry points via eglGetProcAddress;
        // fall back to dlsym from the GLES client libraries.
        if (g_glesLib == nullptr) {
            g_glesLib = dlopen("libGLESv3.so", RTLD_NOW | RTLD_LOCAL);
            if (g_glesLib == nullptr) {
                g_glesLib = dlopen("libGLESv2.so", RTLD_NOW | RTLD_LOCAL);
            }
        }
        if (g_glesLib != nullptr) {
            proc = reinterpret_cast<GLFWglproc>(dlsym(g_glesLib, procname));
        }
    }
    return proc;
}

GLFWAPI int glfwExtensionSupported(const char* /*extension*/) { return GLFW_FALSE; }

GLFWAPI void glfwGetFramebufferSize(GLFWwindow* /*window*/, int* width, int* height) {
    if (width) *width = g_window.fbWidth;
    if (height) *height = g_window.fbHeight;
}

GLFWAPI void glfwGetWindowSize(GLFWwindow* /*window*/, int* width, int* height) {
    if (width) *width = g_window.fbWidth;
    if (height) *height = g_window.fbHeight;
}

GLFWAPI void glfwSetWindowTitle(GLFWwindow*, const char*) {}
GLFWAPI void glfwShowWindow(GLFWwindow*) {}
GLFWAPI void glfwFocusWindow(GLFWwindow*) {}
GLFWAPI void glfwSetWindowSize(GLFWwindow*, int, int) {}

GLFWAPI GLFWmonitor* glfwGetPrimaryMonitor(void) {
    return reinterpret_cast<GLFWmonitor*>(&g_vidmode);
}

GLFWAPI const GLFWvidmode* glfwGetVideoMode(GLFWmonitor* /*monitor*/) {
    bridge::getSize(&g_vidmode.width, &g_vidmode.height);
    return &g_vidmode;
}

GLFWAPI double glfwGetTime(void) {
    return std::chrono::duration<double>(std::chrono::steady_clock::now() - g_timeBase).count();
}

GLFWAPI void glfwSetTime(double time) {
    g_timeBase = std::chrono::steady_clock::now() -
                 std::chrono::duration_cast<std::chrono::steady_clock::duration>(
                         std::chrono::duration<double>(time));
}

GLFWAPI int glfwGetKey(GLFWwindow*, int) { return GLFW_RELEASE; }
GLFWAPI int glfwGetMouseButton(GLFWwindow*, int button) {
    return (button == GLFW_MOUSE_BUTTON_LEFT) ? g_mouseLeftState : GLFW_RELEASE;
}
GLFWAPI void glfwGetCursorPos(GLFWwindow*, double* xpos, double* ypos) {
    if (xpos) *xpos = g_cursorX;
    if (ypos) *ypos = g_cursorY;
}
GLFWAPI void glfwSetInputMode(GLFWwindow*, int, int) {}
GLFWAPI int glfwGetInputMode(GLFWwindow*, int) { return 0; }

GLFWAPI GLFWerrorfun glfwSetErrorCallback(GLFWerrorfun cb) {
    GLFWerrorfun prev = g_errorCb;
    g_errorCb = cb;
    return prev;
}
GLFWAPI GLFWkeyfun glfwSetKeyCallback(GLFWwindow*, GLFWkeyfun cb) {
    GLFWkeyfun prev = g_window.keyCb;
    g_window.keyCb = cb;
    return prev;
}
GLFWAPI GLFWcharfun glfwSetCharCallback(GLFWwindow*, GLFWcharfun cb) {
    GLFWcharfun prev = g_window.charCb;
    g_window.charCb = cb;
    return prev;
}
GLFWAPI GLFWcursorposfun glfwSetCursorPosCallback(GLFWwindow*, GLFWcursorposfun cb) {
    GLFWcursorposfun prev = g_window.cursorPosCb;
    g_window.cursorPosCb = cb;
    return prev;
}
GLFWAPI GLFWmousebuttonfun glfwSetMouseButtonCallback(GLFWwindow*, GLFWmousebuttonfun cb) {
    GLFWmousebuttonfun prev = g_window.mouseButtonCb;
    g_window.mouseButtonCb = cb;
    return prev;
}
GLFWAPI GLFWscrollfun glfwSetScrollCallback(GLFWwindow*, GLFWscrollfun cb) {
    GLFWscrollfun prev = g_window.scrollCb;
    g_window.scrollCb = cb;
    return prev;
}
GLFWAPI GLFWframebuffersizefun glfwSetFramebufferSizeCallback(GLFWwindow*, GLFWframebuffersizefun cb) {
    GLFWframebuffersizefun prev = g_window.fbSizeCb;
    g_window.fbSizeCb = cb;
    return prev;
}
GLFWAPI GLFWwindowsizefun glfwSetWindowSizeCallback(GLFWwindow*, GLFWwindowsizefun cb) {
    GLFWwindowsizefun prev = g_window.winSizeCb;
    g_window.winSizeCb = cb;
    return prev;
}

}  // extern "C"
