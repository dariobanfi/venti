package fly.venti.app.widget

import android.app.Activity
import android.os.Bundle
import android.widget.LinearLayout

/**
 * An empty screen for a UI test to lay the home-screen widget's views on:
 * a test cannot place a widget on the launcher. It is part of debug
 * builds only, and nothing in the app opens it.
 */
class WidgetStage : Activity() {
    lateinit var stage: LinearLayout
        private set

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        stage = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            // Clear of the status bar, as a launcher's widgets are.
            fitsSystemWindows = true
        }
        setContentView(stage)
    }
}
