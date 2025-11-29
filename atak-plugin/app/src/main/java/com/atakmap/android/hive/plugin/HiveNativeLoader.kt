package com.atakmap.android.hive.plugin

import android.content.Context
import android.util.Log
import java.io.File

/**
 * Native library loader for HIVE plugin.
 *
 * Uses absolute paths for security as per ATAK SDK guidelines.
 */
object HiveNativeLoader {

    private const val TAG = "HiveNativeLoader"
    private var nativeLibDir: String? = null

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

    /**
     * Load a native library using absolute path.
     * Security guidance: Use validated absolute paths instead of System.loadLibrary().
     */
    fun loadLibrary(name: String) {
        val ndl = nativeLibDir ?: throw IllegalStateException("HiveNativeLoader not initialized")
        val libPath = ndl + File.separator + System.mapLibraryName(name)
        val libFile = File(libPath)

        if (libFile.exists()) {
            Log.d(TAG, "Loading native library: $libPath")
            System.load(libPath)
            Log.i(TAG, "Successfully loaded: $name")
        } else {
            Log.w(TAG, "Native library not found: $libPath")
            throw UnsatisfiedLinkError("Native library not found: $libPath")
        }
    }
}
