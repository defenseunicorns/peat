package com.revolveteam.atak.hive

import android.content.Context
import com.atak.plugins.impl.AbstractPluginTool
import gov.tak.api.util.Disposable

/**
 * HIVE Plugin Tool
 *
 * Toolbar button that opens the HIVE dropdown panel.
 */
class HiveTool(context: Context) : AbstractPluginTool(
    context,
    context.getString(R.string.app_name),
    context.getString(R.string.app_name),
    context.resources.getDrawable(R.drawable.ic_launcher, null),
    HiveDropDownReceiver.SHOW_PLUGIN
), Disposable {

    override fun dispose() {
        // Clean up resources if needed
    }
}
