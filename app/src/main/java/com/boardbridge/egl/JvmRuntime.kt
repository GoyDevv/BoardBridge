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
package com.boardbridge.egl

import android.content.Context
import android.os.Build
import android.util.Log
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import java.util.zip.ZipInputStream

/**
 * Extracts the bundled OpenJDK 21 runtime (per ABI) from assets to the app's
 * files dir and launches a headless "hello JVM" via [NativeBridge.runJvmHello].
 * A process may create only one JVM, so the launch is guarded to run once.
 */
object JvmRuntime {
    private const val TAG = "BoardBridge"
    private val launched = AtomicBoolean(false)

    fun launchHelloAsync(context: Context) {
        // JNI_CreateJavaVM can be called at most once per process; guard re-entry
        // (e.g. Activity recreation) so we never hit JNI_EEXIST.
        if (!launched.compareAndSet(false, true)) {
            Log.i(TAG, "JvmRuntime: JVM already launched for this process, skipping")
            return
        }
        val app = context.applicationContext
        Thread({
            try {
                Log.i(TAG, "JvmRuntime: thread started, abis=${Build.SUPPORTED_ABIS.joinToString()}")
                val t0 = System.currentTimeMillis()
                val jre = ensureJre(app)
                if (jre == null) {
                    Log.e(TAG, "JvmRuntime: no bundled JRE for ABIs ${Build.SUPPORTED_ABIS.joinToString()}")
                    return@Thread
                }
                Log.i(TAG, "JvmRuntime: JRE ready in ${System.currentTimeMillis() - t0}ms")
                val jar = copyAsset(app, "jvmtest/hello.jar", File(app.filesDir, "hello.jar"))
                val libDir = app.applicationInfo.nativeLibraryDir
                Log.i(TAG, "JvmRuntime: launching hello JVM (jre=${jre.absolutePath})")
                val rc = NativeBridge.runJvmHello(jre.absolutePath, jar.absolutePath, libDir)
                Log.i(TAG, "JvmRuntime: runJvmHello returned $rc")
            } catch (t: Throwable) {
                Log.e(TAG, "JvmRuntime: failed", t)
            }
        }, "jvm-hello").start()
    }

    private fun abiAssetName(): String? {
        for (abi in Build.SUPPORTED_ABIS) {
            when (abi) {
                "arm64-v8a" -> return "jre21/jre-arm64.zip"
                "x86_64" -> return "jre21/jre-x86_64.zip"
            }
        }
        return null
    }

    private fun ensureJre(context: Context): File? {
        val asset = abiAssetName() ?: return null
        val jreDir = File(context.filesDir, "jre")
        val marker = File(jreDir, ".extracted")
        if (marker.exists()) return jreDir
        if (jreDir.exists()) jreDir.deleteRecursively()
        jreDir.mkdirs()
        Log.i(TAG, "JvmRuntime: extracting $asset (first run)...")
        var count = 0
        val buffer = ByteArray(64 * 1024)
        context.assets.open(asset).use { raw ->
            ZipInputStream(BufferedInputStream(raw, 64 * 1024)).use { zis ->
                var entry = zis.nextEntry
                while (entry != null) {
                    val name = entry.name
                    // Cheap Zip-Slip guard (our controlled archive has no ".." entries).
                    if (name.contains("..")) throw SecurityException("bad zip entry: $name")
                    val out = File(jreDir, name)
                    if (entry.isDirectory) {
                        out.mkdirs()
                    } else {
                        out.parentFile?.mkdirs()
                        BufferedOutputStream(out.outputStream(), 64 * 1024).use { os ->
                            var n = zis.read(buffer)
                            while (n >= 0) {
                                os.write(buffer, 0, n)
                                n = zis.read(buffer)
                            }
                        }
                        count++
                    }
                    entry = zis.nextEntry
                }
            }
        }
        File(jreDir, "bin").listFiles()?.forEach { it.setExecutable(true, false) }
        marker.writeText("ok")
        Log.i(TAG, "JvmRuntime: extracted $count files to ${jreDir.absolutePath}")
        return jreDir
    }

    private fun copyAsset(context: Context, assetPath: String, dest: File): File {
        dest.parentFile?.mkdirs()
        context.assets.open(assetPath).use { input -> dest.outputStream().use { input.copyTo(it) } }
        return dest
    }
}
