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
 * Embeds a bundled OpenJDK 21 via the JNI Invocation API. This is the native
 * half of the "hello JVM" milestone: dlopen libjvm.so, JNI_CreateJavaVM, then
 * call a plain Java main(). The approach (stdio->logcat pipe, java.home/
 * classpath/library-path options) is standard for Android JVM launchers; the
 * implementation here is original.
 */
#include <jni.h>

#include <android/log.h>
#include <dlfcn.h>
#include <pthread.h>
#include <unistd.h>

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#define TAG "BoardBridge"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, TAG, __VA_ARGS__)
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, TAG, __VA_ARGS__)

namespace {

using CreateJavaVM_t = jint (*)(JavaVM**, void**, void*);

int g_pipe[2] = {-1, -1};

void* logcatPumpThread(void*) {
    char buffer[1024];
    ssize_t count;
    while ((count = read(g_pipe[0], buffer, sizeof(buffer) - 1)) > 0) {
        if (buffer[count - 1] == '\n') {
            --count;
        }
        buffer[count] = '\0';
        __android_log_print(ANDROID_LOG_INFO, "JVMOut", "%s", buffer);
    }
    return nullptr;
}

// Route the JVM's stdout/stderr into logcat under the "JVMOut" tag.
void redirectStdioToLogcat() {
    setvbuf(stdout, nullptr, _IONBF, 0);
    setvbuf(stderr, nullptr, _IONBF, 0);
    if (pipe(g_pipe) != 0) {
        LOGE("jvm: pipe() failed");
        return;
    }
    dup2(g_pipe[1], STDOUT_FILENO);
    dup2(g_pipe[1], STDERR_FILENO);
    pthread_t thread;
    if (pthread_create(&thread, nullptr, logcatPumpThread, nullptr) == 0) {
        pthread_detach(thread);
    }
}

}  // namespace

extern "C" JNIEXPORT jint JNICALL
Java_com_boardbridge_egl_NativeBridge_runJvmHello(JNIEnv* env, jobject /*thiz*/,
                                                  jstring jJreDir, jstring jClasspath,
                                                  jstring jLibDir) {
    const char* jreC = env->GetStringUTFChars(jJreDir, nullptr);
    const char* cpC = env->GetStringUTFChars(jClasspath, nullptr);
    const char* libC = env->GetStringUTFChars(jLibDir, nullptr);
    const std::string jreDir = jreC ? jreC : "";
    const std::string classpath = cpC ? cpC : "";
    const std::string libDir = libC ? libC : "";
    env->ReleaseStringUTFChars(jJreDir, jreC);
    env->ReleaseStringUTFChars(jClasspath, cpC);
    env->ReleaseStringUTFChars(jLibDir, libC);

    redirectStdioToLogcat();
    LOGI("jvm: launching, java.home=%s", jreDir.c_str());

    setenv("JAVA_HOME", jreDir.c_str(), 1);
    const std::string ldPath = jreDir + "/lib:" + jreDir + "/lib/server:" + libDir;
    setenv("LD_LIBRARY_PATH", ldPath.c_str(), 1);

    // Load the JLI helper first (some builds need it in the namespace), then libjvm.
    const std::string jliPath = jreDir + "/lib/libjli.so";
    const std::string jvmPath = jreDir + "/lib/server/libjvm.so";
    dlopen(jliPath.c_str(), RTLD_GLOBAL | RTLD_LAZY);
    void* jvmHandle = dlopen(jvmPath.c_str(), RTLD_GLOBAL | RTLD_NOW);
    if (jvmHandle == nullptr) {
        LOGE("jvm: dlopen(%s) failed: %s", jvmPath.c_str(), dlerror());
        return -1;
    }
    auto createJavaVM = reinterpret_cast<CreateJavaVM_t>(dlsym(jvmHandle, "JNI_CreateJavaVM"));
    if (createJavaVM == nullptr) {
        LOGE("jvm: dlsym(JNI_CreateJavaVM) failed: %s", dlerror());
        return -2;
    }

    // Preload core JRE libraries with RTLD_GLOBAL so the JVM's lazily-loaded
    // libraries (libnio -> libnet -> libjava, etc.) can resolve their DT_NEEDED
    // sonames from the global group. On Android the app's linker namespace does
    // not include the JRE lib dir, so LD_LIBRARY_PATH alone is insufficient.
    const char* preloadLibs[] = {"libverify.so", "libjava.so", "libjimage.so",
                                 "libzip.so", "libnet.so", "libnio.so"};
    for (const char* name : preloadLibs) {
        const std::string p = jreDir + "/lib/" + name;
        void* h = dlopen(p.c_str(), RTLD_GLOBAL | RTLD_NOW);
        if (h == nullptr) {
            LOGE("jvm: preload %s failed: %s", name, dlerror());
        } else {
            LOGI("jvm: preloaded %s", name);
        }
    }

    const std::string optHome = "-Djava.home=" + jreDir;
    const std::string optClasspath = "-Djava.class.path=" + classpath;
    const std::string optLibPath = "-Djava.library.path=" + libDir;
    const std::string optTmp = "-Djava.io.tmpdir=" + libDir;  // any writable dir; overridden below if needed

    std::vector<JavaVMOption> options;
    options.push_back({const_cast<char*>(optHome.c_str()), nullptr});
    options.push_back({const_cast<char*>(optClasspath.c_str()), nullptr});
    options.push_back({const_cast<char*>(optLibPath.c_str()), nullptr});
    options.push_back({const_cast<char*>("-Xmx128m"), nullptr});
    options.push_back({const_cast<char*>("-XX:+UseSerialGC"), nullptr});

    JavaVMInitArgs vmArgs;
    vmArgs.version = JNI_VERSION_1_6;
    vmArgs.nOptions = static_cast<jint>(options.size());
    vmArgs.options = options.data();
    vmArgs.ignoreUnrecognized = JNI_TRUE;

    JavaVM* vm = nullptr;
    JNIEnv* vmEnv = nullptr;
    jint rc = createJavaVM(&vm, reinterpret_cast<void**>(&vmEnv), &vmArgs);
    if (rc != JNI_OK || vm == nullptr) {
        LOGE("jvm: JNI_CreateJavaVM failed rc=%d", rc);
        return -3;
    }
    LOGI("jvm: JNI_CreateJavaVM OK");

    jint result = 0;
    jclass cls = vmEnv->FindClass("HelloJvm");
    if (cls == nullptr) {
        LOGE("jvm: FindClass(HelloJvm) failed");
        if (vmEnv->ExceptionCheck()) vmEnv->ExceptionDescribe();
        result = -4;
    } else {
        jmethodID mid = vmEnv->GetStaticMethodID(cls, "main", "([Ljava/lang/String;)V");
        if (mid == nullptr) {
            LOGE("jvm: no HelloJvm.main");
            result = -5;
        } else {
            jclass stringCls = vmEnv->FindClass("java/lang/String");
            jobjectArray argv = vmEnv->NewObjectArray(0, stringCls, nullptr);
            vmEnv->CallStaticVoidMethod(cls, mid, argv);
            if (vmEnv->ExceptionCheck()) {
                vmEnv->ExceptionDescribe();
                vmEnv->ExceptionClear();
                result = -6;
            }
            LOGI("jvm: HelloJvm.main returned");
        }
    }

    fflush(stdout);
    fflush(stderr);
    // Give the logcat pump a moment to flush before returning.
    usleep(200 * 1000);
    vm->DestroyJavaVM();
    LOGI("jvm: DestroyJavaVM done (result=%d)", result);
    return result;
}
