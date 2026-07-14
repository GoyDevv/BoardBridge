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
 * JNI glue for com.boardbridge.egl.NativeBridge. BoardBridge is now a PASSIVE
 * provider: these entry points only feed the bridge core (window/size/input/
 * lifecycle). The actual rendering runs on a separate game thread that drives
 * the GLFW shim (game_client_run) — the ownership model an LWJGL/Minecraft
 * launcher requires.
 */
#include <jni.h>
#include <android/native_window.h>
#include <android/native_window_jni.h>

#include <mutex>
#include <thread>

#include "bridge_core.h"
#include "game_client.h"
#include "log.h"

namespace {
std::mutex g_threadMutex;
std::thread g_gameThread;
bool g_started = false;
}  // namespace

extern "C" {

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_startClient(JNIEnv* /*env*/, jobject /*thiz*/) {
    std::lock_guard<std::mutex> lock(g_threadMutex);
    if (g_started) {
        return;
    }
    bridge::beginSession();  // clear any closing/paused state from a previous session
    g_started = true;
    LOGI("JNI startClient: spawning game thread");
    g_gameThread = std::thread(&game_client_run);
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_stopClient(JNIEnv* /*env*/, jobject /*thiz*/) {
    LOGI("JNI stopClient");
    bridge::requestClose();
    std::thread toJoin;
    {
        std::lock_guard<std::mutex> lock(g_threadMutex);
        if (g_gameThread.joinable()) {
            toJoin = std::move(g_gameThread);
        }
        g_started = false;
    }
    if (toJoin.joinable()) {
        toJoin.join();  // joined outside the lock
    }
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onSurfaceCreated(JNIEnv* env, jobject /*thiz*/,
                                                       jobject surface) {
    LOGI("JNI onSurfaceCreated");
    ANativeWindow* window = ANativeWindow_fromSurface(env, surface);
    if (window == nullptr) {
        LOGE("ANativeWindow_fromSurface returned null");
        return;
    }
    bridge::setWindow(window);  // ownership transferred to the bridge core
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onSurfaceChanged(JNIEnv* /*env*/, jobject /*thiz*/,
                                                       jint width, jint height) {
    LOGI("JNI onSurfaceChanged %dx%d", width, height);
    bridge::setSize(width, height);
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onSurfaceDestroyed(JNIEnv* /*env*/, jobject /*thiz*/) {
    LOGI("JNI onSurfaceDestroyed");
    bridge::clearWindow();  // blocks until the game thread frees its EGLSurface
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onResume(JNIEnv* /*env*/, jobject /*thiz*/) {
    LOGI("JNI onResume");
    bridge::setPaused(false);
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onPause(JNIEnv* /*env*/, jobject /*thiz*/) {
    LOGI("JNI onPause");
    bridge::setPaused(true);
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onTouch(JNIEnv* /*env*/, jobject /*thiz*/, jint pointerId,
                                              jint action, jfloat x, jfloat y, jlong timeMs) {
    bridge::pushTouch(pointerId, action, x, y, timeMs);
}

JNIEXPORT void JNICALL
Java_com_boardbridge_egl_NativeBridge_onKey(JNIEnv* /*env*/, jobject /*thiz*/, jint keyCode,
                                            jint action, jint unicodeChar, jlong timeMs) {
    bridge::pushKey(keyCode, action, unicodeChar, timeMs);
}

JNIEXPORT jstring JNICALL
Java_com_boardbridge_egl_NativeBridge_getRendererInfo(JNIEnv* env, jobject /*thiz*/) {
    char buffer[512];
    buffer[0] = '\0';
    bridge::getRendererInfo(buffer, sizeof(buffer));
    return env->NewStringUTF(buffer);
}

}  // extern "C"
