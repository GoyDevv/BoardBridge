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
 * JNI glue for com.boardbridge.egl.NativeBridge. This is the native half of
 * BoardBridge's modernization of Boardwalk's Surface-to-GL binding: the raw
 * android.view.Surface is turned into an ANativeWindow and handed to a
 * dedicated native render thread that owns the EGL context.
 */
#include <jni.h>
#include <android/native_window.h>
#include <android/native_window_jni.h>

#include <memory>
#include <mutex>

#include "log.h"
#include "render_thread.h"

namespace {
std::unique_ptr<RenderThread> g_renderThread;
std::mutex g_mutex;

// Must be called with g_mutex held.
RenderThread* ensureRenderThreadLocked() {
    if (!g_renderThread) {
        g_renderThread = std::make_unique<RenderThread>();
        g_renderThread->start();
    }
    return g_renderThread.get();
}
}  // namespace

extern "C" {

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onSurfaceCreated(JNIEnv* env, jobject /*thiz*/,
                                                       jobject surface) {
    // Acquires one reference to the underlying ANativeWindow.
    ANativeWindow* window = ANativeWindow_fromSurface(env, surface);
    if (window == nullptr) {
        LOGE("ANativeWindow_fromSurface returned null");
        return;
    }
    std::lock_guard<std::mutex> lock(g_mutex);
    RenderThread* renderThread = ensureRenderThreadLocked();
    renderThread->setWindow(window);  // ownership transferred
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onSurfaceChanged(JNIEnv* /*env*/, jobject /*thiz*/,
                                                       jint width, jint height) {
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_renderThread) {
        g_renderThread->setSize(width, height);
    }
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onSurfaceDestroyed(JNIEnv* /*env*/, jobject /*thiz*/) {
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_renderThread) {
        g_renderThread->clearWindow();  // blocks until the window is released
    }
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onResume(JNIEnv* /*env*/, jobject /*thiz*/) {
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_renderThread) {
        g_renderThread->setPaused(false);
    }
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onPause(JNIEnv* /*env*/, jobject /*thiz*/) {
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_renderThread) {
        g_renderThread->setPaused(true);
    }
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onDestroy(JNIEnv* /*env*/, jobject /*thiz*/) {
    std::unique_ptr<RenderThread> toStop;
    {
        std::lock_guard<std::mutex> lock(g_mutex);
        toStop = std::move(g_renderThread);
    }
    // Join outside the lock so we never hold g_mutex while blocking.
    if (toStop) {
        toStop->requestStop();
    }
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onTouch(JNIEnv* /*env*/, jobject /*thiz*/, jint pointerId,
                                              jint action, jfloat x, jfloat y, jlong timeMs) {
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_renderThread) {
        g_renderThread->input().pushTouch(pointerId, action, x, y, timeMs);
    }
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onKey(JNIEnv* /*env*/, jobject /*thiz*/, jint keyCode,
                                            jint action, jint unicodeChar, jlong timeMs) {
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_renderThread) {
        g_renderThread->input().pushKey(keyCode, action, unicodeChar, timeMs);
    }
}

JNIEXPORT jstring JNICALL
Java_com_boardbridge_egl_NativeBridge_getRendererInfo(JNIEnv* env, jobject /*thiz*/) {
    std::string info;
    {
        std::lock_guard<std::mutex> lock(g_mutex);
        if (g_renderThread) {
            info = g_renderThread->rendererInfo();
        }
    }
    return env->NewStringUTF(info.c_str());
}

}  // extern "C"
