/*
 * Copyright (c) 2026 Defense Unicorns.  All rights reserved.
 */

package com.defenseunicorns.atak.peat

import android.content.Context
import android.util.Log
import java.io.File

/**
 * Native library loader for Peat plugin.
 *
 * Uses absolute paths for security as per ATAK SDK guidelines.
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

    fun getNativeLibDir(): String? = nativeLibDir

    fun isLibraryLoaded(): Boolean = libraryLoaded

    /**
     * Load native library.
     *
     * Tries multiple loading strategies to work around Android's linker namespace isolation:
     * 1. System.loadLibrary() - simplest, works if library is on java.library.path
     * 2. System.load() with absolute path - for security/explicit path loading
     */
    fun loadLibrary(name: String) {
        val ndl = nativeLibDir ?: throw IllegalStateException("PeatNativeLoader not initialized")
        val libPath = ndl + File.separator + System.mapLibraryName(name)
        val libFile = File(libPath)

        if (!libFile.exists()) {
            Log.w(TAG, "Native library not found: $libPath")
            throw UnsatisfiedLinkError("Native library not found: $libPath")
        }

        Log.d(TAG, "Loading native library from: $libPath")

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

        // Try System.loadLibrary first (may work better with JNI symbol lookup)
        try {
            System.loadLibrary(name)
            libraryLoaded = true
            Log.i(TAG, "Loaded via System.loadLibrary: $name")
            return
        } catch (e: UnsatisfiedLinkError) {
            Log.d(TAG, "System.loadLibrary failed: ${e.message}, trying System.load")
        }

        // Fallback to System.load with absolute path
        System.load(libPath)
        libraryLoaded = true
        Log.i(TAG, "Loaded via System.load: $name")
    }
}
