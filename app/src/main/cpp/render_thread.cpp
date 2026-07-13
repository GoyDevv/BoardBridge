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
#include "render_thread.h"

#include "log.h"

namespace {
// Fixed solid test color (linear RGBA8 window surface): ~ (0, 158, 166).
constexpr float kSolidR = 0.00f;
constexpr float kSolidG = 0.62f;
constexpr float kSolidB = 0.65f;

GLuint compileShader(GLenum type, const char* source) {
    GLuint shader = glCreateShader(type);
    glShaderSource(shader, 1, &source, nullptr);
    glCompileShader(shader);
    GLint ok = GL_FALSE;
    glGetShaderiv(shader, GL_COMPILE_STATUS, &ok);
    if (ok != GL_TRUE) {
        char buffer[512];
        glGetShaderInfoLog(shader, sizeof(buffer), nullptr, buffer);
        LOGE("Shader compile failed: %s", buffer);
        glDeleteShader(shader);
        return 0;
    }
    return shader;
}
}  // namespace

RenderThread::RenderThread() = default;

RenderThread::~RenderThread() {
    requestStop();
}

void RenderThread::start() {
    std::lock_guard<std::mutex> lock(mutex_);
    if (running_) {
        return;
    }
    running_ = true;
    thread_ = std::thread(&RenderThread::threadMain, this);
}

void RenderThread::requestStop() {
    {
        std::lock_guard<std::mutex> lock(mutex_);
        if (!running_ && !thread_.joinable()) {
            return;
        }
        running_ = false;
        cond_.notify_all();
    }
    if (thread_.joinable()) {
        thread_.join();
    }
}

void RenderThread::setWindow(ANativeWindow* window) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (!running_) {
        if (window != nullptr) {
            ANativeWindow_release(window);
        }
        return;
    }
    if (pendingWindow_ != nullptr && pendingWindow_ != window) {
        ANativeWindow_release(pendingWindow_);
    }
    pendingWindow_ = window;  // ownership transferred in
    windowChanged_ = true;
    paused_ = false;
    cond_.notify_all();
}

void RenderThread::clearWindow() {
    std::unique_lock<std::mutex> lock(mutex_);
    if (!running_) {
        if (currentWindow_ != nullptr) {
            ANativeWindow_release(currentWindow_);
            currentWindow_ = nullptr;
        }
        if (pendingWindow_ != nullptr) {
            ANativeWindow_release(pendingWindow_);
            pendingWindow_ = nullptr;
        }
        return;
    }
    if (pendingWindow_ != nullptr) {
        ANativeWindow_release(pendingWindow_);
        pendingWindow_ = nullptr;
        windowChanged_ = false;
    }
    clearRequested_ = true;
    clearAck_ = false;
    cond_.notify_all();
    cond_.wait(lock, [this] { return clearAck_ || !running_; });
}

void RenderThread::setSize(int width, int height) {
    std::lock_guard<std::mutex> lock(mutex_);
    width_ = width;
    height_ = height;
    cond_.notify_all();
}

void RenderThread::setPaused(bool paused) {
    std::lock_guard<std::mutex> lock(mutex_);
    paused_ = paused;
    cond_.notify_all();
}

std::string RenderThread::rendererInfo() {
    std::lock_guard<std::mutex> lock(infoMutex_);
    return rendererInfo_;
}

void RenderThread::toggleMode() {
    const int solid = static_cast<int>(RenderMode::Solid);
    const int triangle = static_cast<int>(RenderMode::Triangle);
    renderMode_.store(renderMode_.load() == solid ? triangle : solid);
}

const char* RenderThread::modeName() const {
    return renderMode_.load() == static_cast<int>(RenderMode::Solid) ? "SOLID" : "TRIANGLE";
}

void RenderThread::processWindowChangeLocked(std::unique_lock<std::mutex>& lock) {
    if (clearRequested_) {
        egl_.destroyWindowSurface();
        if (currentWindow_ != nullptr) {
            ANativeWindow_release(currentWindow_);
            currentWindow_ = nullptr;
        }
        clearRequested_ = false;
        clearAck_ = true;
        cond_.notify_all();  // wake the clearWindow() waiter
    }

    if (windowChanged_) {
        windowChanged_ = false;
        egl_.destroyWindowSurface();
        if (currentWindow_ != nullptr) {
            ANativeWindow_release(currentWindow_);
            currentWindow_ = nullptr;
        }
        ANativeWindow* window = pendingWindow_;
        pendingWindow_ = nullptr;
        currentWindow_ = window;  // take ownership

        if (window != nullptr) {
            lock.unlock();
            bool created = egl_.createWindowSurface(window);
            if (created && egl_.makeCurrent()) {
                std::string info = egl_.queryGlInfo();
                {
                    std::lock_guard<std::mutex> infoLock(infoMutex_);
                    rendererInfo_ = info;
                }
                LOGI("Surface bound. %s", info.c_str());
            } else {
                LOGE("Failed to bind window surface");
            }
            lock.lock();
        }
    }
}

void RenderThread::threadMain() {
    startTime_ = std::chrono::steady_clock::now();
    lastStatsTime_ = startTime_;
    frameCount_ = 0;
    framesSinceStats_ = 0;

    if (!egl_.initialize()) {
        LOGE("EGL initialization failed; render thread exiting");
        std::lock_guard<std::mutex> lock(mutex_);
        running_ = false;
        if (pendingWindow_ != nullptr) {
            ANativeWindow_release(pendingWindow_);
            pendingWindow_ = nullptr;
        }
        if (currentWindow_ != nullptr) {
            ANativeWindow_release(currentWindow_);
            currentWindow_ = nullptr;
        }
        return;
    }

    std::unique_lock<std::mutex> lock(mutex_);
    while (running_) {
        if (windowChanged_ || clearRequested_) {
            processWindowChangeLocked(lock);
        }
        if (!running_) {
            break;
        }
        const bool canRender = !paused_ && egl_.hasSurface();
        if (!canRender) {
            cond_.wait(lock);
            continue;
        }
        const int width = width_;
        const int height = height_;
        lock.unlock();

        drainInput();
        egl_.makeCurrent();
        drawFrame(width, height);
        egl_.swapBuffers();  // paces to the display refresh (vsync)

        lock.lock();
    }

    egl_.destroyWindowSurface();
    egl_.makeNothingCurrent();
    if (currentWindow_ != nullptr) {
        ANativeWindow_release(currentWindow_);
        currentWindow_ = nullptr;
    }
    if (pendingWindow_ != nullptr) {
        ANativeWindow_release(pendingWindow_);
        pendingWindow_ = nullptr;
    }
    lock.unlock();
    egl_.release();
}

void RenderThread::drainInput() {
    InputEvent e;
    while (input_.pop(e)) {
        // A primary DOWN (touch ACTION_DOWN == 0 or key ACTION_DOWN == 0)
        // toggles the render mode, so input visibly drives rendering.
        if (e.action == 0) {
            toggleMode();
            if (e.type == InputType::Touch) {
                LOGI("touch DOWN at (%.0f, %.0f) -> render mode = %s", e.x, e.y, modeName());
            } else {
                LOGI("key DOWN code=%d -> render mode = %s", e.code, modeName());
            }
        }
    }
}

bool RenderThread::initGl() {
    static const char* kVertexSrc =
            "#version 300 es\n"
            "layout(location = 0) in vec2 aPos;\n"
            "layout(location = 1) in vec3 aColor;\n"
            "uniform float uAngle;\n"
            "out vec3 vColor;\n"
            "void main() {\n"
            "  float c = cos(uAngle);\n"
            "  float s = sin(uAngle);\n"
            "  mat2 rot = mat2(c, -s, s, c);\n"
            "  gl_Position = vec4(rot * aPos, 0.0, 1.0);\n"
            "  vColor = aColor;\n"
            "}\n";
    static const char* kFragmentSrc =
            "#version 300 es\n"
            "precision mediump float;\n"
            "in vec3 vColor;\n"
            "out vec4 fragColor;\n"
            "void main() {\n"
            "  fragColor = vec4(vColor, 1.0);\n"
            "}\n";

    GLuint vs = compileShader(GL_VERTEX_SHADER, kVertexSrc);
    GLuint fs = compileShader(GL_FRAGMENT_SHADER, kFragmentSrc);
    if (vs == 0 || fs == 0) {
        return false;
    }
    program_ = glCreateProgram();
    glAttachShader(program_, vs);
    glAttachShader(program_, fs);
    glLinkProgram(program_);
    GLint linked = GL_FALSE;
    glGetProgramiv(program_, GL_LINK_STATUS, &linked);
    glDeleteShader(vs);
    glDeleteShader(fs);
    if (linked != GL_TRUE) {
        char buffer[512];
        glGetProgramInfoLog(program_, sizeof(buffer), nullptr, buffer);
        LOGE("Program link failed: %s", buffer);
        glDeleteProgram(program_);
        program_ = 0;
        return false;
    }
    uAngleLoc_ = glGetUniformLocation(program_, "uAngle");

    static const GLfloat vertices[] = {
            0.0f,  0.6f, 1.0f, 0.25f, 0.25f,
            -0.6f, -0.5f, 0.25f, 1.0f, 0.25f,
            0.6f, -0.5f, 0.25f, 0.25f, 1.0f,
    };
    glGenVertexArrays(1, &vao_);
    glGenBuffers(1, &vbo_);
    glBindVertexArray(vao_);
    glBindBuffer(GL_ARRAY_BUFFER, vbo_);
    glBufferData(GL_ARRAY_BUFFER, sizeof(vertices), vertices, GL_STATIC_DRAW);
    glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 5 * sizeof(GLfloat),
                          reinterpret_cast<void*>(0));
    glEnableVertexAttribArray(0);
    glVertexAttribPointer(1, 3, GL_FLOAT, GL_FALSE, 5 * sizeof(GLfloat),
                          reinterpret_cast<void*>(2 * sizeof(GLfloat)));
    glEnableVertexAttribArray(1);
    glBindVertexArray(0);
    return true;
}

void RenderThread::drawFrame(int width, int height) {
    if (!glTried_) {
        glOk_ = initGl();
        glTried_ = true;
    }
    if (width > 0 && height > 0) {
        glViewport(0, 0, width, height);
    }

    const bool triangle = renderMode_.load() == static_cast<int>(RenderMode::Triangle);
    if (triangle) {
        glClearColor(0.05f, 0.06f, 0.09f, 1.0f);
        glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);
        if (glOk_) {
            const float t = std::chrono::duration<float>(
                    std::chrono::steady_clock::now() - startTime_).count();
            glUseProgram(program_);
            glUniform1f(uAngleLoc_, t);
            glBindVertexArray(vao_);
            glDrawArrays(GL_TRIANGLES, 0, 3);
            glBindVertexArray(0);
        }
    } else {
        // SOLID render test: fixed color, cleared and swapped every frame.
        glClearColor(kSolidR, kSolidG, kSolidB, 1.0f);
        glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);
    }

    ++frameCount_;
    ++framesSinceStats_;
    if (frameCount_ == 1) {
        LOGI("First frame rendered (%dx%d, mode=%s)", width, height, modeName());
    }
    logFrameStatsIfDue(width, height);
}

void RenderThread::logFrameStatsIfDue(int width, int height) {
    const auto now = std::chrono::steady_clock::now();
    const float dt = std::chrono::duration<float>(now - lastStatsTime_).count();
    if (dt < 1.0f) {
        return;
    }
    GLubyte px[4] = {0, 0, 0, 0};
    if (width > 0 && height > 0) {
        // Read the back buffer (post-draw, pre-swap): the center pixel.
        glReadPixels(width / 2, height / 2, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, px);
    }
    const float fps = static_cast<float>(framesSinceStats_) / dt;
    LOGI("frames=%lld fps=%.1f mode=%s center_pixel_RGBA=(%d,%d,%d,%d)",
         frameCount_, fps, modeName(), px[0], px[1], px[2], px[3]);
    lastStatsTime_ = now;
    framesSinceStats_ = 0;
}
