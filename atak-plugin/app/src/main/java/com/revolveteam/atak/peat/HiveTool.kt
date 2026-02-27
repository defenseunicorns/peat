/*
 * Copyright (c) 2026 (r)evolve - Revolve Team LLC.  All rights reserved.
 */

package com.revolveteam.atak.peat

import android.content.Context
import com.atak.plugins.impl.AbstractPluginTool
import gov.tak.api.util.Disposable

/**
 * PEAT Plugin Tool
 *
 * Toolbar button that opens the PEAT dropdown panel.
 */
class PeatTool(context: Context) : AbstractPluginTool(
    context,
    context.getString(R.string.app_name),
    context.getString(R.string.app_name),
    context.resources.getDrawable(R.drawable.ic_launcher, null),
    PeatDropDownReceiver.SHOW_PLUGIN
), Disposable {

    override fun dispose() {
        // Clean up resources if needed
    }
}
