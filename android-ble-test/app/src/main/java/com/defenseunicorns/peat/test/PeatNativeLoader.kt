/*
 * Copyright (c) 2026 Defense Unicorns.  All rights reserved.
 */

package com.defenseunicorns.peat.test

import android.content.Context
import android.util.Log
import java.io.File

/**
 * Native library loader for Peat BLE test app.
 *
 * Simplified from atak-plugin version — standalone app can use
 * System.loadLibrary directly since there's no linker namespace isolation.
 */
object PeatNativeLoader {

    private const val TAG = "PeatNativeLoader"
    private var nativeLibDir: String? = null
    private var libraryLoaded = false

    @Synchronized
    fun init(context: Context) {
        if (nativeLibDir == null) {
            try {
                nativeLibDir = context.packageManager
                    .getApplicationInfo(context.packageName, 0)
                    .nativeLibraryDir
                Log.d(TAG, "Native library directory: $nativeLibDir")
            } catch (e: Exception) {
                throw IllegalArgumentException(
                    "Failed to get native library directory: ${e.message}"
                )
            }
        }
    }

    fun isLibraryLoaded(): Boolean = libraryLoaded

    fun loadLibrary(name: String) {
        if (libraryLoaded) {
            Log.d(TAG, "Library already loaded")
            return
        }

        val ndl = nativeLibDir ?: throw IllegalStateException("PeatNativeLoader not initialized")

        // Load libc++_shared.so first if present
        val cppSharedPath = ndl + File.separator + "libc++_shared.so"
        if (File(cppSharedPath).exists()) {
            Log.d(TAG, "Loading C++ shared library: $cppSharedPath")
            try {
                System.load(cppSharedPath)
            } catch (e: UnsatisfiedLinkError) {
                Log.w(TAG, "Failed to load libc++_shared.so: ${e.message}")
            }
        }

        // Try System.loadLibrary first (standard for standalone apps)
        try {
            System.loadLibrary(name)
            libraryLoaded = true
            Log.i(TAG, "Loaded via System.loadLibrary: $name")
            return
        } catch (e: UnsatisfiedLinkError) {
            Log.d(TAG, "System.loadLibrary failed: ${e.message}, trying System.load")
        }

        // Fallback to absolute path
        val libPath = ndl + File.separator + System.mapLibraryName(name)
        val libFile = File(libPath)
        if (!libFile.exists()) {
            throw UnsatisfiedLinkError("Native library not found: $libPath")
        }

        System.load(libPath)
        libraryLoaded = true
        Log.i(TAG, "Loaded via System.load: $name")
    }
}
